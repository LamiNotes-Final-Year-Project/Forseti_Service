use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileVersion {
    pub version_id: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub username: Option<String>,
    pub message: Option<String>,
    pub content_hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileBranch {
    pub branch_id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub base_version: String,
    pub head_version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActiveEditor {
    pub user_id: String,
    pub username: Option<String>,
    pub editing_since: DateTime<Utc>,
    pub branch: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionedFileMetadata {
    pub file_id: String,
    pub file_name: String,
    pub current_version: String,  // ID of the current version in main branch
    pub versions: HashMap<String, FileVersion>,  // Map of version_id -> FileVersion
    pub branches: HashMap<String, FileBranch>,  // Map of branch_id -> FileBranch
    pub active_editors: Vec<ActiveEditor>,
    pub last_modified: DateTime<Utc>,
    pub team_id: Option<String>,
    pub owner_id: String,
}

// Request for saving a file with version control
#[derive(Serialize, Deserialize, Debug)]
pub struct SaveVersionedFileRequest {
    pub content: String,
    pub base_version: String,
    pub message: Option<String>,
    pub branch: Option<String>,
}

// Response for save operations with potential conflicts
#[derive(Serialize, Deserialize, Debug)]
pub struct SaveVersionedFileResponse {
    pub status: SaveStatus,
    pub new_version: Option<String>,
    pub conflicts: Option<Vec<Conflict>>,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum SaveStatus {
    #[serde(rename = "saved")]
    Saved,
    #[serde(rename = "conflict")]
    Conflict,
    #[serde(rename = "auto_merged")]
    AutoMerged,
}

// Request for resolving conflicts
#[derive(Serialize, Deserialize, Debug)]
pub struct ResolveConflictRequest {
    pub content: String,
    pub base_version: String,
    pub current_version: String,
    pub message: String,
}

// Represents a text change
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TextChange {
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

// Represents a conflict between versions
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Conflict {
    pub start_line: usize,
    pub end_line: usize,
    pub base_content: String,
    pub current_content: String,
    pub your_content: String,
}

// Request for creating a new branch
#[derive(Serialize, Deserialize, Debug)]
pub struct CreateBranchRequest {
    pub name: String,
    pub base_version: String,
    pub content: Option<String>,  // Optional new content for branch
}

// Request to merge branches
#[derive(Serialize, Deserialize, Debug)]
pub struct MergeBranchRequest {
    pub source_branch: String,
    pub target_branch: String,
    pub message: Option<String>,
}

// Response from diff operation
#[derive(Serialize, Deserialize, Debug)]
pub struct DiffResponse {
    pub base_version: String,
    pub compare_version: String,
    pub changes: Vec<TextChange>,
    pub conflicts: Vec<Conflict>,
    pub can_auto_merge: bool,
}

// Request to start editing a file
#[derive(Serialize, Deserialize, Debug)]
pub struct StartEditingRequest {
    pub branch: Option<String>,
}

// Response for active editors
#[derive(Serialize, Deserialize, Debug)]
pub struct ActiveEditorsResponse {
    pub active_editors: Vec<ActiveEditor>,
}

// Request for file version history
#[derive(Serialize, Deserialize, Debug)]
pub struct VersionHistoryRequest {
    pub branch: Option<String>,
    pub limit: Option<usize>,
    pub skip: Option<usize>,
}

// Response for file version history
#[derive(Serialize, Deserialize, Debug)]
pub struct VersionHistoryResponse {
    pub versions: Vec<FileVersion>,
    pub total_count: usize,
    pub current_version: String,
}