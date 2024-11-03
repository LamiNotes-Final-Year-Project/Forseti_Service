// src/routes/file_routes.rs
// standard library
use std::fs;
use std::io;
// third-party dependencies
use actix_web::{App, get, post, web, HttpResponse, Responder};
use serde_json::json;
use std::path::Path;
const STORAGE_PATH: &str = "./storage"; //TODO: Implement group specifc file paths


// GET ROUTES
#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("Welcome to Laminotes!\nCheck out the github repo for more information on calling the API at https://github.com/LamiNotes-Final-Year-Project/Forseti_Service")
}

// Pulls files down from server
#[get("/files/{filename}")]
async fn get_file(path: web::Path<String>) -> impl Responder {
    let filename = path.into_inner();
    let filepath = format!("{}/{}", STORAGE_PATH, filename); // Save the file as a named file to the storage path //TODO: Implement group specifc file paths

    // Read the file content
    match fs::read_to_string(&filepath) {
        Ok(content) => HttpResponse::Ok().content_type("text/plain").body(content),
        Err(_) => HttpResponse::NotFound().body("File not found"),
    }
}

// Outputs list of files within directory
#[get("/list-files")]
async fn list_files() -> impl Responder {
    // Define the directory to scan for files
    let directory_path = Path::new(STORAGE_PATH); //TODO: Implement group specifc file paths

    // Read all files in the directory
    let file_names = match read_file_names(directory_path) {
        Ok(names) => names,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to read directory"),
    };

    // Return the file names as a JSON array
    HttpResponse::Ok().json(json!(file_names))
}

// POST ROUTES

// Process to upload files to the server
#[post("/upload/{filename}")]
async fn upload_file(path: web::Path<String>, body: String) -> impl Responder {
    let filename = path.into_inner();
    let filepath = format!("{}/{}", STORAGE_PATH, filename); //TODO: Implement group specifc file paths

    // Save the file content
    match fs::write(&filepath, body) {
        Ok(_) => HttpResponse::Ok().body(format!("File '{}' uploaded successfully!", filename)),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed to upload file: {}", e)),
    }
}

// PUT ROUTES


// DELETE ROUTES




// Accompanying functions

// Helper function to read file names from a directory
fn read_file_names(directory: &Path) -> io::Result<Vec<String>> {
    let mut file_names = Vec::new();

    // Iterate over found entries in directory
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        // If the path is a file, attempts to retrieve the filename and convert it to a string
        if path.is_file() {
            if let Some(file_name) = path.file_name() {
                if let Some(name_str) = file_name.to_str() {
                    file_names.push(name_str.to_string());
                }
            }
        }
    }
    Ok(file_names)
}

// Register routes function for easy import
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(upload_file).service(get_file).service(index).service(list_files);
}