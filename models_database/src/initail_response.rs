use diesel::prelude::*;
use diesel::result::Error;
use serde_json::Value;
use crate::models::*;
use crate::schema::*;
use anyhow::{Result};

pub fn store_json_data(conn: &mut SqliteConnection, json_data: &Value) -> Result<(), Error> {
    conn.transaction::<_, Error, _>(|conn| {
        println!("Storing JSON data into the database...");
        println!("JSON data: {:?}", json_data.to_string());
        // Insert Agent
        if let Some(agent_value) = json_data.get("agent") {
            let agent: Agent = serde_json::from_value(agent_value.clone())
                .map_err(|e| {
                    println!("Failed to parse agent: {e}");
                    Error::RollbackTransaction
                })?;
            diesel::insert_into(agent::table).values(&agent).execute(conn)
                .map_err(|e| {
                    println!("Failed to insert into agent table: {e}");
                    Error::RollbackTransaction
                })?;
        }

        // Insert Device and Related Tables
        if let Some(device_value) = json_data.get("device") {
            let device: Device = serde_json::from_value(device_value.clone())
                .map_err(|e| {
                    println!("Failed to parse device: {e}");
                    Error::RollbackTransaction
                })?;
            diesel::insert_into(device::table).values(&device).execute(conn)
                .map_err(|e| {
                    println!("Failed to insert into device table: {e}");
                    Error::RollbackTransaction
                })?;

            let device_uuid = device.uuid.clone();

            // CPU
            if let Some(cpu_array) = device_value.get("cpu").and_then(|v| v.as_array()) {
                for c in cpu_array {
                    let mut cpu: Cpu = serde_json::from_value(c.clone()).map_err(|e| {
                        println!("Failed to parse CPU: {e}");
                        Error::RollbackTransaction
                    })?;
                    cpu.device_uuid = device_uuid.clone();
                    diesel::insert_into(cpu::table).values(&cpu).execute(conn).map_err(|e| {
                        println!("Failed to insert into cpu table: {e}");
                        Error::RollbackTransaction
                    })?;
                }
            }

            // Memory
            if let Some(mem_array) = device_value.get("memory").and_then(|v| v.as_array()) {
                for m in mem_array {
                    let mut memory: Memory = serde_json::from_value(m.clone()).map_err(|e| {
                        println!("Failed to parse memory: {e}");
                        Error::RollbackTransaction
                    })?;
                    memory.device_uuid = device_uuid.clone();
                    diesel::insert_into(memory::table).values(&memory).execute(conn).map_err(|e| {
                        println!("Failed to insert into memory table: {e}");
                        Error::RollbackTransaction
                    })?;
                }
            }

            // Storage + Partition
            if let Some(stor_array) = device_value.get("storage").and_then(|v| v.as_array()) {
                for s in stor_array {
                    let mut storage: Storage = serde_json::from_value(s.clone()).map_err(|e| {
                        println!("Failed to parse storage: {e}");
                        Error::RollbackTransaction
                    })?;
                    storage.device_uuid = device_uuid.clone();
                    let storage_uuid = storage.uuid.clone();
                    diesel::insert_into(storage::table).values(&storage).execute(conn).map_err(|e| {
                        println!("Failed to insert into storage table: {e}");
                        Error::RollbackTransaction
                    })?;

                    if let Some(part_array) = s.get("partition").and_then(|v| v.as_array()) {
                        for p in part_array {
                            let mut partition: Partition = serde_json::from_value(p.clone()).map_err(|e| {
                                println!("Failed to parse partition: {e}");
                                Error::RollbackTransaction
                            })?;
                            partition.storage_uuid = storage_uuid.clone();
                            diesel::insert_into(partition::table).values(&partition).execute(conn).map_err(|e| {
                                println!("Failed to insert into partition table: {e}");
                                Error::RollbackTransaction
                            })?;
                        }
                    }
                }
            }

            // NIC + Port + IP
            if let Some(nics) = device_value.get("nic").and_then(|v| v.as_array()) {
                for n in nics {
                    let mut nic: Nic = serde_json::from_value(n.clone()).map_err(|e| {
                        println!("Failed to parse NIC: {e}");
                        Error::RollbackTransaction
                    })?;
                    nic.device_uuid = device_uuid.clone();
                    let nic_uuid = nic.uuid.clone();
                    diesel::insert_into(nic::table).values(&nic).execute(conn).map_err(|e| {
                        println!("Failed to insert into nic table: {e}");
                        Error::RollbackTransaction
                    })?;

                    if let Some(port_array) = n.get("port").and_then(|v| v.as_array()) {
                        for port_v in port_array {
                            let mut port: Port = serde_json::from_value(port_v.clone()).map_err(|e| {
                                println!("Failed to parse port: {e}");
                                Error::RollbackTransaction
                            })?;
                            port.nic_uuid = nic_uuid.clone();
                            let port_uuid = port.uuid.clone();
                            diesel::insert_into(port::table).values(&port).execute(conn).map_err(|e| {
                                println!("Failed to insert into port table: {e}");
                                Error::RollbackTransaction
                            })?;

                            if let Some(ip_array) = port_v.get("ip").and_then(|v| v.as_array()) {
                                for ip_v in ip_array {
                                    let mut ip: Ip = serde_json::from_value(ip_v.clone()).map_err(|e| {
                                        println!("Failed to parse IP: {e}");
                                        Error::RollbackTransaction
                                    })?;
                                    ip.port_uuid = port_uuid.clone();
                                    diesel::insert_into(ip_address::table).values(&ip).execute(conn).map_err(|e| {
                                        println!("Failed to insert into ip_address table: {e}");
                                        Error::RollbackTransaction
                                    })?;
                                }
                            }
                        }
                    }
                }
            }

            // GPU
            if let Some(gpus) = device_value.get("gpu").and_then(|v| v.as_array()) {
                for g in gpus {
                    let mut gpu: Gpu = serde_json::from_value(g.clone()).map_err(|e| {
                        println!("Failed to parse GPU: {e}");
                        Error::RollbackTransaction
                    })?;
                    gpu.device_uuid = device_uuid.clone();
                    diesel::insert_into(gpu::table).values(&gpu).execute(conn).map_err(|e| {
                        println!("Failed to insert into gpu table: {e}");
                        Error::RollbackTransaction
                    })?;
                }
            }
        }

        println!("JSON data stored successfully.");
        Ok(())
    })
}


