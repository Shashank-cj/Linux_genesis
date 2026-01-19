import subprocess
import socket
import uuid
import json
from datetime import datetime
import os
import re
from sqlalchemy import create_engine, MetaData, Table, text
from sqlalchemy.orm import sessionmaker
import logging

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')

class AgentData:
 
    def __init__(self):
        self.timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")

        self.db_path = os.environ.get("GENESIS_AGENT_DB_PATH","/var/lib/genesis_agent/monitoring.db")
        db_key = os.environ.get("GENESIS_AGENT_DB_KEY")
        if not db_key:
            raise RuntimeError("GENESIS_AGENT_DB_KEY is not set")

        self.engine = create_engine(f"sqlite+pysqlcipher://:{db_key}@/{self.db_path}",echo=False,)
        
        with self.engine.connect() as conn:
            conn.execute(text("PRAGMA cipher_compatibility = 4;"))
            conn.execute(text(f"PRAGMA key = '{db_key}'"))
            conn.execute(text("SELECT count(*) FROM sqlite_master;"))

        Session = sessionmaker(bind=self.engine)
        self.session = Session()

        self.metadata = MetaData()
        try:
            self.metadata.reflect(bind=self.engine)
        except Exception as e:
            logging.warning("Could not reflect DB metadata: %s", e)

    # NEW: Add UUID lookup function like Windows code
    def get_uuid_by_name(self, table_name, name_field, name_value):
        if not name_value:
            logging.error(f"Missing name_value for field: {name_field}")
            return "unknown"

        try:
            table = Table(table_name, self.metadata, autoload_with=self.engine)
            result = self.session.query(table.c.uuid)\
                                .filter(getattr(table.c, name_field) == name_value).first()
            uuid = result[0] if result else "unknown"
            return uuid
        except Exception as e:
            logging.error(f"Error fetching UUID for {name_value}: {e}")
            return "unknown"

    def run_cmd(self, cmd):
        return subprocess.run(cmd, capture_output=True, text=True).stdout
    
    def is_virtual_machine(self, model, manufacturer):
        virtual_keywords = [
            "vmware", "virtualbox", "kvm", "xen", "qemu", "hyper-v", "microsoft corporation",
            "bochs", "parallels", "bhyve"
        ]
        combined = f"{model} {manufacturer}".lower()
        return any(kw in combined for kw in virtual_keywords)
    
    def get_device_make(self, devname):
        path = f"/sys/block/{devname}/device/vendor"
        try:
            if os.path.exists(path):
                with open(path) as f:
                    return f.read().strip()
        except Exception:
            pass
        return "Unknown"

    def convert_size_to_bytes(self, size_str):
        """Convert size string to bytes - handles all formats: '8192 M', '16G', '1Gbit/s'"""
        if not size_str or size_str == "Unknown":
            return 0
        
        size_str = size_str.strip().upper()
        size_str = size_str.replace("BIT/S", "").replace("/S", "")
        
        parts = size_str.split()
        if len(parts) == 2:
            try:
                number = float(parts[0])
                unit = parts[1]
            except:
                return 0
        # Try no-space format (lsblk: "20G")
        elif len(size_str) > 1 and size_str[-1].isalpha():
            try:
                number = float(size_str[:-1])
                unit = size_str[-1]
            except:
                return 0
        # Just a number
        else:
            try:
                return int(float(size_str))
            except:
                return 0
        
        # Convert based on unit
        if unit in ["K", "KB"]:
            return int(number * 1024)
        elif unit in ["M", "MB", "MBIT"]:
            return int(number * 1024 * 1024)
        elif unit in ["G", "GB", "GBIT"]:
            return int(number * 1024 * 1024 * 1024)
        elif unit in ["T", "TB", "TBIT"]:
            return int(number * 1024 * 1024 * 1024 * 1024)
        else:
            return int(number)  

    def get_device_info(self):
        try:
            make = self.run_cmd(["sudo", "dmidecode", "-s", "system-manufacturer"]).strip() or "Unknown"
            model = self.run_cmd(["sudo", "dmidecode", "-s", "system-product-name"]).strip() or "Unknown"
            serial = self.run_cmd(["sudo", "dmidecode", "-s", "system-serial-number"]).strip() or "Unknown"
            dev_type = "Virtual Machine" if self.is_virtual_machine(model, make) else "Physical Machine"
            return {
                "make": make,
                "model": model,
                "serial_number": self.run_cmd(["sudo", "dmidecode", "-s", "system-uuid"]).strip(),
                "dev_phy_vm": dev_type,
                "reboot_time":None
            }
        except:
            return {
                "make": "Unknown",
                "model": "Unknown",
                "serial_number": "Unknown",
                "dev_phy_vm": "Unknown",
                "reboot_time":"Unknown"
            }

    def get_cpu_info(self):
        cpu_raw = self.run_cmd(["lscpu"])
        cpu = {}
        
        for line in cpu_raw.splitlines():
            if "Model name" in line:
                cpu["model"] = line.split(":")[1].strip()
            elif "Vendor ID" in line:
                cpu["make"] = line.split(":")[1].strip()
            elif "Socket(s)" in line:
                cpu["p_cores"] = int(line.split(":")[1].strip())
            elif "CPU(s):" in line and "NUMA" not in line:
                cpu["l_cores"] = int(line.split(":")[1].strip())
            elif "CPU MHz" in line:    
                mhz_value = float(line.split(':')[1].strip())
                cpu["speed"] = int(mhz_value * 1_000_000) 
        
        if "speed" not in cpu:
            try:
                result = self.run_cmd(["grep", "MHz", "/proc/cpuinfo"])
                if result:
                    first_line = result.splitlines()[0]  
                    mhz = float(first_line.split(':')[1].strip())
                    cpu["speed"] = int(mhz * 1_000_000) 
            except:
                cpu["speed"] = 0
        
        cpu["os_uuid"] = self.run_cmd(["sudo", "dmidecode", "-s", "system-uuid"]).strip() 
        return [cpu]

    def get_memory_info(self):
        mem_raw = self.run_cmd(["sudo", "dmidecode", "-t", "memory"])
        memory = []
        module = {}
        inside_device = False
        system_uuid = self.run_cmd(["sudo", "dmidecode", "-s", "system-uuid"]).strip()

        for line in mem_raw.splitlines():
            line = line.strip()

            if line.startswith("Memory Device"):
                if module:
                    # Append the previous module before starting a new one
                    if module.get("size") and module["size"] != "No Module Installed":
                        module["os_uuid"] = system_uuid
                        memory.append(module)
                    module = {}
                inside_device = True

            elif inside_device:
                if line.startswith("Size"):
                    size_value = line.split(":", 1)[1].strip()
                    if "No Module Installed" in size_value:
                        module["size"] = "No Module Installed"
                    else:
                        module["size"] = self.convert_size_to_bytes(size_value)

                elif line.startswith("Speed") and "Configured" not in line:
                    module["speed"] = line.split(":", 1)[1].strip()

                elif line.startswith("Manufacturer"):
                    module["make"] = line.split(":", 1)[1].strip()

                elif line.startswith("Part Number"):
                    module["model"] = line.split(":", 1)[1].strip()

                elif line.startswith("Serial Number"):
                    serial = line.split(":", 1)[1].strip()
                    if not serial or serial.lower() in ["", "unknown", "none"]:
                        serial = "Not Specified"
                    module["serial_number"] = serial

            if not line and module:
                if module.get("size") and module["size"] != "No Module Installed":
                    module["os_uuid"] = system_uuid
                    memory.append(module)
                module = {}
                inside_device = False

        # Add last module if not already added
        if module and module.get("size") and module["size"] != "No Module Installed":
            module["os_uuid"] = system_uuid
            memory.append(module)

        # --- Remove duplicates and clean serial numbers ---
        unique_memory = []
        seen_serials = set()

        for mem in memory:
            serial = mem.get("serial_number") or "Not Specified"
            mem["serial_number"] = serial  # Ensure not empty

            if serial not in seen_serials:
                seen_serials.add(serial)
                unique_memory.append(mem)

        return unique_memory


    def get_partition_uuid(self, device_path):
        try:
            output = self.run_cmd(["sudo", "blkid", device_path])
            match = re.search(r'UUID="([^"]+)"', output)
            if match:
                return match.group(1)
            match = re.search(r'PTUUID="([^"]+)"', output)
            if match:
                return match.group(1)
        except Exception:
            return "Unknown"

    def get_storage_info(self, action=None):
        lsblk = self.run_cmd(["lsblk", "-b", "-J", "-o", "NAME,TYPE,SIZE,MODEL,SERIAL,FSTYPE,MOUNTPOINT,FSAVAIL,FSUSED,FSSIZE,UUID"])
        blk_data = json.loads(lsblk)
        storage = []
        
        for device in blk_data['blockdevices']:
            if device.get("type") != "disk":
                 continue

            has_partitions = bool(device.get("children"))
            has_fs = bool(device.get("fstype"))
            has_mount = bool(device.get("mountpoint"))

            if not (has_partitions or has_fs or has_mount):
                continue
                    
            if not device.get('mountpoint'):
                device_name = device["name"]
                device_path = f"/dev/{device_name}"
                
                # Use raw data directly - no conversion needed
                total_size_bytes = device.get("size", "0") or "0"
                allocated_size_bytes = 0
                
                for part in device.get("children", []):
                    # Use raw data directly
                    part_size_bytes = part.get("size", "0") or "0"
                    allocated_size_bytes += int(part_size_bytes) 

                unallocated_bytes = max(0, int(total_size_bytes) - allocated_size_bytes)
                
                disk = {
                    "os_uuid": self.get_partition_uuid(device_path),
                    "hw_disk_type": "sata",
                    "make": self.get_device_make(device_name),
                    "model": device.get("model", ""),
                    "serial_number": device.get("serial") or "not specified",
                    "base_fs_type": device.get("fstype") or "unknown",
                    "free_space": device.get("fsavail", "0") or "0",
                    "total_disk_usage": device.get("fsused", "0") or "0",
                    "total_disk_size": total_size_bytes,
                    "allocated_disk_size": str(allocated_size_bytes),     
                    "unallocated_disk_size": str(unallocated_bytes),  
                    "partition": []
                }

                if action == "disk" or action == "partition":
                    disk_uuid = self.get_partition_uuid(device_path)
                    if disk_uuid == "Unknown":
                        uuid = self.get_uuid_by_name("storage", "serial_number", device.get("serial", ""))
                    else:
                        uuid = self.get_uuid_by_name("storage", "os_uuid", disk_uuid)
                    disk["uuid"] = uuid

                for part in device.get("children", []):
                    if not part.get("mountpoint") and str(part.get("fstype") or "").upper() != "LVM2_MEMBER":
                        continue
                    part_path = f"/dev/{part.get('name')}"
                    fs_type = part.get("fstype", "") or "" 

                    if fs_type.upper() == "LVM2_MEMBER" and part.get("children"):
                        for lv in part.get("children", []):
                            if lv.get("mountpoint"):  
                                lv_fs_type = lv.get("fstype", "") or ""
                                # Use raw data directly
                                lv_free = lv.get("fsavail", "0") or "0"
                                lv_used = lv.get("fsused", "0") or "0"
                                lv_total = lv.get("fssize", "0") or lv.get("size", "0") or "0"
                                
                                partition_data = {
                                    "os_uuid": lv.get("uuid", ""),
                                    "name": lv.get("name", ""),
                                    "fs_type": lv_fs_type or "unknown",
                                    "serial_number": lv.get("uuid", ""),
                                    "free_space": lv_free,
                                    "used_space": lv_used,
                                    "total_size": lv_total
                                }
                                
                                if action == "disk" or action == "partition":
                                    uuid = self.get_uuid_by_name("partition", "os_uuid", lv.get("uuid", ""))
                                    partition_data["uuid"] = uuid
                                
                                disk["partition"].append(partition_data)
                    else:
                        if fs_type.upper() != "LVM2_MEMBER":
                            # Use raw data directly
                            free_space = part.get("fsavail", "0") or "0"
                            used_space = part.get("fsused", "0") or "0"
                            total_size = part.get("fssize", "0") or part.get("size", "0") or "0"

                            partition_data = {
                                "os_uuid": self.get_partition_uuid(part_path),
                                "name": part.get("name", ""),
                                "fs_type": fs_type or "unknown",
                                "serial_number": self.get_partition_uuid(part_path),
                                "free_space": free_space,
                                "used_space": used_space,
                                "total_size": total_size
                            }
                            
                            if action == "disk" or action == "partition":
                                uuid = self.get_uuid_by_name("partition", "os_uuid", self.get_partition_uuid(part_path))
                                partition_data["uuid"] = uuid

                            disk["partition"].append(partition_data)

                storage.append(disk)
        return storage


    def get_nic_system_uuid(self, interface_name):
        """Get unique system UUID for each network interface"""
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
            
            mac_path = f"/sy3s/class/net/{interface_name}/address"
            if os.path.exists(mac_path):
                with open(mac_path, 'r') as f:
                    mac = f.read().strip()
                    return mac.replace(":", "").upper()
                    
        except Exception as e:
            print(f"Error getting UUID for {interface_name}: {e}")
        return str(uuid.uuid4())
    
    def get_nic_info(self, action=None):
        result = self.run_cmd(["lshw", "-class", "network"])
        interfaces = []
        blocks = result.split("*-network")
        
        for block in blocks[1:]:
            lines = block.splitlines()
            nic = {}
            port = {}
            
            for line in lines:
                line = line.strip()
                if "product:" in line:
                    nic["model"] = line.split("product:")[1].strip()
                elif "vendor:" in line:
                    nic["make"] = line.split("vendor:")[1].strip()
                elif "serial:" in line:
                    nic["serial_number"] = line.split("serial:")[1].strip()
                    nic["mac_address"] = line.split("serial:")[1].strip()
                elif "logical name:" in line:
                    port["interface_name"] = line.split("logical name:")[1].strip()
                elif "capacity:" in line:
                    speed_str = line.split("capacity:")[1].strip()
                    nic["max_speed"] = self.convert_size_to_bytes(speed_str)
            
            if "max_speed" not in nic:
                nic["max_speed"] = 10000000000  
            
            interface_name = port.get("interface_name", "")
            nic["os_uuid"] = self.get_nic_system_uuid(interface_name)
            
            nic["number_of_ports"] = 1
            nic["supported_speeds"] = "1000, 2500"
            port["operating_speed"] = nic.get("max_speed", 0)
            port["is_physical_logical"] = "physical"
            port["logical_type"] = "bridge"
            port["ip"] = self.get_ip_for_interface(port["interface_name"])
            
            if action == "nic":
                nic["uuid"] = self.get_uuid_by_name("nic", "os_uuid", nic["os_uuid"])
                port["uuid"] = self.get_uuid_by_name("port", "interface_name", interface_name)
            nic["port"] = [port]
            interfaces.append(nic)
        
        return interfaces

    def get_ip_for_interface(self, iface):
        ip_info = self.run_cmd(["ip", "addr", "show", iface])
        route_info = self.run_cmd(["ip", "route", "show", "default"])
        resolv_conf = self.run_cmd(["cat", "/etc/resolv.conf"])

        ip_match = re.search(r"inet (\d+\.\d+\.\d+\.\d+)/(\d+)", ip_info)
        if ip_match:
            ip_addr = ip_match.group(1)
            subnet_prefix = int(ip_match.group(2))
            mask_bits = (0xffffffff >> (32 - subnet_prefix)) << (32 - subnet_prefix) 
            subnet_mask = ".".join([str((mask_bits >> (i * 8)) & 0xff) for i in range(3, -1, -1)])
        else:
            ip_addr = "Unknown"
            subnet_mask = "Unknown"

        gw_match = re.search(r"default via (\d+\.\d+\.\d+\.\d+)", route_info)
        gateway = gw_match.group(1) if gw_match else "Unknown"

        dns_match = re.findall(r"nameserver (\d+\.\d+\.\d+\.\d+)", resolv_conf)
        dns = dns_match if dns_match else ["Unknown"]

        return [{
            "address": ip_addr,
            "gateway": gateway,
            "subnet_mask": subnet_mask,
            "dns": ", ".join(dns)
        }]

    def get_gpu_info(self):
        result = self.run_cmd(["lshw", "-C", "display"])
        gpus = []
        gpu = {}
        for line in result.splitlines():
            line = line.strip()
            if "product:" in line:
                gpu["model"] = line.split("product:")[1].strip()
            elif "vendor:" in line:
                gpu["make"] = line.split("vendor:")[1].strip()
            elif "serial:" in line:
                gpu["serial_number"] = line.split("serial:")[1].strip()
                gpu["size"] = 1073741824  
                gpu["driver"] = "Unknown"
                gpus.append(gpu)
                gpu = {}
        return gpus


    def scan_particular_action(self, action):
        try:
            if action == "disk" or action == "partition":
                return {
                    "disk": self.get_storage_info(action)
                }
            elif action == "nic":
                return {
                    action: self.get_nic_info(action)
                }
        except Exception as e:
            logging.error(f"Error in scan_particular_action: {e}")
            return {}
    
    
    def inject_uuids(self, payload):
        if "device" not in payload:
            return payload

        device = payload["device"]

        serial = device.get("serial_number")
        if serial and serial != "Unknown":
            device["uuid"] = self.get_uuid_by_name("device", "serial_number", serial)
        else:
            device["uuid"] = "unknown"

        for cpu in device.get("cpu", []):
            os_uuid = cpu.get("os_uuid")
            if os_uuid:
                cpu["uuid"] = self.get_uuid_by_name("cpu", "os_uuid", os_uuid)
            else:
                cpu["uuid"] = "unknown"

        for mem in device.get("memory", []):
            os_uuid = mem.get("os_uuid")
            if os_uuid and os_uuid != "Not Specified":
                mem["uuid"] = self.get_uuid_by_name("memory", "os_uuid", os_uuid)
            else:
                mem["uuid"] = "unknown"

        for disk in device.get("storage", []):
            os_uuid = disk.get("os_uuid")
            serial = disk.get("serial_number", "")

            if os_uuid and os_uuid != "Unknown":
                disk["uuid"] = self.get_uuid_by_name("storage", "os_uuid", os_uuid)
            elif serial and serial != "not specified":
                disk["uuid"] = self.get_uuid_by_name("storage", "serial_number", serial)
            else:
                disk["uuid"] = "unknown"

            for part in disk.get("partition", []):
                part_os_uuid = part.get("os_uuid")
                part_serial = part.get("serial_number")

                if part_os_uuid and part_os_uuid != "Unknown":
                    part["uuid"] = self.get_uuid_by_name("partition", "os_uuid", part_os_uuid)
                elif part_serial and part_serial != "Unknown":
                    part["uuid"] = self.get_uuid_by_name("partition", "serial_number", part_serial)
                else:
                    part["uuid"] = "unknown"

        for nic in device.get("nic", []):
            os_uuid = nic.get("os_uuid")
            if os_uuid:
                nic["uuid"] = self.get_uuid_by_name("nic", "os_uuid", os_uuid)
            else:
                nic["uuid"] = "unknown"

            for port in nic.get("port", []):
                iface = port.get("interface_name")
                if iface:
                    port["uuid"] = self.get_uuid_by_name("port", "interface_name", iface)
                else:
                    port["uuid"] = "unknown"
                
                for ip in port.get("ip"):
                    address=ip.get("address")
                    if address:
                        ip["uuid"] = self.get_uuid_by_name("ip_address", "address", address)
                    else:
                        ip["uuid"] = "unknown"
                                 

        for gpu in device.get("gpu", []):
            serial = gpu.get("serial_number")
            if serial:
                gpu["uuid"] = self.get_uuid_by_name("gpu", "serial_number", serial)
            else:
                gpu["uuid"] = "unknown"

        return payload


 
    def collect_data(self):
        return {"device": self.get_full_info()["device"]}

    def get_full_info(self):
        return {
            "device": {
                **self.get_device_info(),
                "cpu": self.get_cpu_info(),
                "memory": self.get_memory_info(),
                "storage": self.get_storage_info(),
                "nic": self.get_nic_info(),
                "gpu": self.get_gpu_info()
            },
        }

    def get_reboot_time_if_valid(self):
        try:
            # install time from env (written during install)
            install_str = os.environ.get("GENESIS_AGENT_INSTALL_TIME")
            if not install_str:
                return None

            install_time = datetime.fromisoformat(install_str.replace("Z", "+00:00"))

            # system boot time
            with open("/proc/stat", "r") as f:
                for line in f:
                    if line.startswith("btime"):
                        boot_ts = int(line.split()[1])
                        reboot_time = datetime.fromtimestamp(boot_ts, tz=timezone.utc)

                        # ONLY if OS rebooted after install
                        if reboot_time > install_time:
                            return reboot_time.strftime("%Y-%m-%d %H:%M:%S")

                        return None
        except Exception:
            return None

        return None
        
    def reScan(self):
        logging.info("[RESCAN] Collecting data for update")

        data = self.collect_data()
        data = self.inject_uuids(data)

        # Send reboot_time ONLY if OS reboot happened after install
        data["device"]["reboot_time"] = self.get_reboot_time_if_valid()

        return data

if __name__ == "__main__":
    agent_data = AgentData()
