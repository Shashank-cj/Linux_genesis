use diesel::prelude::*;
use serde::{Deserialize, Serialize};
#[derive(Debug, Queryable, Insertable, Serialize, Deserialize)]
#[diesel(table_name = crate::schema::agent_credential)]
pub struct AgentCredential {
    pub id: Option<i32>,
    pub uuid: String,
    pub client_id: String,
    pub client_secret: String,
    pub master_key: String,
}


#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = crate::schema::agent)]
pub struct Agent {
    pub uuid: String,
    pub os: String,
    pub hostname: String,
    pub os_version: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = crate::schema::device)]
pub struct Device {
    pub uuid: String,
    pub make: String,
    pub model: String,
    pub serial_number: String,
    pub dev_phy_vm: String,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = crate::schema::cpu)]
pub struct Cpu {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub device_uuid: String,
    pub make: String,
    pub model: String,
    pub p_cores: i32,
    pub l_cores: i32,
    pub speed: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = crate::schema::memory)]
pub struct Memory {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub device_uuid: String,
    pub make: String,
    pub model: String,
    pub speed: String,
    pub size: String,
    pub serial_number: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable, AsChangeset, Deserialize,Queryable)]
#[diesel(table_name = crate::schema::storage)]
pub struct Storage {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub device_uuid: String,
    pub hw_disk_type: String,
    pub make: String,
    pub model: String,
    pub serial_number: String,
    pub base_fs_type: String,
    pub free_space: String,
    pub total_disk_usage: String,
    pub total_disk_size: String,
    pub unallocated_disk_size: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable,Queryable, Deserialize,AsChangeset)]
#[diesel(table_name = crate::schema::partition)]
pub struct Partition {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub storage_uuid: String,
    pub name: String,
    pub serial_number: String,
    pub fs_type: String,
    pub free_space: String,
    pub used_space: String,
    pub total_size: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable, Deserialize, Queryable,AsChangeset)]
#[diesel(table_name = crate::schema::nic)]
pub struct Nic {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub device_uuid: String,
    pub make: String,
    pub model: String,
    pub number_of_ports: i32,
    pub max_speed: String,
    pub supported_speeds: String,
    pub serial_number: String,
    pub mac_address: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable, Deserialize, Queryable,AsChangeset)]
#[diesel(table_name = crate::schema::port)]
pub struct Port {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub nic_uuid: String,
    pub interface_name: String,
    pub operating_speed: String,
    pub is_physical_logical: String,
    pub logical_type: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable, Deserialize , Queryable)]
#[diesel(table_name = crate::schema::ip_address)]
pub struct Ip {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub port_uuid: String,
    pub address: String,
    pub gateway: Option<String>,
    pub subnet_mask: String,
    pub dns: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = crate::schema::gpu)]
pub struct Gpu {
    pub uuid: String,
    #[serde(skip_deserializing)] 
    pub device_uuid: String,
    pub make: String,
    pub model: String,
    pub serial_number: String,
    pub size: String,
    pub driver: String,
    pub os_uuid: Option<String>,
}

#[derive(Debug, Queryable, Insertable, Serialize, Deserialize, Clone, AsChangeset, Selectable)]
#[diesel(table_name = crate::schema::tokens)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Token {
    pub id: Option<i32>, 
    pub token: String,
    pub expiration: String,
    pub token_type: String,
}


