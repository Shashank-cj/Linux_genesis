use tracing::{info, error,warn};
use serde_json::Value;
use tokio::signal;
use shared_config::CONFIG;
use nats::publisher::NatsPublisher;
use nats::subscriber::NatsSubscriber;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
mod server_api; 
use server_api::{send_master_key_to_server, send_to_server, get_new_access_token,send_to_monitor_server,scan_data_to_server};
use models_database::db::{
    save_token,get_token,token_exists,delete_initial_data,
};
use serde::Serialize;
use sha2::{Sha256, Digest};
use base64::{engine::general_purpose, Engine};
use pnet_datalink::interfaces;
 use std::path::PathBuf;
use hostname;
use sys_info;
use hex;


//mod config; // Add this line to include the config module

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



fn hash_each_mac(macs: Vec<String>) -> Vec<String> {
    macs.into_iter()
        .map(|mac| {
            let mut hasher = Sha256::new();
            hasher.update(mac.as_bytes());
            hex::encode(hasher.finalize())
        })
        .collect()
}

fn combine_mac_hashes(hashes: Vec<String>) -> String {
    hashes.join("-")
}


pub async fn create_publisher() -> Result<NatsPublisher, Box<dyn std::error::Error + Send + Sync>> {
    let publisher = NatsPublisher::new(
        &CONFIG.nats_url,
        &std::fs::read_to_string(&CONFIG.b_jwt_path)?,
        &std::fs::read_to_string(&CONFIG.b_nkey_path)?,
        &CONFIG.ca_cert_path,
        &CONFIG.bridge_cert_path,
        &CONFIG.bridge_key_path,
    )
    .await?;
    Ok(publisher)
}

