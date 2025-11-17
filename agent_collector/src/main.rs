use tracing::{info};
use tokio::signal;
use serde::Serialize;
// Remove this line: use tokio::io::AsyncBufReadExt;
use futures::StreamExt;
use base64::{engine::general_purpose, Engine as _};
use agent_lib;
use shared_config::CONFIG;
mod key_utils;
use key_utils::KeyManager;

use nats::publisher::NatsPublisher;
use nats::subscriber::NatsSubscriber;
use models_database::db::{establish_connection, get_agent_details};
use async_nats::Client;
use hostname;
use sys_info;
use pnet_datalink::interfaces;
use sha2::{Sha256, Digest};
use hex;

#[derive(Serialize, Debug)]
struct MasterKeyPayload {
    master_key: String,
    hostname: String,
    os: String,
    os_version: String,
    mac_hash: String,
}

/// Helper to extract Linux OS version (from /etc/os-release)
fn get_linux_os_info() -> (String, String) {
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        let mut os_name = String::new();
        let mut os_version = String::new();

        for line in content.lines() {
            if line.starts_with("NAME=") {
                os_name = line.trim_start_matches("NAME=")
                              .trim_matches('"')
                              .to_string();
            } else if line.starts_with("PRETTY_NAME=") {
                let pretty = line.trim_start_matches("PRETTY_NAME=").trim_matches('"');
                if let Some(rest) = pretty.strip_prefix(&format!("{} ", os_name)) {
                    os_version = rest.to_string();
                } else {
                    os_version = pretty.to_string();
                }
            }
        }

        (os_name, os_version)
    } else {
        (
            String::from("Linux"),
            sys_info::os_release().unwrap_or_default(),
        )
    }
}

fn get_all_mac_addresses() -> Vec<String> {
    interfaces()
        .into_iter()
        .filter_map(|iface| iface.mac.map(|m| m.to_string()))
        .collect()
}



fn hash_all_macs(macs: Vec<String>) -> String {
    let combined = macs.join("|"); // combine all macs into one string
    let mut hasher = Sha256::new();
    hasher.update(combined.as_bytes());
    hex::encode(hasher.finalize())
}


