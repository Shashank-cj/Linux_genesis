import psutil
import json
import os
import time
import logging
import subprocess
import re
import threading
from datetime import datetime
from collections import defaultdict
from sqlalchemy import create_engine, MetaData, Table, text
from sqlalchemy.orm import sessionmaker

logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s")

TABLE_UUID_MAP = {
    "memory_monitoring": "memory",
    "cpu_monitoring": "cpu",
    "disk_monitoring": "storage",
    "partition_monitoring": "partition",
    "network_monitoring": "port",
    "device": "device",
}

class MonitoringLinux:
    def __init__(self, db_path=os.environ.get("GENESIS_AGENT_DB_PATH","/var/lib/genesis_agent/monitoring.db" )):
        self.db_path = os.path.abspath(db_path)
        db_key = os.environ.get("GENESIS_AGENT_DB_KEY")
        if not db_key:
            raise RuntimeError("GENESIS_AGENT_DB_KEY is not set")

        self.engine = create_engine(f"sqlite+pysqlcipher://:{db_key}@/{self.db_path}",echo=False,)

        with self.engine.connect() as conn:
            conn.execute(text("PRAGMA cipher_compatibility = 4"))
            conn.execute(text(f"PRAGMA key = '{db_key}'"))
            conn.execute(text("SELECT count(*) FROM sqlite_master"))

        Session = sessionmaker(bind=self.engine)
        self.session = Session()

        self.metadata = MetaData()
        try:
            self.metadata.reflect(bind=self.engine)
        except Exception as e:
            logging.warning("Could not reflect DB metadata: %s", e)
        try:
            self.last_mod_time = os.path.getmtime(self.db_path)
        except Exception:
            self.last_mod_time = 0

        # Command caching (to avoid running heavy commands repeatedly)
        self._cmd_cache = {}
        
        # NEW: Comprehensive caching system
        self._cpu_cache = {"data": None, "timestamp": 0, "ttl": 2}
        self._process_cache = {"data": None, "timestamp": 0, "ttl": 3}
        self._disk_cache = {"data": None, "timestamp": 0, "ttl": 5}
        self._network_cache = {"data": None, "timestamp": 0, "ttl": 2}
        self._partition_cache = {"data": None, "timestamp": 0, "ttl": 10}
        self._memory_cache = {"data": None, "timestamp": 0, "ttl": 5}

        self.uuid_cache = {}
        self.hardware_identifiers = {}
        self.disk_hardware_identifiers = {}
        self._prev_bytes = {}
        self._prev_disk_time = {}

        self._cpu_background_data = {"percpu": [], "overall": 0, "timestamp": 0}
        self._cpu_background_thread = None
        self._stop_background = False

        self.cache_hardware_identifiers()

        self.start_background_monitoring()

    def start_background_monitoring(self):
        """Start background thread for CPU monitoring"""
        def monitor_cpu():
            psutil.cpu_percent()
            psutil.cpu_percent(percpu=True)
            
            while not self._stop_background:
                try:
                    time.sleep(2)
                    overall = psutil.cpu_percent(interval=None)
                    percpu = psutil.cpu_percent(percpu=True, interval=None)
                    
                    self._cpu_background_data = {
                        "percpu": percpu,
                        "overall": overall,
                        "timestamp": time.time()
                    }
                except Exception as e:
                    logging.debug(f"Background CPU monitoring error: {e}")
                    time.sleep(1)
        
        self._cpu_background_thread = threading.Thread(target=monitor_cpu, daemon=True)
        self._cpu_background_thread.start()

    def get_cached_data(self, cache_key, fetch_function, *args, **kwargs):
        """Generic caching function"""
        cache = getattr(self, f"_{cache_key}_cache", None)
        if not cache:
            return fetch_function(*args, **kwargs)
        
        now = time.time()
        if cache["data"] is not None and (now - cache["timestamp"]) < cache["ttl"]:
            return cache["data"]
        
        # Fetch new data
        try:
            data = fetch_function(*args, **kwargs)
            cache["data"] = data
            cache["timestamp"] = now
            return data
        except Exception as e:
            logging.error(f"Error fetching {cache_key} data: {e}")
            return cache["data"] if cache["data"] is not None else {}

    def run_cmd_cached(self, cmd, cache_key=None, ttl=60):
        """Optimized command caching"""
        if cache_key is None:
            cache_key = " ".join(cmd)
        now = time.time()
        entry = self._cmd_cache.get(cache_key)
        if entry and now - entry["time"] < ttl:
            return entry["output"]

        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=5)
            output = result.stdout.strip() if result.returncode == 0 else ""
        except (subprocess.TimeoutExpired, Exception):
            output = ""
        
        self._cmd_cache[cache_key] = {"output": output, "time": now}
        return output

    def cache_hardware_identifiers(self):
        """Populate hardware identifiers using system UUID for CPU, memory, and device."""
        try:
            sys_uuid = self.run_cmd_cached(
                ["sudo", "dmidecode", "-s", "system-uuid"],
                "system_uuid",
                ttl=300
            ).strip()
            self.hardware_identifiers["serial_number"] = sys_uuid or "Unknown"
        except Exception as e:
            logging.debug("system uuid error: %s", e)
            self.hardware_identifiers["serial_number"] = "Unknown"
        
        try:
            nic_uuid_map = {}
            nic_list = os.listdir("/sys/class/net")
            for nic in nic_list:
                nic_uuid_map[nic] = self.get_nic_system_uuid(nic)
            self.hardware_identifiers["nic_uuid_map"] = nic_uuid_map
        except Exception as e:
            logging.debug("NIC UUID error: %s", e)
            self.hardware_identifiers["nic_uuid_map"] = {}

        try:
            disk_map = {}
            partuuid_map = {}
            out = self.run_cmd_cached(["lsblk", "-o", "NAME,UUID,PARTUUID"], "lsblk_uuid", ttl=300)
            for line in out.splitlines()[1:]:
                parts = line.split()
                if len(parts) >= 2:
                    disk_name = parts[0]
                    disk_uuid = parts[1] if len(parts) >= 2 else "N/A"
                    part_uuid = parts[2] if len(parts) >= 3 else "N/A"
                    disk_map[disk_name] = disk_uuid
                    partuuid_map[disk_name] = part_uuid
            self.disk_hardware_identifiers["disk_uuid_map"] = disk_map
            self.disk_hardware_identifiers["disk_partuuid_map"] = partuuid_map
        except Exception as e:
            logging.debug("disk uuid error: %s", e)
            self.disk_hardware_identifiers["disk_uuid_map"] = {}
            self.disk_hardware_identifiers["disk_partuuid_map"] = {}

    def load_batch_uuids(self):
        """Load commonly used UUIDs in batch"""
        if hasattr(self, '_batch_loaded') and self._batch_loaded:
            return
        
        try:
            # Pre-load device UUID
            device_serial = self.hardware_identifiers.get('serial_number', 'Unknown')
            self.get_uuid_by_name("device", "serial_number", device_serial)
            
            # Pre-load CPU and memory UUIDs
            self.get_uuid_by_name("cpu_monitoring", "os_uuid", device_serial)
            self.get_uuid_by_name("memory_monitoring", "os_uuid", device_serial)
            
            self._batch_loaded = True
        except Exception as e:
            logging.debug(f"Batch UUID loading error: {e}")

    def get_uuid_by_name(self, logical_table_name, name_field, name_value):
        """Generic lookup for uuid(s) in the local sqlite DB by a name field."""
        if not name_value:
            logging.warning("Missing name_value for %s", logical_table_name)
            return ("unknown", "unknown") if logical_table_name in ["partition_monitoring", "network_monitoring"] else "unknown"

        if self.has_db_changed():
            self.uuid_cache.clear()

        cache_key = (logical_table_name, name_value)
        if cache_key in self.uuid_cache:
            return self.uuid_cache[cache_key]

        table_name = TABLE_UUID_MAP.get(logical_table_name)
        if not table_name:
            return ("unknown", "unknown") if logical_table_name in ["partition_monitoring", "network_monitoring"] else "unknown"

        try:
            table = Table(table_name, self.metadata, autoload_with=self.engine)
            if logical_table_name == "partition_monitoring":
                res = self.session.query(table.c.uuid, table.c.storage_uuid).filter(getattr(table.c, name_field) == name_value).first()
                uuid, storage_uuid = (res.uuid, res.storage_uuid) if res else ("unknown", "unknown")
                self.uuid_cache[cache_key] = (uuid, storage_uuid)
                return uuid, storage_uuid
            elif logical_table_name == "network_monitoring":
                res = self.session.query(table.c.uuid, table.c.nic_uuid).filter(getattr(table.c, name_field) == name_value).first()
                uuid, nic_uuid = (res.uuid, res.nic_uuid) if res else ("unknown", "unknown")
                self.uuid_cache[cache_key] = (uuid, nic_uuid)
                return uuid, nic_uuid
            else:
                res = self.session.query(table.c.uuid).filter(getattr(table.c, name_field) == name_value).first()
                uuid = res[0] if res else "unknown"
                self.uuid_cache[cache_key] = uuid
                return uuid
        except Exception as e:
            logging.debug("Error fetching UUID for %s in %s: %s", name_value, logical_table_name, e)
            return ("unknown", "unknown") if logical_table_name in ["partition_monitoring", "network_monitoring"] else "unknown"

    def has_db_changed(self):
        """Return True if underlying sqlite DB file has changed since last check"""
        try:
            mod_time = os.path.getmtime(self.db_path)
        except Exception:
            mod_time = 0
        if mod_time != getattr(self, "last_mod_time", 0):
            self.last_mod_time = mod_time
            self.uuid_cache.clear()
            self._cmd_cache.clear()
            self.cache_hardware_identifiers()
            return True
        return False

    def get_monitoring_checkpoint(self):
        # Load batch UUIDs first
        self.load_batch_uuids()
        
        return {
            "device_uuid": self.get_uuid_by_name("device", "serial_number", self.hardware_identifiers.get('serial_number', 'Unknown')),
            "event_type": "MON_DATA",
            "description": "monitoring data",
            "memory_monitoring": self.get_cached_data("memory", self._get_memory_info),
            "cpu_monitoring": self.get_cached_data("cpu", self._get_cpu_info),
            "disk_monitoring": self.get_cached_data("disk", self._get_disk_info),
            "partition_monitoring": self.get_cached_data("partition", self._partition_monitoring),
            "network_monitoring": self.get_cached_data("network", self._network_monitoring),
            "memory_resource_monitoring": self.get_cached_process_data("memory", 10),
            "disk_resource_monitoring": self.get_cached_process_data("disk", 10),
            "cpu_resource_monitoring": self.get_cached_process_data("cpu", 10)
        }
    def get_cached_process_data(self, process_type, top_n):
        """Get cached process data"""
        cache = self._process_cache
        now = time.time()
        
        if cache["data"] is not None and (now - cache["timestamp"]) < cache["ttl"]:
            processes = cache["data"]
        else:
            # Fetch all process data once
            processes = self._get_all_process_data()
            cache["data"] = processes
            cache["timestamp"] = now
        
        # Return specific type
        if process_type == "cpu":
            return sorted(processes["cpu"], key=lambda x: x['cpu_average'] or 0, reverse=True)[:top_n]
        elif process_type == "memory":
            return sorted(processes["memory"], key=lambda x: x['working_set_kb'] or 0, reverse=True)[:top_n]
        elif process_type == "disk":
            return sorted(processes["disk"], key=lambda x: x['total_b_sec'] or 0, reverse=True)[:top_n]
        
        return []

    def _get_all_process_data(self):
        """Fetch all process data in one iteration"""
        cpu_uuid = self.get_uuid_by_name("cpu_monitoring", "os_uuid", self.hardware_identifiers.get('serial_number', 'Unknown'))
        memory_uuid = self.get_uuid_by_name("memory_monitoring", "os_uuid", self.hardware_identifiers.get('serial_number', 'Unknown'))
        
        cpu_apps = []
        mem_apps = []
        disk_apps = []
        
        for proc in psutil.process_iter(['pid', 'name', 'exe', 'status', 'num_threads', 'cpu_percent', 'username', 'memory_info', 'io_counters', 'cpu_times']):
            try:
                info = proc.info
                
                # CPU data
                cpu_apps.append({
                    'name': info.get('name'),
                    'pid': info.get('pid'),
                    'status': info.get('status'),
                    'threads': info.get('num_threads'),
                    'cpu_average': info.get('cpu_percent', 0),
                    'cpu_uuid': cpu_uuid
                })
                
                # Memory data
                mem_info = info.get('memory_info')
                if mem_info:
                    commit_kb = mem_info.vms // 1024
                    working_set_kb = mem_info.rss // 1024
                    private_kb = mem_info.rss // 1024
                else:
                    commit_kb = working_set_kb = private_kb = 0
                
                mem_apps.append({
                    'name': info.get('name'),
                    'pid': info.get('pid'),
                    'commit_kb': commit_kb,
                    'working_set_kb': working_set_kb,
                    'private_kb': private_kb,
                    'memory_uuid': memory_uuid
                })
                
                # Disk data
                io = info.get('io_counters')
                if io:
                    read_b_sec = io.read_bytes
                    write_b_sec = io.write_bytes
                    total_b_sec = read_b_sec + write_b_sec
                else:
                    read_b_sec = write_b_sec = total_b_sec = 0
                
                try:
                    ionice_info = proc.ionice()
                    io_priority_map = {0: 'None', 1: 'Real-time', 2: 'Best-effort', 3: 'Idle'}
                    io_priority = io_priority_map.get(ionice_info.ioclass, 'Unknown')
                except (AttributeError, psutil.AccessDenied):
                    io_priority = 'Unknown'
                
                cpu_times = info.get('cpu_times')
                response_time = cpu_times.user if cpu_times else 0
                exe_path = info.get('exe')
                disk_uuid = self.get_disk_uuid_for_path(exe_path) if exe_path else None
                
                disk_apps.append({
                    'name': info.get('name'),
                    'pid': info.get('pid'),
                    'file_path': exe_path,
                    'read_b_sec': read_b_sec,
                    'write_b_sec': write_b_sec,
                    'total_b_sec': total_b_sec,
                    'io_priority': io_priority,
                    'response_time': response_time,
                    'disk_uuid': disk_uuid
                })
                
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                continue
        
        return {"cpu": cpu_apps, "memory": mem_apps, "disk": disk_apps}

    def _get_memory_info(self):
        """Cached memory info function"""
        try:
            memory_info = psutil.virtual_memory()
            return {
                "memory_uuid": self.get_uuid_by_name("memory_monitoring", "os_uuid", self.hardware_identifiers.get('serial_number', 'Unknown')),
                "memory_used": memory_info.used,
                "memory_available": memory_info.available,
                "total_memory": memory_info.total,
                "memory_utilization": memory_info.percent,
            }
        except Exception as e:
            logging.error("get_memory_info error: %s", e)
            return {}

    def _get_cpu_info(self):
        """Cached CPU info using background data"""
        try:
            cpu_stats = psutil.cpu_stats()
            
            # Use background CPU data if available and recent
            bg_data = self._cpu_background_data
            now = time.time()
            
            if bg_data["timestamp"] > 0 and (now - bg_data["timestamp"]) < 5:
                logical_usages = bg_data["percpu"]
                overall_cpu = bg_data["overall"]
            else:
                # Fallback to instant reading (no interval)
                logical_usages = psutil.cpu_percent(percpu=True, interval=None)
                overall_cpu = psutil.cpu_percent(interval=None)
            
            core_count = psutil.cpu_count(logical=False) or 1
            logical_count = psutil.cpu_count(logical=True) or core_count

            physical_core_map = defaultdict(list)
            for i in range(min(logical_count, len(logical_usages))):
                p_core_index = i % core_count
                physical_core_map[p_core_index].append(logical_usages[i])

            physical_cores_usage = {
                f"physical_core_{i+1}": round(sum(usages) / len(usages), 2)
                for i, usages in physical_core_map.items()
            }

            return {
                "cpu_uuid": self.get_uuid_by_name("cpu_monitoring", "os_uuid", self.hardware_identifiers.get('serial_number', 'Unknown')),
                "p_cores_perc": physical_cores_usage,
                "l_cores_perc": {f"logical_core_{i+1}": usage for i, usage in enumerate(logical_usages)},
                "ctx_switches": getattr(cpu_stats, "ctx_switches", None),
                "sw_irq": getattr(cpu_stats, "soft_interrupts", None),
                "hw_irq": getattr(cpu_stats, "interrupts", None),
                "syscalls": getattr(cpu_stats, "syscalls", None),
                "cpu_utilization": overall_cpu,
            }
        except Exception as e:
            logging.error("get_cpu_info error: %s", e)
            return {}

    def _get_disk_info(self):
        """Cached disk info function"""
        now = time.time()
        result = []
        try:
            lsblk_json = self.run_cmd_cached(["lsblk", "-J", "-o", "NAME,TYPE,UUID,PTUUID"], "lsblk_json", ttl=60)
            lsblk_data = json.loads(lsblk_json) if lsblk_json else {"blockdevices": []}
            disk_uuid_map = {}
            for device in lsblk_data.get("blockdevices", []):
                if device.get("type") == "disk":
                    disk_name = device["name"]
                    disk_uuid = device.get("ptuuid") or device.get("uuid") or "unknown"
                    disk_uuid_map[disk_name] = disk_uuid
            
            physical_disks = {d["name"] for d in lsblk_data.get("blockdevices", []) if d.get("type") == "disk"}
            io_counters = psutil.disk_io_counters(perdisk=True)

            for disk_name, io in io_counters.items():
                if disk_name not in physical_disks:
                    continue
                disk_uuid = disk_uuid_map.get(disk_name, "unknown")
                current_busy_ms = (getattr(io, "read_time", 0) or 0) + (getattr(io, "write_time", 0) or 0)
                prev_busy_ms, prev_ts = self._prev_disk_time.get(disk_name, (current_busy_ms, now))
                delta_busy_ms = current_busy_ms - prev_busy_ms
                delta_t = max(now - prev_ts, 1e-6)
                util_pct = (delta_busy_ms / (delta_t * 1000)) * 100 if delta_t > 0 else 0.0
                self._prev_disk_time[disk_name] = (current_busy_ms, now)

                result.append({
                    "disk_uuid": self.get_uuid_by_name("disk_monitoring", "os_uuid", disk_uuid),
                    "read_count_io": getattr(io, "read_count", None),
                    "write_count_io": getattr(io, "write_count", None),
                    "bytes_read_io": getattr(io, "read_bytes", None),
                    "bytes_write_io": getattr(io, "write_bytes", None),
                    "read_time_io": getattr(io, "read_time", None),
                    "write_time_io": getattr(io, "write_time", None),
                    "disk_utilization": round(util_pct, 2),
                }) 
        except Exception as e:
            logging.error("Error in get_disk_info: %s", e)
        return result

    def _partition_monitoring(self):
        """Cached partition monitoring"""
        partitions_info = []
        try:
            lsblk_json = self.run_cmd_cached(["lsblk", "-J", "-o", "NAME,TYPE,MOUNTPOINT,FSTYPE,UUID,PTUUID"], "lsblk_partitions", ttl=60)
            lsblk_data = json.loads(lsblk_json) if lsblk_json else {"blockdevices": []}

            def build_partition_to_disk(devices):
                mapping = {}
                for dev in devices:
                    if dev.get("type") == "disk":
                        mapping[dev.get("name")] = dev.get("name")
                    if "children" in dev:
                        for child in dev["children"]:
                            mapping[child.get("name")] = dev.get("name")
                            if "children" in child:
                                for subchild in child["children"]:
                                    mapping[subchild.get("name")] = dev.get("name")
                return mapping

            def build_uuid_mappings(devices):
                disk_uuid_map = {}
                partition_uuid_map = {}
                
                for dev in devices:
                    if dev.get("type") == "disk":
                        disk_name = dev.get("name")
                        disk_uuid = dev.get("ptuuid") or dev.get("uuid") or "unknown"
                        disk_uuid_map[disk_name] = disk_uuid
                    
                    if "children" in dev:
                        for child in dev["children"]:
                            child_name = child.get("name")
                            partition_uuid = child.get("uuid") or "unknown"
                            partition_uuid_map[child_name] = partition_uuid
                            
                            if "children" in child:
                                for subchild in child["children"]:
                                    subchild_name = subchild.get("name")
                                    subpartition_uuid = subchild.get("uuid") or "unknown"
                                    partition_uuid_map[subchild_name] = subpartition_uuid
                
                return disk_uuid_map, partition_uuid_map

            partition_to_disk = build_partition_to_disk(lsblk_data.get("blockdevices", []))
            disk_uuid_map, partition_uuid_map = build_uuid_mappings(lsblk_data.get("blockdevices", []))

            valid_fs = {"ext2", "ext3", "ext4", "xfs", "btrfs"}
            skip_fs = {
                "tmpfs", "devtmpfs", "squashfs", "proc", "sysfs", "cgroup2", "securityfs",
                "pstore", "bpf", "autofs", "hugetlbfs", "mqueue", "debugfs", "tracefs",
                "fusectl", "configfs", "ramfs", "binfmt_misc", "nsfs"
            }

            for partition in psutil.disk_partitions(all=True):
                fstype = (partition.fstype or "").lower()
                if fstype in skip_fs or fstype not in valid_fs:
                    continue

                try:
                    usage = psutil.disk_usage(partition.mountpoint)
                except Exception:
                    continue

                partition_name = partition.device.split("/")[-1]
                disk_name = partition_to_disk.get(partition_name, "unknown")
                disk_uuid = disk_uuid_map.get(disk_name, "unknown")
                part_uuid = partition_uuid_map.get(partition_name, "unknown")
                uuid, storage_uuid = self.get_uuid_by_name("partition_monitoring", "os_uuid", part_uuid)
                partitions_info.append({
                    "partition_uuid": uuid, 
                    "disk_uuid": storage_uuid,        
                    "mount_point": partition.mountpoint,
                    "fstype": partition.fstype,
                    "free_space": usage.free,
                    "used_space": usage.used,
                    "used_space_perc": f"{usage.percent} %",
                })
        except Exception as e:
            logging.error("Error in partition_monitoring: %s", e)
        return partitions_info

    def _network_monitoring(self):
        """Cached network monitoring"""
        now = time.time()
        net_info = psutil.net_io_counters(pernic=True)
        net_stats = psutil.net_if_stats()
        network_data = []

        for iface, data in net_info.items():
            if iface == "lo":  
                continue
            stats = net_stats.get(iface)
            if not stats or not getattr(stats, "isup", False):
                continue

            prev_data = self._prev_bytes.get(iface, {
                "bytes_sent": data.bytes_sent,
                "bytes_recv": data.bytes_recv,
                "timestamp": now
            })

            delta_sent = data.bytes_sent - prev_data["bytes_sent"]
            delta_recv = data.bytes_recv - prev_data["bytes_recv"]
            delta_t = max(now - prev_data["timestamp"], 1.0)

            self._prev_bytes[iface] = {
                "bytes_sent": data.bytes_sent,
                "bytes_recv": data.bytes_recv,
                "timestamp": now
            }

            link_speed_mbps = getattr(stats, "speed", 1000) or 1000
            link_bps = link_speed_mbps * 1_000_000
            tx_bps = (delta_sent * 8) / delta_t
            rx_bps = (delta_recv * 8) / delta_t
            tx_util = (tx_bps / link_bps * 100) if link_bps else 0.0
            rx_util = (rx_bps / link_bps * 100) if link_bps else 0.0
            network_utilization = min(max(tx_util, rx_util), 100.0)
            uuid, nic_uuid = self.get_uuid_by_name("network_monitoring", "interface_name", iface)
            network_data.append({
                "port_uuid": uuid,
                "nic_uuid": nic_uuid,
                "bytes_sent": data.bytes_sent,
                "bytes_received": data.bytes_recv,
                "packets_sent": data.packets_sent,
                "packets_received": data.packets_recv,
                "error_in": getattr(data, "errin", None),
                "error_out": getattr(data, "errout", None),
                "drop_in": getattr(data, "dropin", None),
                "drop_out": getattr(data, "dropout", None),
                "network_utilization": round(network_utilization, 2),
            })
        return network_data

    def get_disk_uuid_for_path(self, file_path):
        """Helper function to get disk UUID for a given file path on Linux"""
        if not file_path or not os.path.exists(file_path):
            return None
        
        try:
            mount_point = None
            longest_match = 0
            
            for partition in psutil.disk_partitions():
                if file_path.startswith(partition.mountpoint):
                    if len(partition.mountpoint) > longest_match:
                        longest_match = len(partition.mountpoint)
                        mount_point = partition.mountpoint
            
            if mount_point:
                for partition in psutil.disk_partitions():
                    if partition.mountpoint == mount_point:
                        partition_name = partition.device.split("/")[-1]
                        uuid_result = self.get_uuid_by_name("partition_monitoring", "name", partition_name)
                        if isinstance(uuid_result, tuple):
                            return uuid_result[1]
                        return uuid_result
        except Exception:
            pass
        
        return None

    def get_nic_system_uuid(self, interface_name):
        """Get unique system UUID for each network interface"""
        if not interface_name:
            return "unknown"
        
        try:
            device_path = f"/sys/class/net/{interface_name}/device"
            if os.path.exists(device_path):
                pci_path = os.readlink(device_path)
                if "pci" in pci_path:
                    pci_address = pci_path.split("/")[-1]
                    if pci_address:
                        return pci_address

            uevent_path = f"/sys/class/net/{interface_name}/device/uevent"
            if os.path.exists(uevent_path):
                with open(uevent_path, 'r') as f:
                    for line in f:
                        if "PCI_SLOT_NAME=" in line:
                            return line.split("=")[1].strip()
            
            mac_path = f"/sys/class/net/{interface_name}/address"
            if os.path.exists(mac_path):
                with open(mac_path, 'r') as f:
                    mac = f.read().strip()
                    return mac.replace(":", "").upper()
                    
        except Exception as e:
            logging.debug(f"Error getting UUID for {interface_name}: {e}")
        return "unknown"
  
    def __del__(self):
        """Cleanup background thread"""
        self._stop_background = True
        if self._cpu_background_thread and self._cpu_background_thread.is_alive():
            self._cpu_background_thread.join(timeout=1)

if __name__ == "__main__":
    monitoring = MonitoringLinux()
    while True:
        start_time = time.time()
        checkpoint = monitoring.get_monitoring_checkpoint()
        end_time = time.time()
        print(f"Collection time: {end_time - start_time:.2f} seconds")
        print(json.dumps(checkpoint, indent=4))
        time.sleep(1) 