pub async fn create_subscriber() -> Result<Arc<Mutex<NatsSubscriber>>, Box<dyn std::error::Error + Send + Sync>> {
    let subscriber = NatsSubscriber::new(
        &CONFIG.nats_url,
        &std::fs::read_to_string(&CONFIG.b_jwt_path)?,
        &std::fs::read_to_string(&CONFIG.b_nkey_path)?,
        &CONFIG.ca_cert_path,
        &CONFIG.bridge_cert_path,
        &CONFIG.bridge_key_path,
    )
    .await?;
    
    // Wrap the subscriber in Arc<Mutex<>> for sharing
    Ok(Arc::new(Mutex::new(subscriber)))
}

    
pub async fn handle_nats_operations(publisher: NatsPublisher,) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting NATS operations handler");

    
    info!("NATS Publisher created successfully");
    // let subscriber_master = create_subscriber().await?;
    let subscriber_agent = create_subscriber().await?;
    let subscriber_monitor = create_subscriber().await?;
    let subscriber_scan =create_subscriber().await?;
    let subscriber_rescan = create_subscriber().await?;



    let http_client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    // Spawn independent handlers
    // let master_key_handler = handle_master_key_operations(subscriber_master, publisher.clone(), http_client.clone());
    let agent_data_handler = handle_agent_data_operations(subscriber_agent, publisher.clone(), http_client.clone());
    let monitor_data_handler = handle_monitor_data_operations(subscriber_monitor, publisher.clone(), http_client.clone());
    let scan_data_handler = handle_scan_data_operations(subscriber_scan, publisher.clone(), http_client.clone());
    let agent_rescan_handler =handle_agent_rescan_operations(subscriber_rescan, publisher.clone(), http_client.clone());


    tokio::select! {
        // res = master_key_handler => {
        //     if let Err(e) = res {
        //         error!("NATS Master key handler failed: {:?}", e);
        //         return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
        //     }
        // }
        res = agent_data_handler => {
            if let Err(e) = res {
                error!("NATS Agent data handler failed: {:?}", e);
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        }
        res = agent_rescan_handler => {
        if let Err(e) = res {
            error!("NATS Agent rescan handler failed: {:?}", e);
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
        }
    }
        res = monitor_data_handler => {
            if let Err(e) = res {
                error!("[MONITOR] Monitor handler crashed: {:?}", e);
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        }
        res = scan_data_handler => {
            if let Err(e) = res {
                 error!("NATS Scan data handler failed: {:?}", e);
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
        }
    }

    Ok(())
}


async fn process_monitor_data(http_client: &reqwest::Client, payload: &str) -> Result<String, Box<dyn std::error::Error>> {
   info!("[MONITOR] Processing monitor data batch");

   let mut conn = models_database::establish_encrypted_connection();

    
    let token = match get_token(&mut conn, "access_token") {
        Some(token) => {
            info!("[MONITOR] Found existing access token in database");
            token.token
        },
        None => {
            info!("[MONITOR] No access token found, requesting new token...");
            match get_new_access_token("access_token").await {
                Ok(token) => {

                    let token_json: Value = match serde_json::from_str(&token) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[MONITOR] Failed to parse token JSON: {}", e);
                            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to parse token JSON")));
                        }
                    };

                    let expires_in = token_json.get("expires_in")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
            
                    let access_token_str = token_json.get("access_token")
                        .and_then(Value::as_str)
                        .unwrap_or("");

                    if access_token_str.is_empty() {
                    error!("[MONITOR] Received empty access_token from server");
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Empty access_token received")));
                     }
            
            
                    let expiration_time = (chrono::Local::now().naive_local()
                        + chrono::Duration::seconds(expires_in))
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string();
            

                    if let Err(e) = save_token(&mut conn, access_token_str, &expiration_time, "access_token") {
                         error!("[MONITOR] Failed to save token to DB: {}", e);
                    }
                    info!("[MONITOR] Successfully saved new access token to database");
        
                    match get_token(&mut conn, "access_token") {
                        Some(token) => 
                        {
                        info!("[MONITOR] Retrieved newly saved token from database");
                        token.token
                        },
                        None => {
                            error!("[MONITOR] Failed to retrieve newly saved token from database");
                            String::new()
                        }
                    }
                    
                }
                Err(e) => {
                   error!("[MONITOR] Failed to fetch access token: {}", e);
                    String::new() 
                }
            }
        }
    };

    if token.is_empty() {
        error!("[MONITOR] Access token is empty, cannot send monitoring data");
        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Empty access token")));
    }

    let result = send_to_monitor_server(payload, &token).await;

    match result {
        Ok(response_data) => {
            info!("[MONITOR] Successfully sent monitoring data to server");
            Ok(response_data)
        },
        Err(e) => {
            error!("[MONITOR] Failed to send monitoring data to server: {}", e);
            return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)));
        }
    }
}

// Master key operations handler
async fn handle_master_key_operations(publisher: NatsPublisher,) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    info!("Master key handler started");
    // let mut subscriber = subscriber.lock().await;
    // let mut subscriber = subscriber.client().subscribe("master.key".to_string()).await?; 

    // while let Some(msg) = subscriber.next().await {
    //     let received_payload = String::from_utf8_lossy(&msg.payload);
    //     info!("Received master key payload");


        info!("Loading master key from systemd-credentials");
        let (os_name, os_version) = get_linux_os_info();
        let all_macs = get_all_mac_addresses();
        let mac_hashes = hash_each_mac(all_macs); 
        let final_mac_hash = combine_mac_hashes(mac_hashes);
        let master_key_path = PathBuf::from(&CONFIG.secrets_dir).join("master_key.bin");
        let master_key = std::fs::read(&master_key_path)?;

        let payload = MasterKeyPayload {
            master_key: general_purpose::STANDARD.encode(&master_key),
            hostname: hostname::get()?.to_string_lossy().to_string(),
            os: os_name,        
            os_version: os_version, 
            mac_hash: final_mac_hash,
        };


        let payload_json = serde_json::to_string(&payload)?;

        if let Err(e) = send_master_key_to_server(&payload_json).await {
            error!("Failed to send master key: {}", e);
        }
        let mut conn = models_database::establish_encrypted_connection();


        if !token_exists(&mut conn, "access_token") {
            info!("Token not found in the database, fetching new token...");
       
            match get_new_access_token("token").await {
                Ok(token) => {
                    let token_json: Value = match serde_json::from_str(&token) {
                                Ok(v) => v,
                                Err(e) => {
                                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Failed to parse token JSON")));
                                }
                            };

                            let expires_in = token_json.get("expires_in")
                                .and_then(Value::as_i64)
                                .unwrap_or(0);
                    
                            let access_token_str = token_json.get("access_token")
                                .and_then(Value::as_str)
                                .unwrap_or("");
                    
                            let expiration_time = (chrono::Local::now().naive_local()
                                + chrono::Duration::seconds(expires_in))
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string();

                    
                    if let Err(e) = save_token(&mut conn, access_token_str, &expiration_time, "access_token") {
                                error!("Failed to save token to DB: {}", e);
                            }

                    let response = serde_json::json!({
                        "status": "ok",
                        "token": token,
                    });

                    if let Err(e) = publisher.publish("bridge.response", &response).await {
                        error!("Failed to publish token response: {}", e);
                    }
                }
                Err(e) => error!("Failed to fetch access token: {}", e),
            }
        } else {
    info!("Token already exists in the database");
    let response = serde_json::json!({"message": "Token is already exists"});
    if let Err(e) = publisher.publish("bridge.response", &response).await {
        error!("Failed to publish token response: {}", e);
    } else {
        info!("Successfully published to bridge.response");
    }
}

    // }

    Ok(())
}

