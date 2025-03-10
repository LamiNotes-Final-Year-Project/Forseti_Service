use crate::models::{Claims, FileMetadata, ServiceError, TeamRole, UploadRequest};
use crate::utils::{fs_utils, team_storage};
use crate::utils::UserContext; // Import UserContext
use actix_web::{delete, get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use log::{debug, error, info};
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
    let user_context = req.extensions().get::<UserContext>().cloned().unwrap_or(UserContext {
        user_id: "public".to_string(),
        active_team_id: None
    });

    let filename = path.into_inner();
    info!("📥 Get file request: user_id={}, team={:?}, filename={}",
          user_context.user_id, user_context.active_team_id, filename);

    // Determine the storage path based on active team
    let filepath = if let Some(team_id) = user_context.active_team_id {
        // Check if user has access to this team
        if team_storage::user_has_team_access(&user_context.user_id, &team_id)? {
            format!("./storage/teams/{}/{}", team_id, filename)
        } else {
            error!("❌ User does not have access to team files");
            return Err(ServiceError::Forbidden);
        }
    } else {
        format!("./storage/{}/{}", user_context.user_id, filename)
    };

    debug!("Looking for file at path: {}", filepath);

    // Read the file content
    match fs::read_to_string(&filepath) {
        Ok(content) => {
            info!("✅ File found and read successfully: {}", filepath);
            Ok(HttpResponse::Ok().content_type("text/plain").body(content))
        },
        Err(e) => {
            error!("❌ Error reading file: {:?}", e);
            // Check if file exists to give better feedback
            if !Path::new(&filepath).exists() {
                error!("File does not exist at: {}", filepath);
            }
            Err(ServiceError::NotFound)
        }
    }
}



// List all files for a user or team
#[get("/list-files")]
async fn list_files(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
    let user_context = req.extensions().get::<UserContext>().cloned().unwrap_or(UserContext {
        user_id: "public".to_string(),
        active_team_id: None
    });

    info!("📋 List files request: user_id={}, team={:?}",
          user_context.user_id, user_context.active_team_id);

    // List files based on active team
    let file_names = if let Some(team_id) = user_context.active_team_id {
        // Check if user has access to this team
        if team_storage::user_has_team_access(&user_context.user_id, &team_id)? {
            fs_utils::list_team_files(&team_id)
        } else {
            error!("❌ User does not have access to team files");
            return Err(ServiceError::Forbidden);
        }
    } else {
        fs_utils::list_user_files(&user_context.user_id)
    }.map_err(|e| {
        error!("❌ Error listing files: {:?}", e);
        ServiceError::InternalServerError
    })?;

    info!("✅ Found {} files", file_names.len());
    Ok(HttpResponse::Ok().json(file_names))
}

// Upload a file with optional metadata
#[post("/upload/{filename}")]
async fn upload_file(
    req: HttpRequest,
    path: web::Path<String>,
    upload_data: web::Json<UploadRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_context = req.extensions().get::<UserContext>().cloned().unwrap_or(UserContext {
        user_id: "public".to_string(),
        active_team_id: None
    });

    let filename = path.into_inner();
    info!("📤 Upload request: user_id={}, team={:?}, filename={}",
          user_context.user_id, user_context.active_team_id, filename);

    // Check for explicit team_id in the request (overrides active team)
    let team_id = upload_data.team_id.clone().or(user_context.active_team_id.clone());

    // Determine the storage directory
    let storage_dir = if let Some(team_id) = &team_id {
        // Check if user has sufficient permissions for this team
        if !team_storage::user_has_team_role(&user_context.user_id, team_id, TeamRole::Contributor)? {
            error!("❌ User does not have sufficient permissions to upload to team");
            return Err(ServiceError::Forbidden);
        }

        // Ensure team directory exists
        fs_utils::ensure_team_directory(team_id).map_err(|e| {
            error!("❌ Error creating team directory: {:?}", e);
            ServiceError::InternalServerError
        })?;

        format!("./storage/teams/{}", team_id)
    } else {
        // Ensure user directory exists
        fs_utils::ensure_user_directory(&user_context.user_id).map_err(|e| {
            error!("❌ Error creating user directory: {:?}", e);
            ServiceError::InternalServerError
        })?;

        format!("./storage/{}", user_context.user_id)
    };

    let filepath = format!("{}/{}", storage_dir, filename);
    info!("📝 Writing file to: {}", filepath);

    // Save the file content
    match fs::write(&filepath, &upload_data.file_content) {
        Ok(_) => {
            info!("✅ File written successfully");

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

                // Add team info to metadata
                metadata.team_id = team_id.clone();

                // Ensure filename matches
                metadata.file_name = filename.clone();

                // Save metadata to a separate file
                let metadata_path = format!("{}/{}.meta", storage_dir, filename);
                let metadata_json = match serde_json::to_string(&metadata) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("❌ Error serializing metadata: {:?}", e);
                        return Err(ServiceError::InternalServerError);
                    }
                };

                info!("📝 Writing metadata to: {}", metadata_path);

                if let Err(e) = fs::write(&metadata_path, metadata_json) {
                    error!("❌ Error writing metadata: {:?}", e);
                    return Err(ServiceError::InternalServerError);
                }

                info!("✅ Metadata written successfully");
            }

            Ok(HttpResponse::Ok().json(json!({
                "message": format!("File '{}' uploaded successfully!", filename),
                "filename": filename,
                "path": filepath,
                "team_id": team_id
            })))
        },
        Err(e) => {
            error!("❌ Error writing file: {:?}", e);
            Err(ServiceError::InternalServerError)
        }
    }
}

