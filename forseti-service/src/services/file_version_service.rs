// src/services/file_version_service.rs

use crate::models::{FileVersion, FileMetadata, ConflictData, ConflictRegion, ResolutionData};
use crate::utils::{fs_utils, user_storage};
use crate::errors::ServiceError;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::fs;
use std::path::Path;
use log::{debug, error, info};
use serde::{Serialize, Deserialize};
use diff::Diff;

// Define our storage directories
const VERSIONS_DIR: &str = "./storage/versions";
const ACCESS_TIMES_DIR: &str = "./storage/access_times";

// Enum to represent conflict status
#[derive(Debug, Serialize, Deserialize)]
pub enum ConflictStatus {
    NoConflict,
    AutoResolvable,
    NeedsResolution(ConflictData),
}

// Initialize directories
pub fn init_storage() -> std::io::Result<()> {
    let versions_dir = Path::new(VERSIONS_DIR);
    let access_times_dir = Path::new(ACCESS_TIMES_DIR);

    if !versions_dir.exists() {
        info!("Creating versions directory");
        fs::create_dir_all(versions_dir)?;
    }

    if !access_times_dir.exists() {
        info!("Creating access times directory");
        fs::create_dir_all(access_times_dir)?;
    }

    Ok(())
}

// Create a new file version
pub fn create_file_version(
    file_id: &str,
    filename: &str,
    content: &str,
    user_id: &str,
    parent_version_id: Option<String>,
    team_id: Option<&str>
) -> Result<FileVersion, ServiceError> {
    // Ensure storage is initialized
    init_storage().map_err(|e| {
        error!("Failed to initialize version storage: {:?}", e);
        ServiceError::InternalServerError
    })?;

    // Create version ID and metadata
    let version_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    // Create file version object
    let version = FileVersion {
        id: version_id.clone(),
        file_id: file_id.to_string(),
        content: content.to_string(),
        created_by: user_id.to_string(),
        created_at: now,
        parent_version_id,
        filename: filename.to_string(),
        team_id: team_id.map(|t| t.to_string()),
    };

    // Create path to save version
    let path = match team_id {
        Some(team) => format!("{}/team_{}/{}", VERSIONS_DIR, team, filename),
        None => format!("{}/user_{}/{}", VERSIONS_DIR, user_id, filename),
    };

    // Create directory if it doesn't exist
    let dir_path = Path::new(&path).parent().ok_or_else(|| {
        error!("Failed to get parent directory from path: {}", path);
        ServiceError::InternalServerError
    })?;

    if !dir_path.exists() {
        fs::create_dir_all(dir_path).map_err(|e| {
            error!("Failed to create version directory: {:?}", e);
            ServiceError::InternalServerError
        })?;
    }

    // Save version metadata and content
    let version_meta_path = format!("{}/{}.meta.json", path, version_id);
    let version_content_path = format!("{}/{}.content", path, version_id);

    let version_meta = serde_json::to_string(&version).map_err(|e| {
        error!("Failed to serialize version metadata: {:?}", e);
        ServiceError::InternalServerError
    })?;

    fs::write(&version_meta_path, version_meta).map_err(|e| {
        error!("Failed to write version metadata: {:?}", e);
        ServiceError::InternalServerError
    })?;

    fs::write(&version_content_path, content).map_err(|e| {
        error!("Failed to write version content: {:?}", e);
        ServiceError::InternalServerError
    })?;

    debug!("Saved file version: {} at {}", version_id, version_meta_path);

    Ok(version)
}

// Get the current version of a file
pub fn get_current_version(
    filename: &str,
    user_id: &str,
    team_id: Option<&str>
) -> Result<FileVersion, ServiceError> {
    let versions = list_file_versions(filename, user_id, team_id)?;

    if versions.is_empty() {
        return Err(ServiceError::NotFound);
    }

    // Return the most recent version
    Ok(versions[0].clone())
}

