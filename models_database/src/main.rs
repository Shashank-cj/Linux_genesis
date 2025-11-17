use models_database::initialize;
use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    // Check if we're running in production mode
    if args.len() > 1 && args[1] == "--init-db" {
        // Production mode - for the Debian package
        production_setup();
    } else {
        // Development mode - your current workflow
        if let Err(e) = initialize() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        println!("models_database initialized successfully.");
    }
}

fn production_setup() {
    println!("Setting up database for production...");
    
    // 1. First run your existing initialize function
    if let Err(e) = initialize() {
        eprintln!("Initialize failed: {}", e);
        std::process::exit(1);
    }
    
    // 2. Then run diesel migrations
    if let Err(e) = run_migrations() {
        eprintln!("Migration failed: {}", e);
        std::process::exit(1);
    }
    
    println!("Production database setup complete!");
}

fn run_migrations() -> Result<(), String> {
    use diesel::prelude::*;
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    
    // This includes your migration files in the binary
    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");
    
    // Get database path - production vs development
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| {
            if std::path::Path::new("/var/lib/genesis_agent").exists() {
                    "sqlite:///var/lib/genesis_agent/monitoring.db".to_string()
                }
            else {
                "sqlite://models_database.sqlite".to_string()  
            }
        });
    
    println!("Running migrations on: {}", database_url);
    
    // Connect and run migrations
    let mut connection = SqliteConnection::establish(&database_url)
        .map_err(|e| format!("Failed to connect: {}", e))?;
    
    let migration_results = connection.run_pending_migrations(MIGRATIONS)
        .map_err(|e| format!("Migration failed: {}", e))?;
    
    if migration_results.is_empty() {
        println!("No pending migrations found.");
    } else {
        println!("Applied {} migrations successfully!", migration_results.len());
    }
    
    Ok(())
}
