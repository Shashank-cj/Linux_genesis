use models_database::{initialize, db};
use std::env;
use std::io::{self, Write};


fn main() {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        // Production DB init (Debian postinst)
        Some("--init-db") => {
            production_setup();
        }

        // Store secret into encrypted DB
        Some("store-secret") => {
            store_secret(&args);
        }


        // Default: dev workflow
        _ => {
            if let Err(e) = initialize() {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            println!("models_database initialized successfully.");
        }
    }
}




fn production_setup() {
    println!("Setting up database for production...");

    // 1. Initialize DB (existing logic)
    if let Err(e) = initialize() {
        eprintln!("Initialize failed: {}", e);
        std::process::exit(1);
    }

    // 2. Run migrations
    if let Err(e) = run_migrations() {
        eprintln!("Migration failed: {}", e);
        std::process::exit(1);
    }

    println!("Production database setup complete!");
}



fn run_migrations() -> Result<(), String> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

    println!("Running encrypted migrations...");

    let mut connection = models_database::establish_encrypted_connection();

    let applied = connection
        .run_pending_migrations(MIGRATIONS)
        .map_err(|e| format!("Migration failed: {}", e))?;

    if applied.is_empty() {
        println!("No pending migrations.");
    } else {
        println!("Applied {} migrations.", applied.len());
    }

    Ok(())
}



fn store_secret(args: &[String]) {
    let key = get_arg(args, "--key");
    let file = get_arg(args, "--file");

    let data = std::fs::read(&file).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", file, e);
        std::process::exit(1);
    });

    let mut conn = models_database::establish_encrypted_connection();
    db::insert_secret(&mut conn, &key, &data).unwrap_or_else(|e| {
        eprintln!("Failed to store secret: {}", e);
        std::process::exit(1);
    });

    println!("Stored secret: {}", key);
}



fn get_arg(args: &[String], name: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("Missing argument: {}", name);
            std::process::exit(1);
        })
}
