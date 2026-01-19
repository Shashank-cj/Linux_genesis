use anyhow::{Result, Context, bail};
use aes::Aes256;
use cbc::Decryptor;
use cipher::{KeyIvInit, BlockDecryptMut, block_padding::Pkcs7};
use sha2::{Sha256, Digest};
use std::{fs, path::Path, os::unix::fs::PermissionsExt};

use shared_config::CONFIG;
use models_database::db;
 use std::collections::HashMap;


fn main() -> Result<()> {
    
    prepare_dirs()?;
    decrypt_master_key()?;

    let mut conn = models_database::establish_encrypted_connection();
    let secrets = db::load_all_secrets(&mut conn)
        .context("Failed to load secrets from DB")?;


    /* CA */
    write_secret(&secrets, "cert.ca.pem", "ca-cert.pem")?;

    /* Bridge */
    write_secret(&secrets, "cert.bridge.pem", "bridge-cert.pem")?;
    write_secret(&secrets, "cert.bridge.key", "bridge-key.pem")?;
    write_secret(&secrets, "nats.bridge.creds", "bridge.creds")?;
    write_secret(&secrets, "nats.bridge.nk", "bridge.nk")?;
    write_secret(&secrets, "nats.bridge.jwt", "bridge.jwt")?;

    /* Collector */
    write_secret(&secrets, "cert.collector.pem", "collector-cert.pem")?;
    write_secret(&secrets, "cert.collector.key", "collector-key.pem")?;
    write_secret(&secrets, "nats.collector.creds", "collector.creds")?;
    write_secret(&secrets, "nats.collector.nk", "collector.nk")?;
    write_secret(&secrets, "nats.collector.jwt", "collector.jwt")?;

    /* Operator / system */
    write_secret(&secrets, "nats.operator.jwt", "operator.jwt")?;
    write_secret(&secrets, "nats.account.jwt", "BridgeAccount.jwt")?;
    write_secret(&secrets, "nats.system_account.pub", "system_account.pub")?;

    generate_nats_config()?;

    println!("✓ Runtime secrets and nats-server.conf prepared in /run");
    Ok(())
}


fn prepare_dirs() -> Result<()> {
    fs::create_dir_all(&CONFIG.secrets_dir)?;
    fs::set_permissions(
        &CONFIG.secrets_dir,
        fs::Permissions::from_mode(0o700),
    )?;
    Ok(())
}


fn decrypt_master_key() -> Result<()> {
    let machine_id_raw = fs::read("/etc/machine-id")?;
    let machine_id = machine_id_raw
        .strip_suffix(b"\n")
        .unwrap_or(&machine_id_raw);

    let key = Sha256::digest(machine_id);

    let encrypted = fs::read(&CONFIG.master_key_enc)?;
    if encrypted.len() <= 16 {
        bail!("Encrypted key too small");
    }

    let (iv, ciphertext) = encrypted.split_at(16);
    let mut buffer = ciphertext.to_vec();

    // ✅ ONLY correct way to do AES-256-CBC + PKCS7
    let plain = Decryptor::<Aes256>::new_from_slices(&key, iv)
        .map_err(|_| anyhow::anyhow!("Invalid AES key or IV"))?
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|_| anyhow::anyhow!("AES decrypt failed"))?;

    let out = Path::new(&CONFIG.secrets_dir).join("master_key.bin");
    fs::write(&out, plain)?;
    fs::set_permissions(&out, fs::Permissions::from_mode(0o600))?;

    Ok(())
}



fn write_secret( secrets: &HashMap<String, Vec<u8>>,key: &str,filename: &str,) -> Result<()> {
    let data = secrets
        .get(key)
        .ok_or_else(|| anyhow::anyhow!("Missing secret {}", key))?;

    let path = Path::new(&CONFIG.secrets_dir).join(filename);
    fs::write(&path, data)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}


fn generate_nats_config() -> Result<()> {
    let hostname = hostname::get()?.to_string_lossy().into_owned();
    let mut template = fs::read_to_string(&CONFIG.nats_template)?;

    template = template
        .replace("{{HOSTNAME}}", &hostname)
        .replace(
            "{{OPERATOR_JWT_PATH}}",
            &format!("{}/operator.jwt", CONFIG.secrets_dir),
        )
        .replace("{{JWT_STORE_PATH}}", &CONFIG.secrets_dir)
        .replace("{{JETSTREAM_PATH}}", &CONFIG.jetstream_dir)
        .replace("{{CERT_PATH}}", &CONFIG.secrets_dir);

    let sys_pub = fs::read_to_string(
        Path::new(&CONFIG.secrets_dir).join("system_account.pub"),
    )?;

    template.push_str(&format!(
        "\nsystem_account: {}\n",
        sys_pub.trim()
    ));

    fs::write(&CONFIG.nats_runtime_config, template)?;
    fs::set_permissions(
        &CONFIG.nats_runtime_config,
        fs::Permissions::from_mode(0o600),
    )?;

    Ok(())
}
