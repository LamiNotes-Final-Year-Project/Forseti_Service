use crate::models::{Claims, FileMetadata, ServiceError, TeamRole, UploadRequest};
use crate::utils::{fs_utils, team_storage, version_control};
use crate::utils::UserContext; // Import UserContext
use actix_web::{delete, get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use log::{debug, error, info, warn};
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

    // Check if file has an active version control ID
    // For compatibility, we'll first check if there's a file ID that matches the filename
    let versioned_file_id = filename.clone();
    let has_version_control = version_control::version_storage::load_versioned_file_metadata(&versioned_file_id)
        .map(|metadata| !metadata.versions.is_empty())
        .unwrap_or(false);

    if has_version_control {
        info!("✨ File has version control, getting latest version");

        // Load metadata to get current version
        let metadata = version_control::version_storage::load_versioned_file_metadata(&versioned_file_id)?;
        let current_version = metadata.current_version.clone();

        // Get content of current version
        match version_control::version_storage::get_file_version_content(&versioned_file_id, &current_version) {
            Ok(content) => {
                return Ok(HttpResponse::Ok().content_type("text/plain").body(content));
            },
            Err(e) => {
                // If version not found, fall back to regular file storage
                warn!("⚠️ Version not found, falling back to regular storage: {:?}", e);
                // Continue with regular file loading below
            }
        }
    }

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

    // Create a file ID for version control
    let file_id = Uuid::new_v4().to_string();
    let enable_versioning = true; // Default to enabling version control

    // Save the file content
    match fs::write(&filepath, &upload_data.file_content) {
        Ok(_) => {
            info!("✅ File written successfully");

            // Initialize version control if enabled
            if enable_versioning {
                // Initialize a new versioned file
                match version_control::version_storage::initialize_file_versioning(
                    &file_id,
                    &filename,
                    &upload_data.file_content,
                    &user_context.user_id,
                    team_id.clone()
                ) {
                    Ok(metadata) => {
                        info!("✅ Version control initialized for file: {}", filename);

                        // If metadata exists in request, update it with versioning info
                        if let Some(mut metadata_req) = upload_data.metadata.clone() {
                            // Set file_id and current_version in metadata
                            metadata_req.file_id = Some(file_id.clone());
                            metadata_req.current_version = Some(metadata.current_version.clone());
                            metadata_req.versioned = Some(true);

                            // Save enhanced metadata to a separate file
                            let metadata_path = format!("{}/{}.meta", storage_dir, filename);
                            let metadata_json = match serde_json::to_string(&metadata_req) {
                                Ok(json) => json,
                                Err(e) => {
                                    error!("❌ Error serializing metadata: {:?}", e);
                                    return Err(ServiceError::InternalServerError);
                                }
                            };

                            info!("📝 Writing metadata with version info to: {}", metadata_path);

                            if let Err(e) = fs::write(&metadata_path, metadata_json) {
                                error!("❌ Error writing metadata: {:?}", e);
                                return Err(ServiceError::InternalServerError);
                            }

                            info!("✅ Enhanced metadata written successfully");
                        }

                        Ok(HttpResponse::Ok().json(json!({
                            "message": format!("File '{}' uploaded successfully with version control!", filename),
                            "filename": filename,
                            "path": filepath,
                            "team_id": team_id,
                            "file_id": file_id,
                            "current_version": metadata.current_version
                        })))
                    },
                    Err(e) => {
                        error!("❌ Error initializing version control: {:?}", e);
                        // Continue anyway, just without version control

                        // If metadata exists, create or update it
                        if let Some(metadata) = upload_data.metadata.clone() {
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
                            "message": format!("File '{}' uploaded successfully (without version control)!", filename),
                            "filename": filename,
                            "path": filepath,
                            "team_id": team_id
                        })))
                    }
                }
            } else {
                // If metadata exists, create or update it without versioning
                if let Some(metadata) = upload_data.metadata.clone() {
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
            }
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

    let filename = path.into_inner();
    info!("📋 Get metadata request: user_id={}, filename={}", user_id, filename);

    // Check if file has version control
    // For simplicity, we'll use the filename as the file_id for now
    let versioned_file_id = filename.clone();
    let version_metadata = version_control::version_storage::load_versioned_file_metadata(&versioned_file_id);

    if let Ok(v_metadata) = version_metadata {
        if !v_metadata.versions.is_empty() {
            info!("✨ File has version control metadata");

            // Create a regular metadata object with version information
            let metadata = FileMetadata {
                file_id: Some(versioned_file_id),
                file_name: filename.clone(),
                last_modified: Some(v_metadata.last_modified),
                team_id: v_metadata.team_id.clone(),
                current_version: Some(v_metadata.current_version.clone()),
                versioned: Some(true),
            };

            // Return the enhanced metadata
            return Ok(HttpResponse::Ok().json(metadata));
        }
    }

    // If no version control or version control failed, fall back to regular metadata
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
            current_version: None,
            versioned: Some(false),
        };

        info!("✨ Creating default metadata for non-existent file");
        return Ok(HttpResponse::Ok().json(metadata));
    }

    // Read metadata file
    match fs::read_to_string(&metadata_path) {
        Ok(content) => {
            info!("✅ Metadata found and read successfully");

            // Parse the metadata
            match serde_json::from_str::<FileMetadata>(&content) {
                Ok(mut metadata) => {
                    // Ensure file_id exists
                    if metadata.file_id.is_none() {
                        metadata.file_id = Some(Uuid::new_v4().to_string());
                    }
                    Ok(HttpResponse::Ok().json(metadata))
                },
                Err(e) => {
                    error!("❌ Error parsing metadata: {:?}", e);
                    // Return as-is to let client handle it
                    Ok(HttpResponse::Ok().content_type("application/json").body(content))
                }
            }
        },
        Err(e) => {
            debug!("⚠️ Metadata file not found or couldn't be read: {:?}", e);

            // Create default metadata for existing file
            let metadata = FileMetadata {
                file_id: Some(Uuid::new_v4().to_string()),
                file_name: filename.clone(),
                last_modified: Some(Utc::now()),
                team_id: None,
                current_version: None,
                versioned: Some(false),
            };

            info!("✨ Created default metadata for existing file");
            Ok(HttpResponse::Ok().json(metadata))
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