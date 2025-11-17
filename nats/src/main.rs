use std::process::Command;
use std::path::Path;
use shared_config::CONFIG;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Checking NATS initialization...");
    nats::initialize_nats_credentials(&CONFIG.app_dir)?;

    let config_path = format!("{}/nats/nats-server.conf", CONFIG.app_dir);

    if !Path::new(&config_path).exists() {
        nats::generate_nats_server_config(&config_path)?;
    }

    // Start the NATS server
    let mut child = Command::new("nats-server")
        .arg("-c")
        .arg(&config_path)
        .arg("-DV")
        .spawn()?;

    println!("✅ NATS server started with configuration: {}", config_path);
    child.wait()?;

    Ok(())
}
