use crate::models::{
    ServiceError, SaveVersionedFileRequest, SaveVersionedFileResponse,
    SaveStatus, ResolveConflictRequest, CreateBranchRequest, MergeBranchRequest,
    StartEditingRequest, ActiveEditorsResponse, VersionHistoryRequest, VersionHistoryResponse,
    FileVersion, ActiveEditor
};
use crate::utils::{
    get_user_id_from_request, get_active_team_from_request, get_username_from_email,
    user_storage, team_storage, fs_utils, version_control
};
use actix_web::{get, post, web, HttpRequest, HttpResponse};
use chrono::Utc;
use log::{debug, error, info, warn};
use serde_json::json;
use uuid::Uuid;
use std::fs;
use std::path::Path;

// Get file version history
#[get("/files/{file_id}/history")]
async fn get_file_history(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<VersionHistoryRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();

    info!("📋 Get file history: file_id={}, user_id={}", file_id, user_id);

    // Get the version history
    let branch = query.branch.as_deref();
    let (mut versions, total_count, current_version) =
        version_control::version_storage::get_file_versions(
            &file_id,
            branch,
            query.limit,
            query.skip
        )?;

    // Add usernames to versions
    for version in &mut versions {
        if let Ok(Some(user)) = user_storage::find_user_by_id(&version.user_id) {
            version.username = Some(get_username_from_email(&user.email));
        }
    }

    let response = VersionHistoryResponse {
        versions,
        total_count,
        current_version,
    };

    Ok(HttpResponse::Ok().json(response))
}

// Get a specific version of a file
#[get("/files/{file_id}/versions/{version_id}")]
async fn get_file_version(
    req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let (file_id, version_id) = path.into_inner();

    info!("📥 Get file version: file_id={}, version_id={}, user_id={}",
          file_id, version_id, user_id);

    // Get the version content
    let content = version_control::version_storage::get_file_version_content(&file_id, &version_id)?;

    Ok(HttpResponse::Ok().content_type("text/plain").body(content))
}

// Compare two versions (diff)
#[get("/files/{file_id}/diff")]
async fn diff_versions(
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<DiffQuery>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();
    let from_version = &query.from;
    let to_version = &query.to;

    info!("📊 Compare versions: file_id={}, from={}, to={}, user_id={}",
          file_id, from_version, to_version, user_id);

    // Get the content of both versions
    let from_content = version_control::version_storage::get_file_version_content(&file_id, from_version)?;
    let to_content = version_control::version_storage::get_file_version_content(&file_id, to_version)?;

    // Use diff util to compare
    let diff_result = version_control::diff_utils::compare_versions(
        &from_content,
        &to_content,
        &to_content // In a straight diff, the third parameter is the same as the second
    );

    Ok(HttpResponse::Ok().json(json!({
        "base_version": from_version,
        "compare_version": to_version,
        "changes": diff_result.changes,
        "conflicts": diff_result.conflicts,
        "can_auto_merge": diff_result.can_auto_merge,
    })))
}

// Register to edit a file
#[post("/files/{file_id}/edit")]
async fn start_editing(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<StartEditingRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();

    info!("🔄 Register as active editor: file_id={}, user_id={}", file_id, user_id);

    // Register as active editor
    let mut active_editors = version_control::version_storage::register_active_editor(
        &file_id,
        &user_id,
        data.branch.clone()
    )?;

    // Add usernames to editors
    for editor in &mut active_editors {
        if let Ok(Some(user)) = user_storage::find_user_by_id(&editor.user_id) {
            editor.username = Some(get_username_from_email(&user.email));
        }
    }

    let response = ActiveEditorsResponse {
        active_editors,
    };

    Ok(HttpResponse::Ok().json(response))
}

// Stop editing a file
#[post("/files/{file_id}/release")]
async fn stop_editing(
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();

    info!("🔄 Unregister as active editor: file_id={}, user_id={}", file_id, user_id);

    // Unregister as active editor
    let mut active_editors = version_control::version_storage::unregister_active_editor(
        &file_id,
        &user_id
    )?;

    // Add usernames to editors
    for editor in &mut active_editors {
        if let Ok(Some(user)) = user_storage::find_user_by_id(&editor.user_id) {
            editor.username = Some(get_username_from_email(&user.email));
        }
    }

    let response = ActiveEditorsResponse {
        active_editors,
    };

    Ok(HttpResponse::Ok().json(response))
}

