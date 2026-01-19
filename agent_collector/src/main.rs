use tracing::{info,error};
use tokio::signal;
use futures::StreamExt;
use agent_lib;
use shared_config::CONFIG;

use nats::publisher::NatsPublisher;
use nats::subscriber::NatsSubscriber;
use models_database::db::{ get_agent_details};
use async_nats::Client;




async fn setup_nats_client() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    // === SUBSCRIBE TO bridge.response ===
    let client = subscriber.client().clone();
    let subscribe_for_scan = subscriber.client().clone();
    let pub_clone = publisher.clone();

    let mut bridge_sub = subscriber.client().subscribe("bridge.response".to_string()).await?;

    let mut agent_response_sub = subscriber.client().subscribe("agent.response".to_string()).await?;

    let mut rescan_agent_response_sub = subscriber.client().subscribe("rescanagent.response".to_string()).await?;

    tokio::spawn(async move {
        while let Some(msg) = bridge_sub.next().await {
            let payload = String::from_utf8_lossy(&msg.payload);
            let mut conn = models_database::establish_encrypted_connection();

            // ---------- RESCAN ----------
            if get_agent_details(&mut conn).is_some() {
                info!("Agent exists → sending RESCAN update");

                match agent_lib::agent_rescan() {
                    Ok(rescan_data) => {
                        info!(rescan_data);
                        if let Err(e) = pub_clone.publish("agent.rescan", &rescan_data).await {
                            error!("Failed to publish agent rescan data: {e}");
                        }
                    }
                    Err(e) => error!("Failed to collect agent rescan data: {e}"),
                }
            }
            else{
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload) {
                if json.get("status") == Some(&serde_json::Value::String("ok".to_string()))
                    || json.get("message") == Some(&serde_json::Value::String("Token is already exists".to_string()))
                {
                    info!("Collecting the agent data...");
                    match agent_lib::agent_data() {
                        Ok(agent_data) => {
                            if let Err(e) = pub_clone.publish("agent.data", &agent_data).await {
                                error!("Failed to publish agent data: {e}");
                            }
                        }
                        Err(e) => error!("Failed to collect agent data: {}", e),
                    }
                }
            } else {
                error!(" Failed to parse JSON response: {}", payload);
            }
        }

            
        }
    });

    let client = subscriber.client().clone();

    let client_for_agent = client.clone();

    // ---------- AGENT RESPONSE ----------
    tokio::spawn(async move {
        while let Some(msg) = agent_response_sub.next().await {
            let payload = String::from_utf8_lossy(&msg.payload);

            if payload.contains("Device created successfully") || payload.contains("Reinstallation detected - returned previous device data")
             {
                info!(" Valid response received. Starting monitoring automatically...");
                start_monitoring(client_for_agent.clone()).await;
            } else {
                error!(" Unexpected response: {}", payload);
            }
        }
    });

    // ---------- RESCAN RESPONSE ----------
    let client_for_rescan = client.clone();
    tokio::spawn(async move {
        while let Some(msg) = rescan_agent_response_sub.next().await {
            let payload = String::from_utf8_lossy(&msg.payload);

            if payload.contains("Device updated successfully") {
                start_monitoring(client_for_rescan.clone()).await;
            } else {
                error!(" Unexpected response: {}", payload);
            }
        }
    });


    // === HANDLE SCAN TOPIC ===
    tokio::spawn(async move {
        let mut new_sub = match subscribe_for_scan.subscribe("scan.>".to_string()).await {
            Ok(sub) => sub,
            Err(e) => {
                error!("Failed to subscribe to scan.partition: {e}");
                return;
            }
        };

        while let Some(msg) = new_sub.next().await {
            let payload = String::from_utf8_lossy(&msg.payload);
            info!("Received scan request: {}", payload);

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Some(action) = json.get("action").and_then(|v| v.as_str()) {
                    let uuid_value = json.get("uuid").and_then(|v| v.as_str()).unwrap_or("");
                    match action {
                        "disk" | "partition" => {
                            info!("Scanning disk...");
                            match agent_lib::scan_disk(action) {
                                Ok(disk) => send_scan_response(&publisher, action, uuid_value, disk).await,
                                Err(e) => error!("Failed to scan disk: {}",e),
                            }
                        }
                        "nic" => {
                            info!("Scanning NIC...");
                            match agent_lib::scan_nic(action) {
                                Ok(nic_data) => send_scan_response(&publisher, action, uuid_value, nic_data).await,
                                Err(e) => error!("Failed to scan NIC: {}",e),
                            }
                        }
                        _ => error!("Unknown action received: {}", action),
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
        error!("Failed to publish agent data: {}",e);
    }
}

// REPLACE your start_monitoring function with this:
async fn start_monitoring(client: Client) {
    // Remove manual input - start automatically
    info!("Starting automatic monitoring");
    
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(CONFIG.monitor_interval_secs));
    let mut data_queue: Vec<String> = Vec::new();

    loop {
        interval.tick().await;

        match agent_lib::monitor_data() {
            Ok(monitor_data) => {
                data_queue.push(monitor_data);

                if data_queue.len() as u64 == CONFIG.monitor_interval {
                    let payload = format!("[{}]", data_queue.join(","));
                    if let Err(e) = client
                        .publish("monitor.data".to_string(), payload.into_bytes().into())
                        .await
                    {
                        error!("Failed to publish batch: {e}");
                    } else {
                        info!("[INFO] Sent 5-point batch to bridge");
                        data_queue.clear();
                    }
                }
            }
            Err(e) => error!("Failed to collect monitor data: {}",e),
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

    setup_nats_client().await?;

    signal::ctrl_c().await?;
    info!("Collector shutting down gracefully...");

    Ok(())
}