async fn setup_nats_client(master_key: Vec<u8>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // === PUBLISHER SETUP ===
    let publisher = NatsPublisher::new(
        &CONFIG.nats_url,
        &std::fs::read_to_string(&CONFIG.c_jwt_path)?,
        &std::fs::read_to_string(&CONFIG.c_nkey_path)?,
        &CONFIG.ca_cert_path,
        &CONFIG.client_cert_path,
        &CONFIG.client_key_path,
    ).await?;

    // === SUBSCRIBER SETUP ===
    let subscriber = NatsSubscriber::new(
        &CONFIG.nats_url,
        &std::fs::read_to_string(&CONFIG.c_jwt_path)?,
        &std::fs::read_to_string(&CONFIG.c_nkey_path)?,
        &CONFIG.ca_cert_path,
        &CONFIG.client_cert_path,
        &CONFIG.client_key_path,
    ).await?;

    println!("CONFIG.c_nkey_path: {}", &CONFIG.c_nkey_path);

    let (os_name, os_version) = get_linux_os_info();
    let all_macs = get_all_mac_addresses();
    let final_mac_hash = hash_all_macs(all_macs); 


    let payload = MasterKeyPayload {
        master_key: general_purpose::STANDARD.encode(&master_key),
        hostname: hostname::get()?.to_string_lossy().to_string(),
        os: os_name,        
        os_version: os_version, 
        mac_hash: final_mac_hash,
    };

    publisher.publish("master.key", &payload).await?;
    info!("Master key published to NATs........... ");

    // === SUBSCRIBE TO bridge.response ===
    let client = subscriber.client().clone();
    let subscribe_for_scan = subscriber.client().clone();
    let pub_clone = publisher.clone();

    let mut sub = subscriber.client().subscribe("bridge.response".to_string()).await?;
    tokio::spawn(async move {
        while let Some(msg) = sub.next().await {
            let payload = String::from_utf8_lossy(&msg.payload);
            let mut conn = establish_connection(&CONFIG.db_path);

            if get_agent_details(&mut conn).is_some() {
                println!("[INFO] Device details stored in database. Starting monitoring automatically...");
                info!("Starting monitoring automatically");
                start_monitoring(client.clone()).await;
            } else {
                println!("[INFO] Device details not found. Collecting the agent data...");
                info!("Device details not found. Collecting the agent data...");
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload) {
                if json.get("status") == Some(&serde_json::Value::String("ok".to_string()))
                    || json.get("message") == Some(&serde_json::Value::String("Token is already exists".to_string()))
                {
                    info!("Collecting the agent data...");
                    match agent_lib::agent_data() {
                        Ok(agent_data) => {
                            if let Err(e) = pub_clone.publish("agent.data", &agent_data).await {
                                eprintln!("Failed to publish agent data: {e}");
                            }

                            println!("Waiting for agent response...");
                            let mut agent_response_sub = match subscriber.client().subscribe("agent.response".to_string()).await {
                                Ok(sub) => sub,
                                Err(e) => {
                                    eprintln!("Failed to subscribe to agent.response: {e}");
                                    return;
                                }
                            };

                            while let Some(msg) = agent_response_sub.next().await {
                                let payload = String::from_utf8_lossy(&msg.payload);
                                println!("Agent response: {}", payload);
                                if payload.contains("Data stored successfully") {
                                    println!("[INFO] Valid response received. Starting monitoring automatically...");
                                    start_monitoring(client.clone()).await;
                                } else {
                                    eprintln!("[WARN] Unexpected response: {}", payload);
                                }
                            }
                        }
                        Err(e) => eprintln!("Failed to collect agent data: {e}"),
                    }
                }
            } else {
                eprintln!("[ERROR] Failed to parse JSON response: {}", payload);
            }
        }
    });

    // === HANDLE SCAN TOPIC ===
    tokio::spawn(async move {
        let mut new_sub = match subscribe_for_scan.subscribe("scan.>".to_string()).await {
            Ok(sub) => sub,
            Err(e) => {
                eprintln!("Failed to subscribe to scan.partition: {e}");
                return;
            }
        };

        while let Some(msg) = new_sub.next().await {
            let payload = String::from_utf8_lossy(&msg.payload);
            println!("Received scan request: {}", payload);

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(action) = json.get("action").and_then(|v| v.as_str()) {
                    let uuid_value = json.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
                    match action {
                        "disk" | "partition" => {
                            info!("Scanning disk...");
                            match agent_lib::scan_disk(action) {
                                Ok(disk) => send_scan_response(&publisher, action, uuid_value, disk).await,
                                Err(e) => eprintln!("Failed to scan disk: {e}"),
                            }
                        }
                        "nic" => {
                            info!("Scanning NIC...");
                            match agent_lib::scan_nic(action) {
                                Ok(nic_data) => send_scan_response(&publisher, action, uuid_value, nic_data).await,
                                Err(e) => eprintln!("Failed to scan NIC: {e}"),
                            }
                        }
                        _ => eprintln!("Unknown action received: {}", action),
                    }
                }
            }
        }
    });

    Ok(())
}

async fn send_scan_response<T: serde::Serialize>(
    publisher: &NatsPublisher,
    action: &str,
    uuid: &str,
    data: T,
) {
    let original_json = serde_json::json!(data);
    let message_json = serde_json::json!({
        "uuid": uuid,
        "result": original_json,
        "action": action
    });

    if let Err(e) = publisher.publish(&format!("send.scan.{}", action), &message_json).await {
        eprintln!("Failed to publish agent data: {e}");
    }
}

// REPLACE your start_monitoring function with this:
async fn start_monitoring(client: Client) {
    // Remove manual input - start automatically
    println!("🚀 Starting automatic monitoring...");
    info!("Starting automatic monitoring");
    
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
    let mut data_queue: Vec<String> = Vec::new();

    loop {
        interval.tick().await;

        match agent_lib::monitor_data() {
            Ok(monitor_data) => {
                println!("Collected monitor data: {}", monitor_data);
                data_queue.push(monitor_data);

                if data_queue.len() == 5 {
                    let payload = format!("[{}]", data_queue.join(","));
                    if let Err(e) = client
                        .publish("monitor.data".to_string(), payload.into_bytes().into())
                        .await
                    {
                        eprintln!("Failed to publish batch: {e}");
                    } else {
                        println!("[INFO] Sent 5-point batch to bridge");
                        data_queue.clear();
                    }
                }
            }
            Err(e) => eprintln!("Failed to collect monitor data: {e}"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Handle installation flag
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--send-agent-data" {
        println!(" Package installation - running normal flow...");
    }
    
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Loading master key...");
    let master_key = KeyManager::load_master_key();
    info!("Master key loaded successfully");

    setup_nats_client(master_key).await?;

    signal::ctrl_c().await?;
    info!("Collector shutting down gracefully...");

    Ok(())
}
