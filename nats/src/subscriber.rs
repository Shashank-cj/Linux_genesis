use async_nats::ConnectOptions;
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use crate::load_tls_certificates;
use nkeys::KeyPair;
use std::error::Error;

pub struct NatsSubscriber {
    client: async_nats::Client, // NATS client for subscribing to topics
}

impl NatsSubscriber {
    /// Creates a new NATS subscriber with secure TLS and JWT authentication
    pub async fn new(
        nats_url: &str,
        b_jwt: &str,
        b_nkey: &str,
        ca_cert_path: &str,
        client_cert_path: &str,
        client_key_path: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Load the NATS key pair and TLS certificates
        let kp = Arc::new(KeyPair::from_seed(b_nkey)?);
        let tls_config = load_tls_certificates(ca_cert_path, client_cert_path, client_key_path)?;

        // Connect to the NATS server with authentication and TLS
        let client = async_nats::connect_with_options(
            nats_url,
            ConnectOptions::new()
                .jwt(b_jwt.to_string(), move |nonce| {
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

    /// Subscribes to a NATS topic and processes messages using a handler function
    pub async fn subscribe<F>(&self, subject: &str, handler: F) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        F: Fn(Value) + Send + 'static,
    {
        // Subscribe to the specified topic
        let mut subscription = self.client.subscribe(subject.to_string()).await?;
        tokio::spawn(async move {
            while let Some(message) = subscription.next().await {
                // Deserialize the message payload into JSON and pass it to the handler
                if let Ok(json) = serde_json::from_slice::<Value>(&message.payload) {
                    handler(json);
                }
            }
        });
        Ok(())
    }

    /// Provides direct access to the underlying NATS client
    pub fn client(&self) -> &async_nats::Client {
        &self.client
    }
}
