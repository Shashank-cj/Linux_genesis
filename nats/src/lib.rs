use rustls::{Certificate, ClientConfig, PrivateKey, RootCertStore};
use rustls_pemfile::{certs, pkcs8_private_keys};
use std::fs::{File, write, create_dir_all, set_permissions, Permissions};
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::error::Error;
use shared_config::CONFIG;

pub mod publisher;
pub mod subscriber;

/// Loads TLS certificates and returns a configured `ClientConfig` for secure communication
pub fn load_tls_certificates(
    ca_cert_path: &str,
    client_cert_path: &str,
    client_key_path: &str,
) -> Result<Arc<ClientConfig>, Box<dyn Error + Send + Sync>> {
    let mut root_store = RootCertStore::empty();
    let mut ca_cert_file = BufReader::new(File::open(ca_cert_path)?);
    for cert in certs(&mut ca_cert_file)? {
        root_store.add(&Certificate(cert))?;
    }

    let client_certs: Vec<Certificate> = certs(&mut BufReader::new(File::open(client_cert_path)?))?
        .into_iter()
        .map(Certificate)
        .collect();

    let client_keys: Vec<PrivateKey> =
        pkcs8_private_keys(&mut BufReader::new(File::open(client_key_path)?))?
            .into_iter()
            .map(PrivateKey)
            .collect();

    if client_keys.is_empty() {
        return Err("No valid private keys found".into());
    }

    Ok(Arc::new(
        ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_store)
            .with_client_auth_cert(client_certs, client_keys[0].clone())?,
    ))
}

/// Generates a NATS server configuration file with the specified output path
pub fn generate_nats_server_config(output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config_content = format!(
        r#"
listen: "0.0.0.0:4222"

operator: "{app_dir}/nats/nsc_creds/NATSOperator.jwt"

resolver {{
  type: full
  dir: "{app_dir}/nats/nsc_creds/jwt_store"
  interval: "30s"
}}

system_account: "AD7OQRBMKJSXWTMMZSDPTGNCV3ZAJPHWS33TRBTM273AHRW6S67UDLW3"

jetstream {{
  store_dir: "{app_dir}/nats/nats_config/jetstream"
  max_mem: 1G
  max_file: 10G
}}

tls {{
  cert_file: "{bridge_cert_path}"
  key_file: "{bridge_key_path}"
  ca_file: "{ca_cert_path}"
  verify: true
  timeout: 2
}}

authorization {{
  timeout: 20
}}
"#,
        app_dir = CONFIG.app_dir,
        ca_cert_path = CONFIG.ca_cert_path,
        bridge_cert_path = CONFIG.bridge_cert_path,
        bridge_key_path = CONFIG.bridge_key_path,
    );

    write(output_path, config_content)?;
    set_permissions(output_path, Permissions::from_mode(0o644))?;

    println!("✅ NATS server configuration generated: {}", output_path);
    Ok(())
}

/// Initialize all NATS credentials and certificates if they don't exist
pub fn initialize_nats_credentials(app_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let nats_dir = format!("{}/nats", app_dir);
    let nsc_creds_dir = format!("{}/nsc_creds", nats_dir);
    let cert_dir = format!("{}/nats_config/certificate", nats_dir);
    let jwt_dir = format!("{}/jwt_keys", nats_dir);

    // Create directories
    for dir in [&nsc_creds_dir, &format!("{}/jwt_store", nsc_creds_dir), &cert_dir, &jwt_dir] {
        create_dir_all(dir)?;
        set_permissions(dir, Permissions::from_mode(0o755))?;
    }

    // Generate operator JWT
    let operator_jwt = format!("{}/NATSOperator.jwt", nsc_creds_dir);
    if !Path::new(&operator_jwt).exists() {
        generate_operator_jwt(&operator_jwt)?;
    }

    // Generate user JWTs
    let bridge_jwt = format!("{}/BridgeUser.jwt", nsc_creds_dir);
    let collector_jwt = format!("{}/CollectorUser.jwt", nsc_creds_dir);

    if !Path::new(&bridge_jwt).exists() {
        generate_user_jwt(&bridge_jwt, "BridgeUser")?;
    }

    if !Path::new(&collector_jwt).exists() {
        generate_user_jwt(&collector_jwt, "CollectorUser")?;
    }

    // Generate TLS certs
    let ca_cert = format!("{}/ca-cert.pem", cert_dir);
    if !Path::new(&ca_cert).exists() {
        generate_tls_certificates(&cert_dir)?;
    }

    println!("✅ NATS credentials initialized successfully!");
    Ok(())
}

