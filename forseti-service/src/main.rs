use actix_web::{App, HttpServer, middleware::Logger};
use actix_cors::Cors;
use dotenv::dotenv;
use std::env;
use log::{info, warn};

// Import the Auth middleware and File Lock middleware
use crate::utils::{Auth, initialize_version_control};
use crate::utils::file_lock::FileLockMiddleware;

// Module imports
mod routes;
mod models;
mod utils;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize environment
    dotenv().ok();
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Create storage directories
    info!("Ensuring storage directories exist");
    std::fs::create_dir_all("./storage")?;
    std::fs::create_dir_all("./storage/users")?;
    std::fs::create_dir_all("./storage/teams")?;
    std::fs::create_dir_all("./storage/team_members")?;
    std::fs::create_dir_all("./storage/public")?;
    std::fs::create_dir_all("./storage/invitations")?;

    // Initialize version control storage
    initialize_version_control()?;

    // Get configuration from environment or use defaults
    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "9090".to_string());
    let address = format!("{}:{}", host, port);

    info!("🚀 Starting Laminotes server at http://{}", address);

    HttpServer::new(|| {
        // Configure CORS
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_headers(vec!["Authorization"])
            .max_age(3600);

        App::new()
            .wrap(Logger::default())
            .wrap(cors)
            .wrap(Auth)
            .wrap(FileLockMiddleware)
            .configure(routes::file_routes::init_routes)
            .configure(routes::auth_routes::init_routes)
            .configure(routes::team_routes::init_routes)
            .configure(routes::version_routes::init_routes)
            .configure(routes::file_lock::lock_routes::init_routes)
            .configure(routes::invitation_routes::init_routes)
            .configure(routes::claude_routes::init_routes)
    })
        .bind(address)?
        .run()
        .await
}