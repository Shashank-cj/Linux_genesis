pub mod db;
pub mod models;
pub mod schema;
pub mod initail_response; 

pub use db::{save_agent, establish_connection,initial_data_save,is_agent_onboarded,get_agent_credential}; 
 
use std::fs::write;
use shared_config::CONFIG;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::env;


pub fn establish_encrypted_connection() -> SqliteConnection {
    let db_path = env::var("RUSTFULL_DB_PATH").unwrap_or_else(|_| CONFIG.db_path.clone());
    
    let mut conn = SqliteConnection::establish(&db_path)
        .unwrap_or_else(|_| panic!("Error connecting to encrypted database at {}", db_path));

    let key = env::var("RUSTFULL_DB_KEY").unwrap_or_else(|_| "SuperSecretKey123!".to_string());

    diesel::sql_query(format!("PRAGMA key = '{}';", key))
        .execute(&mut conn) 
        .expect("Failed to set SQLCipher encryption key");

    conn
}
 
/// Generates the `diesel.toml` file dynamically using the paths from the CONFIG struct.
pub fn generate_diesel_toml() -> Result<(), Box<dyn std::error::Error>> {
    let diesel_toml_content = format!(
        r#"# For documentation on how to configure this file,
# see https://diesel.rs/guides/configuring-diesel-cli
 
[print_schema]
file = "src/schema.rs"
custom_type_derives = ["diesel::query_builder::QueryId", "Clone"]
 
[migrations_directory]
dir = "{migrations_dir}"
"#,
        migrations_dir = CONFIG.db_path.replace("models_database.sqlite", "migrations")
    );
 
    // Write the generated content to the `diesel.toml` file
    write(format!("{}/models_database/diesel.toml", CONFIG.app_dir), diesel_toml_content)?;
 
    Ok(())
}
 
/// Call this function during initialization to ensure `diesel.toml` is generated.
pub fn initialize() -> Result<(), Box<dyn std::error::Error>> {
    generate_diesel_toml()?;
    Ok(())
}