// Agent data operations handler


async fn send_agent_payload(data_payload: &str,) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {

    let mut conn = models_database::establish_encrypted_connection();
    let token = match get_token(&mut conn, "access_token") {
        Some(token) => token.token,
        None => {
            let token = get_new_access_token("access_token").await?;
            let token_json: Value = serde_json::from_str(&token)?;

            let expires_in = token_json
                .get("expires_in")
                .and_then(Value::as_i64)
                .unwrap_or(0);

            let access_token_str = token_json
                .get("access_token")
                .and_then(Value::as_str)
                .unwrap_or("");

            let expiration_time = (chrono::Local::now().naive_local()
                + chrono::Duration::seconds(expires_in))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();

            save_token(&mut conn, access_token_str, &expiration_time, "access_token")?;
            access_token_str.to_string()
        }
    };

    send_to_server(data_payload, &token).await
}
async fn handle_agent_data_operations(subscriber: Arc<Mutex<NatsSubscriber>>,publisher: NatsPublisher,_http_client: reqwest::Client,) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    info!("[AGENT] Agent data handler started");

    let mut subscriber = subscriber
        .lock()
        .await
        .client()
        .subscribe("agent.data".to_string())
        .await?;

    while let Some(msg) = subscriber.next().await {
        info!("Bridge: Listening for 'agent.data'...");

        let data_payload = String::from_utf8_lossy(&msg.payload);

        match send_agent_payload(&data_payload).await {
            Ok(response_msg) => {
                info!("[AGENT] Server responded: {}", response_msg);

                if let Err(e) = publisher.publish("agent.response", &response_msg).await {
                    error!("Bridge: Failed to publish response: {:?}", e);
                } else {
                    info!("[AGENT] Response sent successfully");
                }
            }
            Err(e) => error!("Bridge: Failed to send data to server: {:?}", e),
        }
    }

    Ok(())
}

async fn handle_agent_rescan_operations( subscriber: Arc<Mutex<NatsSubscriber>>, publisher: NatsPublisher,_http_client: reqwest::Client,) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

    let mut subscriber = subscriber
        .lock()
        .await
        .client()
        .subscribe("agent.rescan".to_string())
        .await?;

    while let Some(msg) = subscriber.next().await {
        info!("Bridge: Listening for 'agent.rescan'...");
        let data_payload = String::from_utf8_lossy(&msg.payload);
        info!("[AGENT] rescan responded for rescan: {}", data_payload);
        match send_agent_payload(&data_payload).await {
            
            Ok(response_msg) => {
                info!("[AGENT] Server responded for rescan: {}", response_msg);

                if let Err(e) = publisher.publish("rescanagent.response", &response_msg).await {
                    error!("Bridge: Failed to publish response: {:?}", e);
                } else {
                    info!("[AGENT] Response sent successfully");
                }
            }
            Err(e) => error!("Bridge: Failed to send data to server: {:?}", e),
        }
    }

    Ok(())
}




