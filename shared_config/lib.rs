use std::env;

pub struct Config {
    pub app_dir: String,

    pub b_jwt_path: String,
    pub b_nkey_path: String,
    pub c_jwt_path: String,
    pub c_nkey_path: String,

    pub ca_cert_path: String,
    pub bridge_cert_path: String,
    pub bridge_key_path: String,
    pub client_cert_path: String,
    pub client_key_path: String,

    pub nats_url: String,
    pub central_server_url: String,
    pub web_socket_url: String,
    pub secrets_dir:String,
    pub db_path: String,
    pub db_key:String,
    pub monitor_interval_secs: u64,
    pub monitor_interval: u64,
    pub python_venv_dir: String,

    pub master_key_enc: String,
    pub jetstream_dir: String,
    pub nats_template: String,
    pub nats_runtime_config: String,

}

impl Config {
    pub fn new() -> Self {
        let app_dir = env::var("GENESIS_AGENT_APP_DIR")
            .unwrap_or_else(|_| "/var/lib/genesis_agent".to_string());

      
        let secrets_dir = env::var("GENESIS_AGENT_SECRETS_DIR")
            .unwrap_or_else(|_| "/run/genesis_agent/secrets".to_string());

        let db_path = env::var("GENESIS_AGENT_DB_PATH")
            .unwrap_or_else(|_| format!("{}/monitoring.db", app_dir));

        let db_key = env::var("GENESIS_AGENT_DB_KEY")
            .unwrap_or_else(|_| "/etc/genesis_agent/config.env".to_string());

        let python_venv_dir = env::var("GENESIS_AGENT_PYTHON_VENV")
            .unwrap_or_else(|_| "/opt/genesis_agent/venv".to_string());

        let monitor_interval_secs = env::var("GENESIS_AGENT_MONITOR_INTERVAL_SEC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2);

        let monitor_interval = env::var("GENESIS_AGENT_MONITOR_INTERVAL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);

        Self {
            app_dir: app_dir.clone(),
            master_key_enc: format!("{}/master_key.enc", app_dir),
            jetstream_dir: format!("{}/jetstream", app_dir),
            nats_template: "/opt/genesis_agent/nats.conf.template".to_string(),
            nats_runtime_config: "/run/genesis_agent/nats-server.conf".to_string(),


            b_jwt_path: format!("{}/bridge.jwt", secrets_dir),
            b_nkey_path: format!("{}/bridge.nk", secrets_dir),
            bridge_cert_path: format!("{}/bridge-cert.pem", secrets_dir),
            bridge_key_path: format!("{}/bridge-key.pem", secrets_dir),

     
            c_jwt_path: format!("{}/collector.jwt", secrets_dir),
            c_nkey_path: format!("{}/collector.nk", secrets_dir),
            client_cert_path: format!("{}/collector-cert.pem", secrets_dir),
            client_key_path: format!("{}/collector-key.pem", secrets_dir),

         
            ca_cert_path: format!("{}/ca-cert.pem", secrets_dir),

         
            nats_url: env::var("GENESIS_AGENT_NATS_URL")
                .unwrap_or_else(|_| "tls://127.0.0.1:4222".to_string()),

            central_server_url: env::var("GENESIS_AGENT_CENTRAL_SERVER_URL")
                .unwrap_or_else(|_| "https://192.168.100.13".to_string()),

            web_socket_url: env::var("GENESIS_AGENT_WEB_SOCKET_URL")
                .unwrap_or_else(|_| "wss://192.168.100.13".to_string()),

            db_path,
            monitor_interval_secs,
            monitor_interval,
            python_venv_dir,
            secrets_dir,
            db_key,
        }
    }
}

lazy_static::lazy_static! {
    pub static ref CONFIG: Config = Config::new();
}