// List versions of a file
pub fn list_file_versions(
    filename: &str,
    user_id: &str,
    team_id: Option<&str>
) -> Result<Vec<FileVersion>, ServiceError> {
    // Get path based on team context
    let path = match team_id {
        Some(team) => format!("{}/team_{}/{}", VERSIONS_DIR, team, filename),
        None => format!("{}/user_{}/{}", VERSIONS_DIR, user_id, filename),
    };

    let dir_path = Path::new(&path);
    if !dir_path.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();

    for entry in fs::read_dir(dir_path).map_err(|e| {
        error!("Failed to read versions directory: {:?}", e);
        ServiceError::InternalServerError
    })? {
        let entry = entry.map_err(|e| {
            error!("Failed to read directory entry: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let entry_path = entry.path();

        // Only process meta files
        if entry_path.is_file() && entry_path.to_string_lossy().ends_with(".meta.json") {
            let content = fs::read_to_string(&entry_path).map_err(|e| {
                error!("Failed to read version metadata: {:?}", e);
                ServiceError::InternalServerError
            })?;

            let version: FileVersion = serde_json::from_str(&content).map_err(|e| {
                error!("Failed to parse version metadata: {:?}", e);
                ServiceError::InternalServerError
            })?;

            versions.push(version);
        }
    }

    // Sort by creation time, newest first
    versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(versions)
}

// Get a specific version
pub fn get_version_by_id(
    version_id: &str,
    filename: &str,
    user_id: &str,
    team_id: Option<&str>
) -> Result<FileVersion, ServiceError> {
    // Get path based on team context
    let path = match team_id {
        Some(team) => format!("{}/team_{}/{}/{}.meta.json", VERSIONS_DIR, team, filename, version_id),
        None => format!("{}/user_{}/{}/{}.meta.json", VERSIONS_DIR, user_id, filename, version_id),
    };

    if !Path::new(&path).exists() {
        return Err(ServiceError::NotFound);
    }

    let content = fs::read_to_string(path).map_err(|e| {
        error!("Failed to read version metadata: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let version: FileVersion = serde_json::from_str(&content).map_err(|e| {
        error!("Failed to parse version metadata: {:?}", e);
        ServiceError::InternalServerError
    })?;

    Ok(version)
}

// Get version content
pub fn get_version_content(
    version_id: &str,
    filename: &str,
    user_id: &str,
    team_id: Option<&str>
) -> Result<String, ServiceError> {
    // Get path based on team context
    let path = match team_id {
        Some(team) => format!("{}/team_{}/{}/{}.content", VERSIONS_DIR, team, filename, version_id),
        None => format!("{}/user_{}/{}/{}.content", VERSIONS_DIR, user_id, filename, version_id),
    };

    if !Path::new(&path).exists() {
        return Err(ServiceError::NotFound);
    }

    let content = fs::read_to_string(path).map_err(|e| {
        error!("Failed to read version content: {:?}", e);
        ServiceError::InternalServerError
    })?;

    Ok(content)
}

// Track user access to a file
pub fn update_user_access(
    user_id: &str,
    filename: &str,
    version_id: &str
) -> Result<(), ServiceError> {
    // Ensure storage is initialized
    init_storage().map_err(|e| {
        error!("Failed to initialize access storage: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let access_path = format!("{}/{}_{}.json", ACCESS_TIMES_DIR, user_id, filename.replace('/', "_"));

    let access_data = json!({
        "user_id": user_id,
        "file_id": filename,
        "last_access": Utc::now().to_rfc3339(),
        "last_version_seen": version_id
    });

    let access_json = serde_json::to_string(&access_data).map_err(|e| {
        error!("Failed to serialize access data: {:?}", e);
        ServiceError::InternalServerError
    })?;

    fs::write(&access_path, access_json).map_err(|e| {
        error!("Failed to write access data: {:?}", e);
        ServiceError::InternalServerError
    })?;

    debug!("Updated access for user {} and file {}", user_id, filename);
    Ok(())
}

// Get user access information
pub fn get_user_access(
    user_id: &str,
    filename: &str
) -> Result<Option<UserFileAccess>, ServiceError> {
    let access_path = format!("{}/{}_{}.json", ACCESS_TIMES_DIR, user_id, filename.replace('/', "_"));

    if !Path::new(&access_path).exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&access_path).map_err(|e| {
        error!("Failed to read access data: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let access: UserFileAccess = serde_json::from_str(&content).map_err(|e| {
        error!("Failed to parse access data: {:?}", e);
        ServiceError::InternalServerError
    })?;

    Ok(Some(access))
}

// Find common ancestor version
pub fn find_common_ancestor(
    version1_id: &str,
    version2_id: &str,
    filename: &str,
    user_id: &str,
    team_id: Option<&str>
) -> Result<Option<FileVersion>, ServiceError> {
    if version1_id == version2_id {
        return get_version_by_id(version1_id, filename, user_id, team_id).map(Some);
    }

    // Simple implementation - go back in history of both versions until a common ancestor is found
    let mut v1_history = get_version_history(version1_id, filename, user_id, team_id)?;
    let v2_history = get_version_history(version2_id, filename, user_id, team_id)?;

    for v1 in &v1_history {
        for v2 in &v2_history {
            if v1.id == v2.id {
                return Ok(Some(v1.clone()));
            }
        }
    }

    Ok(None)
}

// Get version history (all ancestors)
fn get_version_history(
    start_version_id: &str,
    filename: &str,
    user_id: &str,
    team_id: Option<&str>
) -> Result<Vec<FileVersion>, ServiceError> {
    let mut history = Vec::new();
    let mut current_id = Some(start_version_id.to_string());

    while let Some(version_id) = current_id {
        match get_version_by_id(&version_id, filename, user_id, team_id) {
            Ok(version) => {
                current_id = version.parent_version_id.clone();
                history.push(version);
            },
            Err(_) => {
                current_id = None;
            }
        }
    }

    Ok(history)
}

// Find conflicting regions in file content
pub fn find_conflicting_regions(
    base: &str,
    local: &str,
    remote: &str
) -> Vec<ConflictRegion> {
    let mut regions = Vec::new();

    // Use the diff library to find changes
    let base_local_diff = diff::lines(base, local);
    let base_remote_diff = diff::lines(base, remote);

    // Build maps of line numbers that changed
    let mut local_changes = std::collections::HashSet::new();
    let mut remote_changes = std::collections::HashSet::new();

    let mut line_num = 0;
    for change in &base_local_diff {
        match change {
            diff::Result::Left(_) => {
                // Line removed in local
                local_changes.insert(line_num);
                line_num += 1;
            },
            diff::Result::Both(_, _) => {
                // Line unchanged
                line_num += 1;
            },
            diff::Result::Right(_) => {
                // Line added in local
                local_changes.insert(line_num);
            }
        }
    }

    line_num = 0;
    for change in &base_remote_diff {
        match change {
            diff::Result::Left(_) => {
                // Line removed in remote
                remote_changes.insert(line_num);
                line_num += 1;
            },
            diff::Result::Both(_, _) => {
                // Line unchanged
                line_num += 1;
            },
            diff::Result::Right(_) => {
                // Line added in remote
                remote_changes.insert(line_num);
            }
        }
    }

    // Find overlapping changes
    let mut conflict_lines: Vec<usize> = local_changes.intersection(&remote_changes)
        .cloned()
        .collect();

    conflict_lines.sort();

    // Group consecutive line numbers into regions
    if !conflict_lines.is_empty() {
        let mut start = conflict_lines[0];
        let mut end = start;

        for &line in &conflict_lines[1..] {
            if line == end + 1 {
                end = line;
            } else {
                // Found a gap, create a region for the current range
                regions.push(ConflictRegion {
                    start_line: start,
                    end_line: end,
                    local_content: extract_lines(local, start, end),
                    remote_content: extract_lines(remote, start, end),
                    base_content: Some(extract_lines(base, start, end)),
                });

                // Start a new range
                start = line;
                end = line;
            }
        }

        // Add the last region
        regions.push(ConflictRegion {
            start_line: start,
            end_line: end,
            local_content: extract_lines(local, start, end),
            remote_content: extract_lines(remote, start, end),
            base_content: Some(extract_lines(base, start, end)),
        });
    }

    regions
}

// Helper to extract lines from content
fn extract_lines(content: &str, start: usize, end: usize) -> String {
    content.lines()
        .skip(start)
        .take(end - start + 1)
        .collect::<Vec<&str>>()
        .join("\n")
}

// Detect conflicts when saving a file
pub fn detect_conflicts(
    user_id: &str,
    filename: &str,
    new_content: &str,
    team_id: Option<&str>
) -> Result<ConflictStatus, ServiceError> {
    // Get the user's access record
    let user_access = match get_user_access(user_id, filename)? {
        Some(access) => access,
        None => {
            // First time accessing, no conflicts
            return Ok(ConflictStatus::NoConflict);
        }
    };

    // Get current version
    let current_version = match get_current_version(filename, user_id, team_id) {
        Ok(version) => version,
        Err(ServiceError::NotFound) => {
            // File doesn't exist yet
            return Ok(ConflictStatus::NoConflict);
        },
        Err(e) => return Err(e),
    };

    // If user has seen the latest version, no conflict
    if user_access.last_version_seen == current_version.id {
        return Ok(ConflictStatus::NoConflict);
    }

    // If user created the current version, no conflict
    if current_version.created_by == user_id {
        return Ok(ConflictStatus::NoConflict);
    }

    // Get the version the user last saw
    let user_version = get_version_by_id(
        &user_access.last_version_seen,
        filename,
        user_id,
        team_id
    )?;

    // Get the content of these versions
    let user_version_content = get_version_content(
        &user_version.id,
        filename,
        user_id,
        team_id
    )?;

    let current_version_content = get_version_content(
        &current_version.id,
        filename,
        user_id,
        team_id
    )?;

    // Find common ancestor
    let base_version = find_common_ancestor(
        &user_version.id,
        &current_version.id,
        filename,
        user_id,
        team_id
    )?;

    let base_content = match base_version {
        Some(version) => get_version_content(&version.id, filename, user_id, team_id)?,
        None => "".to_string(), // No common ancestor, use empty string
    };

    // Find conflicting regions
    let conflict_regions = find_conflicting_regions(
        &base_content,
        &user_version_content,
        &current_version_content
    );

    if conflict_regions.is_empty() {
        return Ok(ConflictStatus::AutoResolvable);
    }

    // Get info about the user who made the remote changes
    let remote_author_name = match user_storage::find_user_by_id(&current_version.created_by)? {
        Some(user) => user.email,
        None => current_version.created_by.clone(),
    };

    // Create conflict data
    let conflict_data = ConflictData {
        file_id: filename.to_string(),
        file_name: filename.to_string(),
        local_version: new_content.to_string(),
        remote_version: current_version_content,
        base_version: Some(base_content),
        last_local_update: user_version.created_at.to_rfc3339(),
        last_remote_update: current_version.created_at.to_rfc3339(),
        remote_author: remote_author_name,
        conflict_regions,
    };

    Ok(ConflictStatus::NeedsResolution(conflict_data))
}

// Save a resolved conflict
pub fn save_resolved_conflict(
    user_id: &str,
    filename: &str,
    resolved_content: &str,
    resolution_data: &ResolutionData,
    team_id: Option<&str>
) -> Result<FileVersion, ServiceError> {
    // Get current version to use as parent
    let current_version = match get_current_version(filename, user_id, team_id) {
        Ok(version) => version.id,
        Err(_) => "".to_string(),
    };

    // Create a new version with the resolved content
    let new_version = create_file_version(
        filename,
        filename,
        resolved_content,
        user_id,
        Some(current_version),
        team_id,
    )?;

    // Update the user's access record
    update_user_access(user_id, filename, &new_version.id)?;

    // Record the resolution
    record_conflict_resolution(
        filename,
        user_id,
        resolution_data,
        team_id,
    )?;

    Ok(new_version)
}

// Record conflict resolution data
pub fn record_conflict_resolution(
    filename: &str,
    user_id: &str,
    resolution_data: &ResolutionData,
    team_id: Option<&str>
) -> Result<(), ServiceError> {
    let resolutions_dir = "./storage/resolutions";
    let dir_path = Path::new(resolutions_dir);

    if !dir_path.exists() {
        fs::create_dir_all(dir_path).map_err(|e| {
            error!("Failed to create resolutions directory: {:?}", e);
            ServiceError::InternalServerError
        })?;
    }

    let resolution_id = Uuid::new_v4().to_string();
    let now = Utc::now();

    let resolution_path = match team_id {
        Some(team) => format!("{}/team_{}_{}_{}_{}.json", resolutions_dir, team, filename.replace('/', "_"), user_id, resolution_id),
        None => format!("{}/user_{}_{}_{}.json", resolutions_dir, user_id, filename.replace('/', "_"), resolution_id),
    };

    let resolution_data = json!({
        "id": resolution_id,
        "file_id": filename,
        "resolved_by": user_id,
        "resolved_at": now.to_rfc3339(),
        "resolution_type": resolution_data.resolution_type,
        "conflict_sections": resolution_data.sections,
        "team_id": team_id,
    });

    let resolution_json = serde_json::to_string(&resolution_data).map_err(|e| {
        error!("Failed to serialize resolution data: {:?}", e);
        ServiceError::InternalServerError
    })?;

    fs::write(&resolution_path, resolution_json).map_err(|e| {
        error!("Failed to write resolution data: {:?}", e);
        ServiceError::InternalServerError
    })?;

    debug!("Recorded conflict resolution: {}", resolution_path);
    Ok(())
}