// Monitor data operations handler
// async fn handle_monitor_data_operations(subscriber: Arc<Mutex<NatsSubscriber>>,publisher: NatsPublisher,http_client: reqwest::Client) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//     info!("Monitor data handler started");
//     let mut subscriber = subscriber.lock().await;
//     let mut subscriber = subscriber.client().subscribe("monitor.data".to_string()).await?;
    

//     while let Some(msg) = subscriber.next().await {
//         let payload = String::from_utf8_lossy(&msg.payload);
//         match process_monitor_data(&http_client, &payload).await {
//             Ok(response_data) => {
//                 info!("Received monitor server response: {}", response_data);
//                 if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&response_data) {

//                     // if json_value.get("deleted").is_some() {
//                     //     info!("Deleting initial data from the database");
//                     //     let mut conn = establish_connection(&CONFIG.db_path);
//                     //     if let Err(e) = delete_initial_data(&mut conn, &json_value) {
//                     //         error!("Failed to delete initial data: {}", e);
//                     //     }
//                     // }

//                     if let Some(action) = json_value.get("action").and_then(|v| v.as_str()) {
//                         if action.contains("deleted") {
//                             info!("Action contains 'deleted', calling delete_action");
//                             let mut conn = establish_connection(&CONFIG.db_path);
//                             if let Err(e) = delete_initial_data(&mut conn, &json_value) {
//                                 error!("Failed to delete initial data: {}", e);
//                             }
//                         }
//                          else{
//                             if let Err(e) = publisher.publish(&format!("scan.{}", action), &json_value).await {
//                                 error!("Bridge: Failed to publish response: {:?}", e);
//                             } else {
//                                 info!("Scan response sent successfully to the collector");
//                             }
//                         }
//                     }          
//                 }
//             }
//             Err(e) => error!("Failed to process monitor data batch: {}", e),
//         }
//     }

//     Ok(())
// }


