use async_nats::ConnectOptions;
use serde::Serialize;
use std::sync::Arc;
use crate::load_tls_certificates;
use nkeys::KeyPair;
use std::error::Error;
 
#[derive(Clone)]
pub struct NatsPublisher {
    client: async_nats::Client, // NATS client for publishing messages
}
 
impl NatsPublisher {
    /// Creates a new NATS publisher with secure TLS and JWT authentication
    pub async fn new(
        nats_url: &str,
        c_jwt: &str,
        c_nkey: &str,
        ca_cert_path: &str,
        client_cert_path: &str,
        client_key_path: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Load the NATS key pair and TLS certificates
        let kp = Arc::new(KeyPair::from_seed(c_nkey)?);
        let tls_config = load_tls_certificates(ca_cert_path, client_cert_path, client_key_path)?;
 
        // Connect to the NATS server with authentication and TLS
        let client = async_nats::connect_with_options(
            nats_url,
            ConnectOptions::new()
                .jwt(c_jwt.to_string(), move |nonce| {
                    let kp = Arc::clone(&kp);
                    Box::pin(async move {
                        kp.sign(&nonce).map_err(|e| async_nats::AuthError::new(e.to_string()))
                    })
                })
                .require_tls(true)
                .tls_client_config((*tls_config).clone()),
        )
        .await?;
 
        Ok(Self { client })
    }
 
    /// Publishes a serialized message to a specified NATS topic
    pub async fn publish<T: Serialize>(&self, subject: &str, message: &T) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Serialize the message to JSON
        let json = serde_json::to_string(message)?;
        // Publish the message to the specified topic
        self.client.publish(subject.to_string(), json.into()).await?;
        Ok(())
    }
}