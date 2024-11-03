//Third-party-dependencies
use actix_web::{App, HttpServer};
use routes::file_routes;

// Module imports:
mod routes;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // address the server will run on
    let address = "127.0.0.1:9090"; //TODO: implement server hosted envirment variable
    println!("Server started at {}", address);
    std::fs::create_dir_all("./storage")?; //TODO: Review stored files
    HttpServer::new(|| {
        App::new()
            .configure(file_routes::init_routes) // utilises methods from routes
    })
        .bind(address)?
        .run()
        .await
}
//TEST: Launch server on http://127.0.0.1:9090/
