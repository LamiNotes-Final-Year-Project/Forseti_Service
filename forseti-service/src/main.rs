//Third-party-dependencies
use actix_web::{get, App, HttpServer, Responder, HttpResponse};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // address the server will run on
    let address = "127.0.0.1:9090";
    println!("Server started at {}", address);
    HttpServer::new(|| {
        App::new()
            .service(hello)
    })
        .bind(address)?
        .run()
        .await
}
//TEST: Launch server on http://127.0.0.1:9090/

//CRUD OPERATIONS

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello, Laminotes!")
}