// Get metadata for a file
#[get("/metadata/{filename}")]
async fn get_file_metadata(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    // Get the user_id from the UserContext
    let user_id = if let Some(user_ctx) = req.extensions().get::<UserContext>() {
        user_ctx.user_id.clone()
    } else if let Some(claims) = req.extensions().get::<Claims>() {
        // Fallback to claims
        claims.sub.clone()
    } else {
        "public".to_string()
    };

    info!("📋 Get metadata request: user_id={}, filename={}", user_id, path);

    let filename = path.into_inner();
    let metadata_path = format!("./storage/{}/{}.meta", user_id, filename);
    let file_path = format!("./storage/{}/{}", user_id, filename);

    debug!("🔍 Looking for metadata at: {}", metadata_path);

    // Check if the actual file exists
    if !Path::new(&file_path).exists() {
        debug!("⚠️ File does not exist: {}", file_path);

        // Create an empty default metadata since file doesn't exist yet
        let metadata = FileMetadata {
            file_id: Some(Uuid::new_v4().to_string()),
            file_name: filename.clone(),
            last_modified: Some(Utc::now()),
            team_id: None,
        };

        info!("✨ Creating default metadata for non-existent file");
        return Ok(HttpResponse::Ok().json(metadata));
    }

    // Read metadata file
    match fs::read_to_string(&metadata_path) {
        Ok(content) => {
            info!("✅ Metadata found and read successfully");
            Ok(HttpResponse::Ok().content_type("application/json").body(content))
        },
        Err(e) => {
            debug!("⚠️ Metadata file not found or couldn't be read: {:?}", e);

            // Create default metadata for existing file
            let metadata = FileMetadata {
                file_id: Some(Uuid::new_v4().to_string()),
                file_name: filename.clone(),
                last_modified: Some(Utc::now()),
                team_id: None,
            };

            // Convert to JSON
            let metadata_json = serde_json::to_string(&metadata)
                .map_err(|_| ServiceError::InternalServerError)?;

            info!("✨ Created default metadata for existing file");
            Ok(HttpResponse::Ok().content_type("application/json").body(metadata_json))
        }
    }
}

// Delete a file
#[delete("/files/{filename}")]
async fn delete_file(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    // Get the user_id from the UserContext
    let user_id = if let Some(user_ctx) = req.extensions().get::<UserContext>() {
        user_ctx.user_id.clone()
    } else if let Some(claims) = req.extensions().get::<Claims>() {
        // Fallback to claims
        claims.sub.clone()
    } else {
        "public".to_string()
    };

    let filename = path.into_inner();
    let filepath = format!("./storage/{}/{}", user_id, filename);
    let metadata_path = format!("./storage/{}/{}.meta", user_id, filename);

    info!("🗑️ Delete file request: user_id={}, filename={}", user_id, filename);

    // Delete the file if it exists
    if Path::new(&filepath).exists() {
        fs::remove_file(&filepath).map_err(|e| {
            error!("❌ Error deleting file: {:?}", e);
            ServiceError::InternalServerError
        })?;
        info!("✅ File deleted successfully");
    } else {
        error!("❌ File not found for deletion: {}", filepath);
        return Err(ServiceError::NotFound);
    }

    // Delete metadata if it exists
    if Path::new(&metadata_path).exists() {
        if let Err(e) = fs::remove_file(&metadata_path) {
            debug!("⚠️ Error deleting metadata: {:?}", e);
        } else {
            debug!("✅ Metadata deleted successfully");
        }
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