pub fn insert_or_update(conn: &mut SqliteConnection, device_values: &[Value]) -> Result<(), Error> {
    conn.transaction::<_, Error, _>(|conn| {
        println!("Storing JSON data into the database...");
        println!("All device_values: {:#?}", device_values);

        for device_value in device_values {
            let device_uuid = device_value
                .get("device_uuid")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            // === STORAGE HANDLING ===
            if let Some(storage_value) = device_value.get("storage") {
                println!("Processing storage data: {:?}", storage_value);

                let mut storage: Storage = serde_json::from_value(storage_value.clone()).map_err(|e| {
                    println!("Failed to parse storage: {e}");
                    Error::RollbackTransaction
                })?;
                storage.device_uuid = device_uuid.clone();
                let storage_uuid = storage.uuid.clone();

                let existing_storage = storage::table
                    .filter(storage::uuid.eq(&storage_uuid))
                    .first::<Storage>(conn)
                    .optional()
                    .map_err(|e| {
                        println!("Failed to query storage: {e}");
                        Error::RollbackTransaction
                    })?;

                if existing_storage.is_none() {
                    diesel::insert_into(storage::table)
                        .values(&storage)
                        .execute(conn)
                        .map_err(|e| {
                            println!("Failed to insert storage: {e}");
                            Error::RollbackTransaction
                        })?;
                    println!("Inserted storage: {}", storage_uuid);
                } else {
                    diesel::update(storage::table.filter(storage::uuid.eq(&storage_uuid)))
                        .set(&storage)
                        .execute(conn)
                        .map_err(|e| {
                            println!("Failed to update storage: {e}");
                            Error::RollbackTransaction
                        })?;
                    println!("Updated storage: {}", storage_uuid);
                }

                // === PARTITIONS ===
                if let Some(partitions) = storage_value.get("partition").and_then(|v| v.as_array()) {
                    if partitions.is_empty() {
                        println!("No partitions found for storage {}, skipping partition insertions.", storage_uuid);
                        continue;
                    }

                    for part in partitions {
                        let mut partition: Partition = serde_json::from_value(part.clone()).map_err(|e| {
                            println!("Failed to parse partition: {e}");
                            Error::RollbackTransaction
                        })?;
                        partition.storage_uuid = storage_uuid.clone();

                        let partition_uuid = partition.uuid.clone();
                        let existing_partition = partition::table
                            .filter(partition::uuid.eq(&partition_uuid))
                            .first::<Partition>(conn)
                            .optional()
                            .map_err(|e| {
                                println!("Failed to query partition: {e}");
                                Error::RollbackTransaction
                            })?;

                        if existing_partition.is_none() {
                            diesel::insert_into(partition::table)
                                .values(&partition)
                                .execute(conn)
                                .map_err(|e| {
                                    println!("Failed to insert partition: {e}");
                                    Error::RollbackTransaction
                                })?;
                            println!("Inserted partition: {}", partition_uuid);
                        } else {
                            diesel::update(partition::table.filter(partition::uuid.eq(&partition_uuid)))
                            .set(&partition)
                            .execute(conn)
                            .map_err(|e| {
                                println!("Failed to update storage: {e}");
                                Error::RollbackTransaction
                            })?;
                        println!("Updated storage: {}", storage_uuid);
                        }
                    }
                }
            }

            // === NIC HANDLING ===
            if let Some(nic_value) = device_value.get("nic") {
                println!("Processing NIC: {:?}", nic_value);

                let mut nic: Nic = serde_json::from_value(nic_value.clone()).map_err(|e| {
                    println!("Failed to parse NIC: {e}");
                    Error::RollbackTransaction
                })?;
                nic.device_uuid = device_uuid.clone();
                let nic_uuid = nic.uuid.clone();

                let existing_nic = nic::table
                    .filter(nic::uuid.eq(&nic_uuid))
                    .first::<Nic>(conn)
                    .optional()
                    .map_err(|e| {
                        println!("Failed to query NIC: {e}");
                        Error::RollbackTransaction
                    })?;

                if existing_nic.is_none() {
                    diesel::insert_into(nic::table)
                        .values(&nic)
                        .execute(conn)
                        .map_err(|e| {
                            println!("Failed to insert NIC: {e}");
                            Error::RollbackTransaction
                        })?;
                    println!("Inserted NIC: {}", nic_uuid);
                } else {
                    diesel::update(nic::table.filter(nic::uuid.eq(&nic_uuid)))
                        .set(&nic)
                        .execute(conn)
                        .map_err(|e| {
                            println!("Failed to update NIC: {e}");
                            Error::RollbackTransaction
                        })?;
                    println!("Updated NIC: {}", nic_uuid);
                }

                // === PORT HANDLING ===
                if let Some(port_array) = nic_value.get("port").and_then(|v| v.as_array()) {
                    for port_value in port_array {
                        let mut port: Port = serde_json::from_value(port_value.clone()).map_err(|e| {
                            println!("Failed to parse port: {e}");
                            Error::RollbackTransaction
                        })?;
                        port.nic_uuid = nic_uuid.clone();
                        let port_uuid = port.uuid.clone();

                        let existing_port = port::table
                            .filter(port::uuid.eq(&port_uuid))
                            .first::<Port>(conn)
                            .optional()
                            .map_err(|e| {
                                println!("Failed to query port: {e}");
                                Error::RollbackTransaction
                            })?;

                        if existing_port.is_none() {
                            diesel::insert_into(port::table)
                                .values(&port)
                                .execute(conn)
                                .map_err(|e| {
                                    println!("Failed to insert port: {e}");
                                    Error::RollbackTransaction
                                })?;
                            println!("Inserted port: {}", port_uuid);
                        } else {
                             diesel::update(port::table.filter(port::uuid.eq(&port_uuid)))
                            .set(&port)
                            .execute(conn)
                            .map_err(|e| {
                                println!("Failed to update storage: {e}");
                                Error::RollbackTransaction
                            })?;
                            println!("Updated port: {}", port_uuid);
                        }

                        // === IP HANDLING ===
                        if let Some(ip_array) = port_value.get("ip").and_then(|v| v.as_array()) {
                            for ip_value in ip_array {
                                let mut ip: Ip = serde_json::from_value(ip_value.clone()).map_err(|e| {
                                    println!("Failed to parse IP: {e}");
                                    Error::RollbackTransaction
                                })?;
                                ip.port_uuid = port_uuid.clone();

                                diesel::insert_into(ip_address::table)
                                    .values(&ip)
                                    .execute(conn)
                                    .map_err(|e| {
                                        println!("Failed to insert IP: {e}");
                                        Error::RollbackTransaction
                                    })?;
                                println!("Inserted IP.");
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    })
}
// pub fn delete_action(conn: &mut SqliteConnection, json_data: &Value) -> Result<(), Error> {
//     use diesel::prelude::*;

//     conn.transaction::<_, Error, _>(|conn| {
//         println!("Deleting data from the database...");
//         println!("JSON data: {:?}", json_data.to_string());

//         let deleted_value = json_data.get("action").ok_or_else(|| {
//             println!("Missing 'deleted' key");
//             diesel::result::Error::NotFound
//         })?;

//         if let Some(table_map) = deleted_value.as_object() {
//             for (table_name, uuid_array) in table_map {
//                 let uuid_list: Vec<String> = uuid_array
//                     .as_array()
//                     .ok_or_else(|| {
//                         println!("UUIDs for table '{}' are not an array", table_name);
//                         diesel::result::Error::NotFound
//                     })?
//                     .iter()
//                     .filter_map(|v| v.as_str().map(|s| s.to_string()))
//                     .collect();

//                 perform_delete(conn, table_name, &uuid_list)?;
//             }
//         }
//         else if let Some(table_name) = deleted_value.as_str() {
//             let uuid_array = json_data.get("uuid").and_then(|v| v.as_array()).ok_or_else(|| {
//                 println!("Missing or invalid 'uuid' key for single delete");
//                 diesel::result::Error::NotFound
//             })?;

//             let uuid_list: Vec<String> = uuid_array
//                 .iter()
//                 .filter_map(|v| v.as_str().map(|s| s.to_string()))
//                 .collect();

//             perform_delete(conn, table_name, &uuid_list)?;
//         } else {
//             println!("Invalid 'deleted' format");
//             return Err(diesel::result::Error::NotFound);
//         }

//         Ok(())
//     })
// }

pub fn delete_action(conn: &mut SqliteConnection, json_data: &Value) -> Result<(), Error> {
    conn.transaction::<_, Error, _>(|conn| {
        println!("Deleting data from the database...");
        println!("JSON data: {:?}", json_data.to_string());

        // Extract action string
        let action = json_data.get("action").and_then(|v| v.as_str())
            .ok_or_else(|| {
                println!("Missing 'action' key");
                diesel::result::Error::NotFound
            })?;

        // Check action prefix, e.g., "deleted_partition"
        if !action.starts_with("deleted_") {
            println!("Action '{}' is not a delete action", action);
            return Err(diesel::result::Error::NotFound);
        }

        // Extract table name after "deleted_"
        let table_name = &action["deleted_".len()..]; // slice after "deleted_"

        // Get UUID array from "uuid" key
        let uuid_list: Vec<String> = match json_data.get("uuid") {
            Some(v) if v.is_array() => {
                v.as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|val| val.as_str().map(|s| s.to_string()))
                    .collect()
            }
            Some(v) if v.is_string() => {
                vec![v.as_str().unwrap().to_string()]
            }
            _ => {
                println!("Missing or invalid 'uuid' key");
                return Err(diesel::result::Error::NotFound);
            }
        };
        // Call perform_delete with extracted table_name and UUIDs
        perform_delete(conn, table_name, &uuid_list)?;

        Ok(())
    })
}

fn perform_delete(conn: &mut SqliteConnection, table_name: &str, uuid_list: &[String]) -> Result<(), Error> {
    use crate::schema::{nic, partition, port, storage}; 
    use diesel::prelude::*;

    match table_name {
        "partition" => {
            diesel::delete(partition::table.filter(partition::uuid.eq_any(uuid_list))).execute(conn)?;
        }
        "disk" => {
            diesel::delete(storage::table.filter(storage::uuid.eq_any(uuid_list))).execute(conn)?; 
        }
         "nic"  => {
            diesel::delete(nic::table.filter(nic::uuid.eq_any(uuid_list))).execute(conn)?;
        }
         "port"  => {
            diesel::delete(port::table.filter(port::uuid.eq_any(uuid_list))).execute(conn)?;
        }
    
        _ => {
            println!("Unsupported table: {}", table_name);
            return Err(diesel::result::Error::NotFound);
        }
    }

    Ok(())
}
