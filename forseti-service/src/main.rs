
#![ allow(warnings)]
#![allow(unused_imports)]
// Third-party dependencies
use actix_web::{App, HttpServer, middleware::Logger};
use actix_cors::Cors;
use dotenv::dotenv;
use std::env;
use log::info;

// Module imports
mod routes;
mod models;
mod utils;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize environment
    dotenv().ok();
    env_logger::init();

    // Create storage directories
    std::fs::create_dir_all("./storage")?;
    std::fs::create_dir_all("./storage/users")?;

    // Get configuration from environment or use defaults
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "9090".to_string());
    let address = format!("{}:{}", host, port);

    info!("Starting Laminotes server at http://{}", address);

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
            .configure(routes::file_routes::init_routes)
            .configure(routes::auth_routes::init_routes)
    })
        .bind(address)?
        .run()
        .await
}
//TEST: Launch server on http://127.0.0.1:9090/
