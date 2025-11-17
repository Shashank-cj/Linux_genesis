use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::rand_core::OsRng;
use sha2::{Sha256, Digest};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

pub struct KeyManager;

impl KeyManager {
    pub fn get_master_key_path() -> PathBuf {
        let mut path = dirs::data_dir().expect("Failed to get data directory");
        path.push("master_key_secure.dat");
        path
    }

    pub fn generate_master_key() -> Vec<u8> {
        let mut master_key = vec![0u8; 32];
        OsRng.fill_bytes(&mut master_key);

        // Derive device-specific encryption key
        let machine_id = fs::read_to_string("/etc/machine-id").unwrap_or_else(|_| "fallback".into());
        let mut hasher = Sha256::new();
        hasher.update(machine_id.as_bytes());
        let encryption_key = hasher.finalize();

        let cipher = Aes256Gcm::new_from_slice(&encryption_key).unwrap();
        
        // Generate random 12-byte nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = aes_gcm::Nonce::from_slice(&nonce_bytes);
        
        let encrypted_master_key = cipher.encrypt(nonce, master_key.as_ref()).unwrap();

        let path = Self::get_master_key_path();
        fs::create_dir_all(path.parent().unwrap()).expect("Failed to create directories");
        let mut file = File::create(&path).expect("Failed to create master key file");
        
        // Store nonce first, then encrypted data
        file.write_all(&nonce_bytes).expect("Failed to write nonce");
        file.write_all(&encrypted_master_key).expect("Failed to write master key");

        println!("[INFO] New master key generated and stored securely at {}", path.display());
        master_key
    }

    pub fn load_master_key() -> Vec<u8> {
        let path = Self::get_master_key_path();
        if path.exists() {
            let mut file = File::open(&path).expect("Failed to open master key file");
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("Failed to read master key");

            // First 12 bytes are nonce, rest is encrypted data
            let (nonce_bytes, encrypted_master_key) = data.split_at(12);

            // Derive same device-specific encryption key
            let machine_id = fs::read_to_string("/etc/machine-id").unwrap_or_else(|_| "fallback".into());
            let mut hasher = Sha256::new();
            hasher.update(machine_id.as_bytes());
            let encryption_key = hasher.finalize();

            let cipher = Aes256Gcm::new_from_slice(&encryption_key).unwrap();
            let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
            let decrypted_key = cipher.decrypt(nonce, encrypted_master_key).unwrap();

            println!("[INFO] Master key loaded securely from {}", path.display());
            return decrypted_key;
        }

        println!("[WARNING] Master key not found, generating a new one.");
        Self::generate_master_key()
    }
}