// Save a file with version control and conflict detection
#[post("/files/{file_id}/save")]
async fn save_with_conflict_detection(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<SaveVersionedFileRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();
    let active_team_id = get_active_team_from_request(&req);

    info!("📤 Save with version control: file_id={}, user_id={}, base_version={}",
          file_id, user_id, data.base_version);

    // Load the versioned file metadata
    let mut metadata = match version_control::version_storage::load_versioned_file_metadata(&file_id) {
        Ok(m) => m,
        Err(e) => {
            error!("Error loading metadata: {:?}", e);
            return Err(ServiceError::InternalServerError);
        }
    };

    // If this is the first save with version control, initialize it
    if metadata.versions.is_empty() {
        // Get the file name from storage
        let file_name = {
            let storage_path = if let Some(team_id) = &active_team_id {
                format!("./storage/teams/{}/{}", team_id, file_id)
            } else {
                format!("./storage/{}/{}", user_id, file_id)
            };

            // Extract file name from path if possible, otherwise use file_id
            Path::new(&storage_path)
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| format!("{}.md", file_id))
        };

        // Initialize versioning
        metadata = match version_control::version_storage::initialize_file_versioning(
            &file_id,
            &file_name,
            &data.content,
            &user_id,
            active_team_id.clone()
        ) {
            Ok(m) => m,
            Err(e) => {
                error!("Error initializing version control: {:?}", e);
                return Err(ServiceError::InternalServerError);
            }
        };

        // Update the response
        let response = SaveVersionedFileResponse {
            status: SaveStatus::Saved,
            new_version: Some(metadata.current_version.clone()),
            conflicts: None,
            message: "File saved with version control enabled".to_string(),
        };

        return Ok(HttpResponse::Ok().json(response));
    }

    // Check for version conflict
    if data.base_version != metadata.current_version && data.base_version != "initial" {
        info!("⚠️ Version conflict detected: base={}, current={}",
            data.base_version, metadata.current_version);

        // Get the base content, current content, and user's new content
        let base_content = match version_control::version_storage::get_file_version_content(
            &file_id,
            &data.base_version
        ) {
            Ok(content) => content,
            Err(e) => {
                error!("Error getting base version content: {:?}", e);
                return Err(ServiceError::InternalServerError);
            }
        };

        let current_content = match version_control::version_storage::get_file_version_content(
            &file_id,
            &metadata.current_version
        ) {
            Ok(content) => content,
            Err(e) => {
                error!("Error getting current version content: {:?}", e);
                return Err(ServiceError::InternalServerError);
            }
        };

        // Try to auto-merge
        if let Some(merged_content) = version_control::diff_utils::attempt_auto_merge(
            &base_content,
            &data.content,
            &current_content
        ) {
            info!("✅ Auto-merged changes successfully");

            // Create a new version with the merged content
            let version_id = Uuid::new_v4().to_string();
            let content_hash = calculate_content_hash(&merged_content);

            // Create version
            let version = FileVersion {
                version_id: version_id.clone(),
                timestamp: Utc::now(),
                user_id: user_id.clone(),
                username: None,
                message: Some("Auto-merged changes".to_string()),
                content_hash,
            };

            // Save version
            if let Err(e) = version_control::version_storage::save_file_version(&file_id, &version_id, &merged_content) {
                error!("Error saving merged version: {:?}", e);
                return Err(ServiceError::InternalServerError);
            }

            // Update metadata
            metadata.versions.insert(version_id.clone(), version);
            metadata.current_version = version_id.clone();
            metadata.last_modified = Utc::now();
            if let Err(e) = version_control::version_storage::save_versioned_file_metadata(&metadata) {
                error!("Error saving metadata: {:?}", e);
                return Err(ServiceError::InternalServerError);
            }

            // Update response
            let response = SaveVersionedFileResponse {
                status: SaveStatus::AutoMerged,
                new_version: Some(version_id),
                conflicts: None,
                message: "Changes were automatically merged".to_string(),
            };

            return Ok(HttpResponse::Ok().json(response));
        } else {
            // Generate conflicts
            let diff = version_control::diff_utils::compare_versions(
                &base_content,
                &data.content,
                &current_content
            );

            // Return conflict information
            let response = SaveVersionedFileResponse {
                status: SaveStatus::Conflict,
                new_version: Some(metadata.current_version.clone()),
                conflicts: Some(diff.conflicts),
                message: "Conflict detected. Please resolve manually.".to_string(),
            };

            return Ok(HttpResponse::Conflict().json(response));
        }
    }

    // No conflict, save the new version
    let version_id = Uuid::new_v4().to_string();
    let content_hash = calculate_content_hash(&data.content);

    // Create version
    let version = FileVersion {
        version_id: version_id.clone(),
        timestamp: Utc::now(),
        user_id: user_id.clone(),
        username: None,
        message: data.message.clone(),
        content_hash,
    };

    // Save version
    if let Err(e) = version_control::version_storage::save_file_version(&file_id, &version_id, &data.content) {
        error!("Error saving version: {:?}", e);
        return Err(ServiceError::InternalServerError);
    }

    // Update metadata
    metadata.versions.insert(version_id.clone(), version);
    metadata.current_version = version_id.clone();
    metadata.last_modified = Utc::now();
    if let Err(e) = version_control::version_storage::save_versioned_file_metadata(&metadata) {
        error!("Error saving metadata: {:?}", e);
        return Err(ServiceError::InternalServerError);
    }

    // Also save to regular storage for backward compatibility
    let storage_path = if let Some(team_id) = active_team_id {
        let team_dir = format!("./storage/teams/{}", team_id);
        if let Err(e) = fs_utils::ensure_team_directory(&team_id) {
            error!("Error ensuring team directory: {:?}", e);
            return Err(ServiceError::InternalServerError);
        }
        format!("{}/{}", team_dir, metadata.file_name)
    } else {
        let user_dir = format!("./storage/{}", user_id);
        if let Err(e) = fs_utils::ensure_user_directory(&user_id) {
            error!("Error ensuring user directory: {:?}", e);
            return Err(ServiceError::InternalServerError);
        }
        format!("{}/{}", user_dir, metadata.file_name)
    };

    // Write the file
    if let Err(e) = fs::write(&storage_path, &data.content) {
        error!("❌ Error writing file to regular storage: {:?}", e);
        return Err(ServiceError::InternalServerError);
    }

    // Return success response
    let response = SaveVersionedFileResponse {
        status: SaveStatus::Saved,
        new_version: Some(version_id),
        conflicts: None,
        message: "File saved successfully".to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

// Resolve conflicts
#[post("/files/{file_id}/resolve-conflicts")]
async fn resolve_conflicts(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<ResolveConflictRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();

    info!("🔄 Resolve conflicts: file_id={}, user_id={}", file_id, user_id);

    // Extract resolved content
    let resolved_content = version_control::extract_resolved_content(&data.content);

    // Create a new version with the resolved content
    let version_id = Uuid::new_v4().to_string();
    let content_hash = calculate_content_hash(&resolved_content);

    // Load metadata
    let mut metadata = version_control::version_storage::load_versioned_file_metadata(&file_id)?;

    // Create version
    let version = FileVersion {
        version_id: version_id.clone(),
        timestamp: Utc::now(),
        user_id: user_id.clone(),
        username: None,
        message: Some(data.message.clone()),
        content_hash,
    };

    // Save version
    version_control::version_storage::save_file_version(&file_id, &version_id, &resolved_content)?;

    // Update metadata
    metadata.versions.insert(version_id.clone(), version);
    metadata.current_version = version_id.clone();
    metadata.last_modified = Utc::now();
    version_control::version_storage::save_versioned_file_metadata(&metadata)?;

    // Save to regular storage for backward compatibility
    let active_team_id = get_active_team_from_request(&req);
    let storage_path = if let Some(team_id) = active_team_id {
        let team_dir = format!("./storage/teams/{}", team_id);
        fs_utils::ensure_team_directory(&team_id).map_err(|e| {
            error!("Error ensuring team directory: {:?}", e);
            ServiceError::InternalServerError
        })?;
        format!("{}/{}", team_dir, metadata.file_name)
    } else {
        let user_dir = format!("./storage/{}", user_id);
        fs_utils::ensure_user_directory(&user_id).map_err(|e| {
            error!("❌ Error ensuring user directory: {:?}", e);
            ServiceError::InternalServerError
        })?;
        format!("{}/{}", user_dir, metadata.file_name)
    };

    // Write the file
    fs::write(&storage_path, &resolved_content).map_err(|e| {
        error!("❌ Error writing file to regular storage: {:?}", e);
        ServiceError::InternalServerError
    })?;

    // Return success response
    let response = SaveVersionedFileResponse {
        status: SaveStatus::Saved,
        new_version: Some(version_id),
        conflicts: None,
        message: "Conflicts resolved successfully".to_string(),
    };

    Ok(HttpResponse::Ok().json(response))
}

// Create a branch
#[post("/files/{file_id}/branches")]
async fn create_branch(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<CreateBranchRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();

    info!("🔄 Create branch: file_id={}, user_id={}, branch_name={}",
          file_id, user_id, data.name);

    // Create the branch
    let branch = version_control::version_storage::create_branch(
        &file_id,
        &data.name,
        &data.base_version,
        &user_id,
        data.content.as_deref()
    )?;

    Ok(HttpResponse::Ok().json(branch))
}

// Merge branches
#[post("/files/{file_id}/merge")]
async fn merge_branches(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<MergeBranchRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();
    let source_branch = &data.source_branch;
    let target_branch = &data.target_branch;
    let message = data.message.clone().unwrap_or_else(||
        format!("Merged branch {} into {}", source_branch, target_branch)
    );

    info!("🔄 Merge branches: file_id={}, user_id={}, source={}, target={}",
          file_id, user_id, source_branch, target_branch);

    // Load metadata
    let metadata = version_control::version_storage::load_versioned_file_metadata(&file_id)?;

    // Get the source branch
    let source = metadata.branches.get(source_branch)
        .ok_or_else(|| ServiceError::BadRequest(format!("Source branch {} not found", source_branch)))?;

    // Get the target branch (or use main if not specified)
    let target_version = if target_branch == "main" || target_branch == "master" {
        metadata.current_version.clone()
    } else {
        let target = metadata.branches.get(target_branch)
            .ok_or_else(|| ServiceError::BadRequest(format!("Target branch {} not found", target_branch)))?;
        target.head_version.clone()
    };

    // Get the content of both branches
    let source_content = version_control::version_storage::get_file_version_content(
        &file_id,
        &source.head_version
    )?;

    let target_content = version_control::version_storage::get_file_version_content(
        &file_id,
        &target_version
    )?;

    // Get their common ancestor (base)
    let base_version = source.base_version.clone();
    let base_content = version_control::version_storage::get_file_version_content(
        &file_id,
        &base_version
    )?;

    // Try to auto-merge
    if let Some(merged_content) = version_control::diff_utils::attempt_auto_merge(
        &base_content,
        &source_content,
        &target_content
    ) {
        info!("✅ Auto-merged branches successfully");

        // Create a new version with the merged content
        let version_id = Uuid::new_v4().to_string();
        let content_hash = calculate_content_hash(&merged_content);

        // Create version
        let version = FileVersion {
            version_id: version_id.clone(),
            timestamp: Utc::now(),
            user_id: user_id.clone(),
            username: None,
            message: Some(message),
            content_hash,
        };

        // Save version
        version_control::version_storage::save_file_version(&file_id, &version_id, &merged_content)?;

        // Update metadata
        let mut updated_metadata = metadata.clone();
        updated_metadata.versions.insert(version_id.clone(), version);

        // Update the current version if merging to main
        if target_branch == "main" || target_branch == "master" {
            updated_metadata.current_version = version_id.clone();
        } else {
            // Update the target branch head
            if let Some(target) = updated_metadata.branches.get_mut(target_branch) {
                target.head_version = version_id.clone();
            }
        }

        updated_metadata.last_modified = Utc::now();
        version_control::version_storage::save_versioned_file_metadata(&updated_metadata)?;

        // Return success
        Ok(HttpResponse::Ok().json(json!({
            "status": "merged",
            "new_version": version_id,
            "message": "Branches merged successfully"
        })))
    } else {
        // Generate conflicts
        let diff = version_control::diff_utils::compare_versions(
            &base_content,
            &source_content,
            &target_content
        );

        // Create a merged content with conflict markers
        let marked_content = version_control::diff_utils::create_marked_merge(
            &base_content,
            &source_content,
            &target_content
        );

        // Return conflict information
        Ok(HttpResponse::Conflict().json(json!({
            "status": "conflict",
            "conflicts": diff.conflicts,
            "marked_content": marked_content,
            "message": "Merge conflicts detected. Please resolve manually."
        })))
    }
}

// Get active editors for a file
#[get("/files/{file_id}/active-editors")]
async fn get_active_editors(
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let file_id = path.into_inner();

    info!("📋 Get active editors: file_id={}, user_id={}", file_id, user_id);

    // Load the versioned file metadata
    let metadata = version_control::version_storage::load_versioned_file_metadata(&file_id)?;

    // Add usernames to active editors
    let mut active_editors = Vec::new();
    for editor in &metadata.active_editors {
        let mut editor_with_username = editor.clone();
        if let Ok(Some(user)) = user_storage::find_user_by_id(&editor.user_id) {
            editor_with_username.username = Some(get_username_from_email(&user.email));
        }
        active_editors.push(editor_with_username);
    }

    let response = ActiveEditorsResponse {
        active_editors,
    };

    Ok(HttpResponse::Ok().json(response))
}

// Query parameters for diff
#[derive(serde::Deserialize)]
pub struct DiffQuery {
    pub from: String,
    pub to: String,
}

// Helper to calculate content hash
fn calculate_content_hash(content: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// Register all version control routes
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(get_file_history)
        .service(get_file_version)
        .service(diff_versions)
        .service(start_editing)
        .service(stop_editing)
        .service(save_with_conflict_detection)
        .service(resolve_conflicts)
        .service(create_branch)
        .service(merge_branches)
        .service(get_active_editors);
}