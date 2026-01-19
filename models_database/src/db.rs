use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use std::fs;
use std::path::Path;
use crate::models::{AgentCredential,Token};
use crate::schema::agent::dsl::{agent, os};
use crate::schema::agent_credential::dsl::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::initail_response::{insert_or_update, store_json_data,delete_action}; 
use chrono::NaiveDateTime;
use diesel::sql_types::Binary;
use diesel::sql_types::Text;

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerResponse {
    pub uuid: String,
    pub client_id: String,
    pub client_secret: String,
    pub master_key: String,
}

#[derive(QueryableByName)]
struct SecretRow {
    #[diesel(sql_type = Text)]
    key: String,

    #[diesel(sql_type = Binary)]
    value: Vec<u8>,
}

// pub fn establish_connection(db_path: &str) -> SqliteConnection {
//     let db_dir = Path::new(db_path).parent().unwrap_or_else(|| Path::new("."));

//     // Create directory if it doesn't exist
//     if !db_dir.exists() {
//         println!("Creating directory: {:?}", db_dir);
//         fs::create_dir_all(db_dir).expect("Failed to create database directory");
//     }

//     // Create database file if it doesn't exist
//     if !Path::new(db_path).exists() {
//         println!("Creating database at: {}", db_path);
//         fs::File::create(db_path).expect("Failed to create database file");
//     }

//     // Connect to the database
//     let conn = SqliteConnection::establish(db_path)
//         .unwrap_or_else(|_| panic!("Error connecting to {}", db_path));
//     conn
// }

// Function to save the response into the database
pub fn save_agent(conn: &mut SqliteConnection, response: &ServerResponse) -> Result<(), diesel::result::Error> {
    let new_agent = AgentCredential {
        id: None, // Changed to None for auto-increment
        uuid: response.uuid.clone(),
        client_id: response.client_id.clone(),
        client_secret: response.client_secret.clone(),
        master_key: response.master_key.clone(),
    };

    diesel::insert_into(agent_credential)
        .values(&new_agent)
        .execute(conn)?;

    Ok(())
}

pub fn is_agent_onboarded(conn: &mut SqliteConnection) -> bool {
    use crate::schema::agent_credential::dsl::*;
    use diesel::prelude::*;

    match agent_credential
        .select(id)
        .first::<Option<i32>>(conn)
    {
        Ok(_) => true, 
        Err(diesel::result::Error::NotFound) => false, 
        Err(_) => {
            println!("[ERROR] Failed to check onboarding status from the database.");
            false
        }
    }
}

pub fn get_agent_credential(conn: &mut SqliteConnection) -> Option<AgentCredential> {
    agent_credential
        .limit(1)
        .first::<AgentCredential>(conn)
        .ok()
} 

pub fn get_agent_details(conn: &mut SqliteConnection) -> Option<String> {
    agent
        .select(os)
        .limit(1)
        .first::<String>(conn)
        .ok()
}

pub fn initial_data_save(conn: &mut SqliteConnection, json_data: &Value) -> Result<(), diesel::result::Error> {
    store_json_data(conn, json_data)?; // Fixed function call
    Ok(())
}

pub fn save_token(conn: &mut SqliteConnection, token_str: &str, expiration_str: &str, token_type_str: &str) -> Result<(), diesel::result::Error> {
    use crate::schema::tokens::dsl::*;

    diesel::insert_into(tokens)
        .values((
            token.eq(token_str),
            expiration.eq(expiration_str),
            token_type.eq(token_type_str),
        ))
        .on_conflict(token_type)
        .do_update()
        .set((
            token.eq(token_str),
            expiration.eq(expiration_str),
        ))
        .execute(conn)?;

    Ok(())
}

pub fn get_token(conn: &mut SqliteConnection, token_type_str: &str) -> Option<Token> {
    use crate::schema::tokens::dsl::*;
    
    let result = tokens
        .filter(token_type.eq(token_type_str))
        .first::<Token>(conn)
        .optional()
        .unwrap_or(None);

    result.and_then(|t| {
        if let Ok(exp_time) = NaiveDateTime::parse_from_str(&t.expiration, "%Y-%m-%d %H:%M:%S") {
            if exp_time > chrono::Local::now().naive_local() {
                return Some(t);
            }
        }
        None
    })
}

pub fn token_exists(conn: &mut SqliteConnection, token_type_str: &str) -> bool {
    use crate::schema::tokens::dsl::*;

    tokens
        .filter(token_type.eq(token_type_str))
        .first::<Token>(conn)
        .optional()
        .unwrap_or(None)
        .is_some()
}

pub fn update_initial_data(conn: &mut SqliteConnection, action: &str, json_data: &Value) -> Result<(), diesel::result::Error> {
    if action == "disk"|| action == "nic" {
        if let Some(devices) = json_data.as_array() {
            match insert_or_update(conn, devices) {
                Ok(_) => println!("updated successfully."),
                Err(e) => eprintln!("Error: {:?}", e),
            }
        } else {
            eprintln!("Expected an array for disk action, but got something else.");
        }
    }
    Ok(())
} 

pub fn delete_initial_data(conn: &mut SqliteConnection, json_data: &Value) -> Result<(), diesel::result::Error> {
    delete_action(conn, json_data)
        .map_err(|e| {
            eprintln!("Error deleting data: {:?}", e);
            diesel::result::Error::RollbackTransaction
        })
        .map(|_| println!("Data deleted successfully."))
   
}

pub fn insert_secret( conn: &mut SqliteConnection,k: &str,v: &[u8],) -> QueryResult<()> {
    diesel::sql_query(
        "INSERT OR REPLACE INTO secrets (key, value) VALUES (?, ?)"
    )
    .bind::<diesel::sql_types::Text, _>(k)
    .bind::<diesel::sql_types::Binary, _>(v)
    .execute(conn)?;
    Ok(())
}


use std::collections::HashMap;

pub fn load_all_secrets(conn: &mut SqliteConnection) -> anyhow::Result<HashMap<String, Vec<u8>>> {
    let rows: Vec<SecretRow> =
        diesel::sql_query("SELECT key, value FROM secrets")
            .load(conn)?;

    let mut secrets = HashMap::new();
    for row in rows {
        secrets.insert(row.key, row.value);
    }

    Ok(secrets)
}