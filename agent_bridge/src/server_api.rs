use serde::{Serialize,Deserialize};
use tracing::{info, error,warn};
use std::collections::HashMap;
use reqwest::Client;
use reqwest::Response;
use serde_json::Value;
use base64::engine::general_purpose;
use base64::Engine;
use shared_config::CONFIG;

use models_database::db::{
    establish_connection, get_agent_credential, initial_data_save, is_agent_onboarded, save_agent ,update_initial_data
};
use tokio_tungstenite::{connect_async_tls_with_config, Connector, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
type WSStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
use futures::SinkExt;
use tokio::sync::Mutex;
use tokio::net::TcpStream;
use std::sync::Arc;

use url::Url;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use anyhow::{Result, Context};

use native_tls::TlsConnector as NativeTlsConnector;
use tungstenite::client::IntoClientRequest;
use tokio_tungstenite::MaybeTlsStream;
use futures_util::StreamExt;

fn base_url() -> &'static str {
    CONFIG.central_server_url.as_str()
}


lazy_static::lazy_static! {
    static ref WS_CONNECTION: Arc<Mutex<Option<WSStream>>> = Arc::new(Mutex::new(None));
}

#[derive(Serialize,Deserialize, Debug)]
struct MasterKeyPayload {
    master_key: String,
    hostname: String,
    os: String,
    os_version: String,
    mac_hash: String,
}

/// Submits the master key to the server for onboarding
pub async fn send_master_key_to_server(received_payload: &str) -> Result<(), Box<dyn std::error::Error>> {
    
    let mut conn = establish_connection(&CONFIG.db_path);

    if is_agent_onboarded(&mut conn) {
        println!("[INFO] Agent is already onboarded. Skipping server call.");
        info!("Agent already onboarded. Skipping master key submission.");
        return Ok(());
    }

    let payload: MasterKeyPayload = serde_json::from_str(received_payload)?;

    let api_key = "1234567890abcdef1234567890abcdef";

    let central_server_url = base_url().to_string() + "/api/agent/onboard/";

    let client = reqwest::Client::builder().danger_accept_invalid_certs(true).build()?;

    let response = client
        .post(central_server_url)
        .header("X-API-KEY", api_key)
        .json(&payload)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;
    info!("Response from server: {}", response_text);

    if status.is_success() {
        let parsed_response: models_database::db::ServerResponse = serde_json::from_str(&response_text)?;

        match save_agent(&mut conn, &parsed_response) {
            Ok(_) => {
                println!("[SUCCESS] Response saved to database!");
                Ok(())
            }
            Err(e) => {
                println!("[ERROR] Failed to save data: {}", e);
                Err(Box::new(e))
            }
        }
    } else {
        println!("[ERROR] Failed to back up master key. Status: {}", status);
        Ok(())
    }
}

/// Retrieves a new access token using saved client credentials
pub async fn get_new_access_token(token_type: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut conn = establish_connection(&CONFIG.db_path);

    let credential = match get_agent_credential(&mut conn) {
        Some(cred) => cred,
        None => {
            println!("[ERROR] No agent credentials found in database.");
            return Err("No credentials found".into());
        }
    };
    info!("Agent credentials found in database: {:?}", credential);

    let mut form_data = HashMap::new();
    form_data.insert("grant_type".to_string(), "client_credentials".to_string());
    form_data.insert("client_id".to_string(), credential.client_id.clone());
    form_data.insert("client_secret".to_string(), credential.client_secret.clone());

    info!("Calling..........................: {}", token_type);
    let url = match token_type {
        "access_token" => format!("{}/api/agent/get/jwt/access_token/", base_url()),
        _ => format!("{}/api/agent/get/jwt/", base_url()),
    };

    let client = reqwest::Client::builder().danger_accept_invalid_certs(true).build()?;
    let response = client
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("uuid", credential.uuid)
        .form(&form_data)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;

    if status.is_success() {
        info!("Successfully fetched access token.");
        Ok(body)
    } else {
        error!("Failed to get access token: {}", body);
        Err(format!("Token error: {status}").into())
    }
}