async fn handle_monitor_data_operations(
    subscriber: Arc<Mutex<NatsSubscriber>>,
    publisher: NatsPublisher,
    http_client: reqwest::Client,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("[MONITOR] Monitor data handler started");

    let mut subscriber = subscriber.lock().await;
    let mut subscriber = subscriber.client().subscribe("monitor.data".to_string()).await?;

    while let Some(msg) = subscriber.next().await {
        let payload_str = String::from_utf8_lossy(&msg.payload);

        // Parse payload as JSON array
        let orig_array: Vec<Value> = serde_json::from_str(&payload_str).unwrap_or_else(|_| Vec::new());

        // Use the 5th item (index 4) if it exists
        if let Some(orig_json) = orig_array.get(4) {
            match process_monitor_data(&http_client, &payload_str).await {
                Ok(response_data) => {
                    info!("Received monitor server response: {}", response_data);

                    if let Ok(json_value) = serde_json::from_str::<Value>(&response_data) {
                        if let Some(action) = json_value.get("action").and_then(|v| v.as_str()) {
                            match action {
                                "verify_disk_existence" => {
                                    handle_verification(
                                        "disk_monitoring",
                                        "disk_uuid",
                                        orig_json,
                                        &json_value,
                                        &http_client,
                                    ).await;
                                }
                                "verify_nic_existence" => {
                                    handle_verification(
                                        "nic_monitoring",
                                        "nic_uuid",
                                        orig_json,
                                        &json_value,
                                        &http_client,
                                    ).await;
                                }
                                _ => {
                                    // For other actions, publish to NATS
                                    if let Err(e) = publisher.publish(&format!("scan.{}", action), &json_value).await {
                                        error!("Bridge: Failed to publish response: {:?}", e);
                                    } else {
                                        info!("Scan response sent successfully to the collector");
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => error!("Failed to process monitor data batch: {}", e),
            }
        } else {
            warn!("Original array does not have 5th item for verification");
        }
    }

    Ok(())
}

async fn handle_verification(
    monitoring_key: &str,
    uuid_key: &str,
    orig_json: &Value,
    resp_json: &Value,
    http_client: &reqwest::Client,
) {
    let orig_device_uuid = orig_json.get("device_uuid").and_then(|v| v.as_str());
    let resp_device_uuid = resp_json.get("device_uuid").and_then(|v| v.as_str());

    info!("Original device_uuid: {:?}", orig_device_uuid);
    info!("Response device_uuid: {:?}", resp_device_uuid);

    // Extract uuid list dynamically
    let orig_uuid_list: Vec<&str> = orig_json
        .get(monitoring_key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| d.get(uuid_key).and_then(|u| u.as_str()))
                .collect()
        })
        .unwrap_or_else(Vec::new);

    let resp_uuid = resp_json.get(uuid_key).and_then(|v| v.as_str());

    info!("Original {} list: {:?}", uuid_key, orig_uuid_list);
    info!("Response {}: {:?}", uuid_key, resp_uuid);

    let uuid_match = orig_uuid_list.iter().any(|u| Some(*u) == resp_uuid);

    if orig_device_uuid == resp_device_uuid && uuid_match {
        info!("UUID verification passed");

        let success_response = serde_json::json!({
            "action": format!("verify_{}_existence", uuid_key.trim_end_matches("_uuid")),
            uuid_key: resp_uuid.unwrap_or("unknown"),
            "device_uuid": resp_device_uuid.unwrap_or("unknown"),
            "exists": true
        });

        info!("Sending success response: {}", success_response);

        if let Err(e) = process_monitor_data(http_client, &success_response.to_string()).await {
            error!("Failed to send success response: {}", e);
        } else {
            info!("Success response sent ");
        }
    } else {
        warn!(" UUID verification failed");

        let failure_response = serde_json::json!({
            "action": format!("verify_{}_existence", uuid_key.trim_end_matches("_uuid")),
            uuid_key: resp_uuid.unwrap_or("unknown"),
            "device_uuid": resp_device_uuid.unwrap_or("unknown"),
            "exists": false
        });

        info!("Sending failure response: {}", failure_response);

        if let Err(e) = process_monitor_data(http_client, &failure_response.to_string()).await {
            error!("Failed to send failure response: {}", e);
        } else {
            info!("Failure response sent ");
        }
    }
}


pub async fn handle_scan_data_operations(
    subscriber: Arc<Mutex<NatsSubscriber>>,
    _publisher: NatsPublisher,
    _http_client: reqwest::Client,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut subscribers = subscriber.lock().await;
    let mut new_subscriber = subscribers.client().subscribe("send.scan.>".to_string()).await?;
    info!("Listening for scan data...");

    while let Some(response_msg) = new_subscriber.next().await {
        let response_payload = String::from_utf8_lossy(&response_msg.payload);
        info!("Received raw response: {}", response_payload);

        let json: Value = match serde_json::from_str(&response_payload) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to parse JSON: {}", e);
                continue;
            }
        };


        if let (Some(action), Some(uuid), Some(result)) = (
            json.get("action").and_then(|v| v.as_str()),
            json.get("uuid").and_then(|v| v.as_str()),
            json.get("result"),
        ) {
            info!("Action: {}, UUID: {}, Result: {}", action, uuid, result);


            if let Err(e) = scan_data_to_server(result, uuid, action).await {
                error!("Failed to send scan data to server: {}", e);
            }
        } else {
            error!("Missing required fields in received message.");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    info!("Bridge Application starting...");
    let publisher = create_publisher().await?;

     if let Err(e) = handle_master_key_operations(publisher.clone()).await {
        error!("Master key handling failed: {:?}", e);
        return Err(e);
    }

    if let Err(e) = handle_nats_operations(publisher.clone()).await {
        error!("Error in NATS operations: {:?}", e);
    }

    signal::ctrl_c().await?;
    info!("Bridge shutting down gracefully...");
    Ok(())
}