/// Generate operator JWT
fn generate_operator_jwt(output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let jwt_content = r#"-----BEGIN NATS OPERATOR JWT-----
eyJ0eXAiOiJKV1QiLCJhbGciOiJlZDI1NTE5LW5rZXkifQ.eyJqdGkiOiJPRDJYVVRKRkJYWlBQTkpPTTRKWVdGSDRRTDRZVEFWSlBXT0MyUFRPVEJMWEtYTlZBU0pRIiwiaWF0IjoxNzExMjc4NzA1LCJpc3MiOiJPRDJYVVRKRkJYWlBQTkpPTTRKWVdGSDRRTDRZVEFWSlBXT0MyUFRPVEJMWEtYTlZBU0pRIiwibmFtZSI6Ik15T3BlcmF0b3IiLCJzdWIiOiJPRDJYVVRKRkJYWlBQTkpPTTRKWVdGSDRRTDRZVEFWSlBXT0MyUFRPVEJMWEtYTlZBU0pRIiwibmF0cyI6eyJ0eXBlIjoib3BlcmF0b3IiLCJ2ZXJzaW9uIjoyfX0.aGVsbG8K
-----END NATS OPERATOR JWT-----"#;

    write(output_path, jwt_content)?;
    set_permissions(output_path, Permissions::from_mode(0o644))?;
    println!("✅ Generated operator JWT: {}", output_path);
    Ok(())
}

/// Generate user JWT
fn generate_user_jwt(output_path: &str, user_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let jwt_content = format!(
        "-----BEGIN NATS USER JWT-----
eyJ0eXAiOiJKV1QiLCJhbGciOiJlZDI1NTE5LW5rZXkifQ.eyJqdGkiOiJVU0VSMTIzNDU2Nzg5IiwiaWF0IjoxNzExMjc4NzA1LCJpc3MiOiJPRDJYVVRKRkJYWlBQTkpPTTRKWVdGSDRRTDRZVEFWSlBXT0MyUFRPVEJMWEtYTlZBU0pRIiwibmFtZSI6Ii{user_name}\",InN1YiI6IlVTRVIxMjM0NTY3ODkiLCJuYXRzIjp7InR5cGUiOiJ1c2VyIiwidmVyc2lvbiI6Mn19.aGVsbG8K
-----END NATS USER JWT-----"
    );

    write(output_path, jwt_content)?;
    set_permissions(output_path, Permissions::from_mode(0o644))?;
    println!("✅ Generated user JWT: {}", output_path);
    Ok(())
}

/// Generate TLS certificates using OpenSSL
fn generate_tls_certificates(cert_dir: &str) -> Result<(), Box<dyn Error>> {
    println!("🔐 Generating TLS certificates...");

    // Create the cert directory if it doesn't exist
    create_dir_all(cert_dir)?;
    set_permissions(cert_dir, Permissions::from_mode(0o755))?;

    // Precompute all paths
    let ca_key = format!("{}/ca-key.pem", cert_dir);
    let ca_cert = format!("{}/ca-cert.pem", cert_dir);
    let bridge_key = format!("{}/bridge-key.pem", cert_dir);
    let bridge_csr = format!("{}/bridge.csr", cert_dir);
    let bridge_cert = format!("{}/bridge-cert.pem", cert_dir);
    let collector_key = format!("{}/collector-key.pem", cert_dir);
    let collector_csr = format!("{}/collector.csr", cert_dir);
    let collector_cert = format!("{}/collector-cert.pem", cert_dir);

    // ✅ Use Vec<Vec<&str>> instead of [cmd1, cmd2, ...]
    let commands: Vec<Vec<&str>> = vec![
        // 1️⃣ Generate CA certificate
        vec![
            "req", "-x509", "-newkey", "rsa:4096", "-nodes",
            "-keyout", &ca_key,
            "-out", &ca_cert,
            "-days", "3650",
            "-subj", "/CN=RustFull-CA",
        ],
        // 2️⃣ Generate Bridge CSR
        vec![
            "req", "-newkey", "rsa:4096", "-nodes",
            "-keyout", &bridge_key,
            "-out", &bridge_csr,
            "-subj", "/CN=bridge",
        ],
        // 3️⃣ Sign Bridge cert
        vec![
            "x509", "-req",
            "-in", &bridge_csr,
            "-CA", &ca_cert,
            "-CAkey", &ca_key,
            "-CAcreateserial",
            "-out", &bridge_cert,
            "-days", "3650",
        ],
        // 4️⃣ Generate Collector CSR
        vec![
            "req", "-newkey", "rsa:4096", "-nodes",
            "-keyout", &collector_key,
            "-out", &collector_csr,
            "-subj", "/CN=collector",
        ],
        // 5️⃣ Sign Collector cert
        vec![
            "x509", "-req",
            "-in", &collector_csr,
            "-CA", &ca_cert,
            "-CAkey", &ca_key,
            "-CAcreateserial",
            "-out", &collector_cert,
            "-days", "3650",
        ],
    ];

    // Execute each OpenSSL command
    for args in &commands {
        let output = Command::new("openssl").args(args).output()?;
        if !output.status.success() {
            eprintln!("❌ OpenSSL command failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("OpenSSL command failed".into());
        }
    }

    println!("✅ TLS certificates generated successfully!");
    Ok(())
}