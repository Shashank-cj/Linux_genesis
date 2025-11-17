use std::env;

pub struct Config {
    pub app_dir: String,
    pub b_jwt_path: String,
    pub b_nkey_path: String,
    pub nats_url: String,
    pub c_jwt_path: String,
    pub c_nkey_path: String,
    pub jwt_private_key_path: String,
    pub jwt_public_key_path: String,
    pub ca_cert_path: String,
    pub bridge_cert_path: String,
    pub bridge_key_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,
    pub central_server_url: String,
    pub db_path: String,
    pub web_socket_url: String,
}

impl Config {
    pub fn new() -> Self {
        // Auto-detect production vs development
        let is_production = std::path::Path::new("/var/lib/genesis_agent").exists();

        // Application directory
        let app_dir = env::var("GENESIS_AGENT_APP_DIR").unwrap_or_else(|_| {
            if is_production {
                "/var/lib/genesis_agent".to_string()
            } else {
                "/mnt/backup/genesis_agent".to_string()
            }
        });

        // Database path - consistent with Python config
        let db_path = env::var("GENESIS_AGENT_DB_PATH").unwrap_or_else(|_| {
            if is_production {
                "/var/lib/genesis_agent/monitoring.db".to_string()
            } else {
                format!("{}/models_database/models_database.sqlite", app_dir)
            }
        });

        Self {
            app_dir: app_dir.clone(),

            // Bridge paths
            b_jwt_path: format!("{}/nats/nsc_creds/BridgeUser.jwt", app_dir),
            b_nkey_path: format!("{}/nats/nsc_creds/BridgeUser.nk", app_dir),
            bridge_cert_path: format!("{}/nats/nats_config/certificate/bridge-cert.pem", app_dir),
            bridge_key_path: format!("{}/nats/nats_config/certificate/bridge-key.pem", app_dir),

            // Collector paths
            c_jwt_path: format!("{}/nats/nsc_creds/CollectorUser.jwt", app_dir),
            c_nkey_path: format!("{}/nats/nsc_creds/CollectorUser.nk", app_dir),
            client_cert_path: format!("{}/nats/nats_config/certificate/collector-cert.pem", app_dir),
            client_key_path: format!("{}/nats/nats_config/certificate/collector-key.pem", app_dir),

            // Common paths
            jwt_private_key_path: format!("{}/nats/jwt_keys/private.pem", app_dir),
            jwt_public_key_path: format!("{}/nats/jwt_keys/public.pem", app_dir),
            ca_cert_path: format!("{}/nats/nats_config/certificate/ca-cert.pem", app_dir),

            // Network
            nats_url: "tls://127.0.0.1:4222".to_string(),
            central_server_url: env::var("GENESIS_AGENT_CENTRAL_SERVER_URL")
                .unwrap_or_else(|_| "https://192.168.100.13".to_string()),
            web_socket_url: env::var("GENESIS_AGENT_WEB_SOCKET_URL")
                .unwrap_or_else(|_| "wss://192.168.100.13".to_string()),

            db_path,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref CONFIG: Config = Config::new();
}
