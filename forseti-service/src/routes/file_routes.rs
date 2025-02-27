use crate::models::{Claims, FileMetadata, ServiceError, UploadRequest};
use crate::utils::fs_utils;
use actix_web::{delete, get, post, put, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use serde_json::json;
use std::fs;
use std::path::Path;
use uuid::Uuid;

// Welcome endpoint
#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body("Welcome to Laminotes API!\nCheck out the GitHub repo for more information at https://github.com/LamiNotes-Final-Year-Project/Forseti_Service")
}

// Get file content
#[get("/files/{filename}")]
async fn get_file(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    // Extract user claims from request extensions
    let user_id = if let Some(claims) = req.extensions().get::<Claims>() {
        claims.sub.clone()
    } else {
        "public".to_string()
    };

    // Ensures user directory exists
    fs_utils::ensure_user_directory(&user_id).map_err(|_| ServiceError::InternalServerError)?;

    let filename = path.into_inner();
    let filepath = format!("./storage/{}/{}", user_id, filename);

    // Read the file content
    match fs::read_to_string(&filepath) {
        Ok(content) => Ok(HttpResponse::Ok().content_type("text/plain").body(content)),
        Err(_) => Err(ServiceError::NotFound),
    }
}

// List all files for a user
#[get("/list-files")]
async fn list_files(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
    // Extract user claims from request extensions
    let user_id = if let Some(claims) = req.extensions().get::<Claims>() {
        claims.sub.clone()
    } else {
        "public".to_string()
    };

    // List files in the user's directory
    match fs_utils::list_user_files(&user_id) {
        Ok(file_names) => Ok(HttpResponse::Ok().json(file_names)),
        Err(_) => Err(ServiceError::InternalServerError),
    }
}

// Upload a file with optional metadata
#[post("/upload/{filename}")]
async fn upload_file(
    req: HttpRequest,
    path: web::Path<String>,
    upload_data: web::Json<UploadRequest>,
) -> Result<HttpResponse, ServiceError> {
    // Extract user claims from request extensions
    let user_id = if let Some(claims) = req.extensions().get::<Claims>() {
        claims.sub.clone()
    } else {
        // For demo purposes, use 'public' if no auth
        "public".to_string()
    };

    // Ensure user directory exists
    fs_utils::ensure_user_directory(&user_id).map_err(|_| ServiceError::InternalServerError)?;

    let filename = path.into_inner();
    let filepath = format!("./storage/{}/{}", user_id, filename);

    // Save the file content
    fs::write(&filepath, &upload_data.file_content).map_err(|_| ServiceError::InternalServerError)?;

    // If metadata exists, create or update it
    if let Some(mut metadata) = upload_data.metadata.clone() {
        // Generate file ID if not provided
        if metadata.file_id.is_none() {
            metadata.file_id = Some(Uuid::new_v4().to_string());
        }

        // Set last_modified to current time if not provided
        if metadata.last_modified.is_none() {
            metadata.last_modified = Some(Utc::now());
        }

        // Ensure filename matches
        metadata.file_name = filename.clone();

        // Save metadata to a separate file
        let metadata_path = format!("./storage/{}/{}.meta", user_id, filename);
        let metadata_json = serde_json::to_string(&metadata).map_err(|_| ServiceError::InternalServerError)?;
        fs::write(&metadata_path, metadata_json).map_err(|_| ServiceError::InternalServerError)?;
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": format!("File '{}' uploaded successfully!", filename),
        "filename": filename
    })))
}

// Get metadata for a file
#[get("/metadata/{filename}")]
async fn get_file_metadata(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    // Extract user claims from request extensions
    let user_id = if let Some(claims) = req.extensions().get::<Claims>() {
        claims.sub.clone()
    } else {
        // For demo purposes, use 'public' if no auth
        "public".to_string()
    };

    let filename = path.into_inner();
    let metadata_path = format!("./storage/{}/{}.meta", user_id, filename);

    // Read metadata file
    match fs::read_to_string(&metadata_path) {
        Ok(content) => Ok(HttpResponse::Ok().content_type("application/json").body(content)),
        Err(_) => {
            // If no metadata exists, generate default metadata
            let filepath = format!("./storage/{}/{}", user_id, filename);

            // Check if the file exists first
            if !Path::new(&filepath).exists() {
                return Err(ServiceError::NotFound);
            }

            // Create default metadata
            let metadata = FileMetadata {
                file_id: Some(Uuid::new_v4().to_string()),
                file_name: filename.clone(),
                last_modified: Some(Utc::now()),
            };

            // Save the new metadata
            let metadata_json = serde_json::to_string(&metadata).map_err(|_| ServiceError::InternalServerError)?;
            fs::write(&metadata_path, &metadata_json).map_err(|_| ServiceError::InternalServerError)?;

            Ok(HttpResponse::Ok().content_type("application/json").body(metadata_json))
        }
    }
}

// Delete a file
#[delete("/files/{filename}")]
async fn delete_file(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    // Extract user claims from request extensions
    let user_id = if let Some(claims) = req.extensions().get::<Claims>() {
        claims.sub.clone()
    } else {
        // For demo purposes, use 'public' if no auth
        "public".to_string()
    };

    let filename = path.into_inner();
    let filepath = format!("./storage/{}/{}", user_id, filename);
    let metadata_path = format!("./storage/{}/{}.meta", user_id, filename);

    // Delete the file if it exists
    if Path::new(&filepath).exists() {
        fs::remove_file(&filepath).map_err(|_| ServiceError::InternalServerError)?;
    } else {
        return Err(ServiceError::NotFound);
    }

    // Delete metadata if it exists
    if Path::new(&metadata_path).exists() {
        fs::remove_file(&metadata_path).ok(); // Ignore errors for metadata deletion
    }

    Ok(HttpResponse::Ok().json(json!({
        "message": format!("File '{}' deleted successfully!", filename)
    })))
}

// Register all file routes
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(index)
        .service(get_file)
        .service(list_files)
        .service(upload_file)
        .service(get_file_metadata)
        .service(delete_file);
}