pub async fn send_to_server(data: &str, token: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut conn = establish_connection(&CONFIG.db_path);
    let url = format!("{}/api/agent/init/data/", base_url());
    let agent_uuid = match get_agent_credential(&mut conn) {
        Some(cred) => cred.uuid,
        None => {
            println!("[ERROR] No agent UUID found in database.");
            return Err("No UUID found".into());
        }
    };
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("Failed to create reqwest client with SSL validation");


    let json_data: serde_json::Value = serde_json::from_str(data).expect("Failed to parse JSON data");
    let response=client
                .post(url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", token))
                .header("uuid", agent_uuid)
                .json(&json_data)
                .send()
                .await?;
    let status = response.status();
    let response_text = response.text().await?;

    if status.is_success() {
        info!("Response from server: {}", response_text);
        let json_data: Value = serde_json::from_str(&response_text)?;
        match initial_data_save(&mut conn, &json_data) {
            Ok(_) => {
                info!("Response data stored successfully");
                return Ok("Data stored successfully".to_string());
            }
            Err(e) => eprintln!("Error storing data: {:?}", e),
        }
    } else {
        error!(
            "[ERROR] Failed to send agent data. Status: {}",
            status
        );
    }

    Ok("Initial Data send to server not store in database".to_string())
}


pub async fn send_to_monitor_server(data: &str, access_token: &str) -> Result<String, String> {
    let mut conn = establish_connection(&CONFIG.db_path);

    let agent_uuid = match get_agent_credential(&mut conn) {
        Some(cred) => cred.uuid,
        None => return Err("No UUID found".into()),
    };

    match send_via_websocket(data, access_token, &agent_uuid).await {
        Ok(response) => Ok(response), 
        Err(e) => {
            warn!("WebSocket failed: {}. Falling back to HTTPS.", e);
            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
            match send_via_https(data, access_token, &agent_uuid).await {
                Ok(response) => {
                    match web_socket_connection(access_token, &agent_uuid).await {
                        Ok(ws_stream) => {
                             let mut ws_stream_lock = WS_CONNECTION.lock().await;
                            *ws_stream_lock = Some(ws_stream);
                             println!("WebSocket reconnected successfully!");
                        },
                        Err(reconnect_error) => {
                            warn!("WebSocket reconnect failed after HTTPS. {}", reconnect_error);
                        }
                    }
                    
                    Ok(response)
                },
                Err(e) => Err(format!("Both WebSocket and HTTPS failed: {}", e))
            }
        }
    }
}

// Function to send data via HTTPS
async fn send_via_https(data: &str, access_token: &str, agent_uuid: &str) -> Result<String, anyhow::Error> {
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    let bridge_url = format!("{}/api/agent/bridge/", base_url());

    println!("Sending data to the HTTPS server");

    let response = client
        .post(bridge_url)
        .header("Content-Type", "application/json")
        .header("access-token", access_token)
        .header("uuid", agent_uuid)
        .body(data.to_string())
        .send()
        .await?;

        let text_response = response.text().await?;
        Ok(text_response)
}

// Function to send data via WebSocket 
async fn send_via_websocket(data: &str,access_token: &str,agent_uuid: &str,) -> Result<String, anyhow::Error> {
    let mut ws_stream_lock = WS_CONNECTION.lock().await;

    if ws_stream_lock.is_none() {
        *ws_stream_lock = Some(web_socket_connection(access_token, agent_uuid).await?);
    }

    let ws_stream = ws_stream_lock.as_mut().unwrap();

    // Send data
    ws_stream
        .send(Message::Text(data.to_string()))
        .await
        .context("Failed to send message")?;

    println!("Data sent");

    // Wait for the Text message
    loop {
        if let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Text(response)) => return Ok(response),
                Ok(Message::Ping(payload)) => {
                    ws_stream.send(Message::Pong(payload)).await.ok();
                    continue;
                }
                Ok(Message::Pong(_)) => {
                    continue;
                }
                Ok(other) => {
                    return Err(anyhow::anyhow!("Unexpected WebSocket message: {:?}", other));
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("WebSocket receive error: {:?}", e));
                }
            }
        } else {
            return Err(anyhow::anyhow!("WebSocket closed unexpectedly"));
        }
    }
}

async fn web_socket_connection(access_token: &str, agent_uuid: &str) -> Result<WSStream> {
    let url = Url::parse(&format!("{}/api/agent/bridge/", CONFIG.web_socket_url)).unwrap();
    let mut req = url.clone().into_client_request().unwrap();

    let headers = req.headers_mut();
    headers.insert("uuid", HeaderValue::from_str(agent_uuid).expect("Invalid UUID header"));
    headers.insert("access-token", HeaderValue::from_str(access_token).expect("Invalid access-token header"));
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));

    let native_tls = NativeTlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("Failed to build native TLS connector");
    let connector = Some(Connector::NativeTls(native_tls));

    let (stream, _) = connect_async_tls_with_config(req, None, false, connector)
        .await
        .expect("WebSocket connection failed");

    println!("WebSocket connected!");
    Ok(stream)
}

pub async fn scan_data_to_server(data: &Value, uuid: &str,action :&str) -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = establish_connection(&CONFIG.db_path);
    let action = if action == "partition" { "disk" } else { action };
    let url = format!("{}/api/agent/init/data/{}/{}/", base_url(),uuid,action);
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;
    println!("[INFO] Sending data to server: {:?}", data);

    let response = client
        .patch(&url)
        .header("Content-Type", "application/json")
        .json(&data)
        .send()
        .await?;

    if response.status().is_success() {
        println!("[INFO] Data sent successfully to server.");
        let response_text = response.text().await?;
        let json_data: Value = serde_json::from_str(&response_text)?;
        println!("[INFO] JSON data parsed successfully: {:?}", json_data);
        match update_initial_data(&mut conn, &action, &json_data){
            Ok(_) => {
                println!("[INFO] Response updated data stored successfully");
                return Ok(());
            }
            Err(e) => eprintln!("[ERROR] Error storing data: {:?}", e),
        }
      
    } else {
        println!("[ERROR] Failed to send data to server. Status: {}", response.status());
    }
    Ok(())
}
