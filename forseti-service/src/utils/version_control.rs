use crate::models::{
    ServiceError, FileVersion, VersionedFileMetadata, FileBranch,
    ActiveEditor, Conflict, DiffResponse, SaveStatus, TextChange
};
use chrono::Utc;
use log::{debug, error, info, warn};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use similar::{ChangeTag, TextDiff};
use regex::Regex;

const VERSION_STORAGE_PATH: &str = "./storage/versions";
const BRANCH_STORAGE_PATH: &str = "./storage/branches";

pub mod version_storage {
    use super::*;

    // Ensures the version storage structure exists
    pub fn ensure_version_storage() -> io::Result<()> {
        // Main version storage
        let version_path = Path::new(VERSION_STORAGE_PATH);
        if !version_path.exists() {
            info!("Creating version storage directory: {}", VERSION_STORAGE_PATH);
            fs::create_dir_all(version_path)?;
        }

        // Branch storage
        let branch_path = Path::new(BRANCH_STORAGE_PATH);
        if !branch_path.exists() {
            info!("Creating branch storage directory: {}", BRANCH_STORAGE_PATH);
            fs::create_dir_all(branch_path)?;
        }

        Ok(())
    }

    // Gets the file path for a specific version
    pub fn get_version_path(file_id: &str, version_id: &str) -> PathBuf {
        let file_dir = format!("{}/{}", VERSION_STORAGE_PATH, file_id);
        Path::new(&file_dir).join(format!("{}.content", version_id))
    }

    // Gets the metadata path for a versioned file
    pub fn get_version_metadata_path(file_id: &str) -> PathBuf {
        let file_dir = format!("{}/{}", VERSION_STORAGE_PATH, file_id);
        Path::new(&file_dir).join("metadata.json")
    }

    // Ensures the file version directory exists
    pub fn ensure_file_version_dir(file_id: &str) -> io::Result<()> {
        let file_dir = format!("{}/{}", VERSION_STORAGE_PATH, file_id);
        let dir_path = Path::new(&file_dir);
        if !dir_path.exists() {
            info!("Creating version directory for file: {}", file_id);
            fs::create_dir_all(dir_path)?;
        }
        Ok(())
    }

    // Saves a specific version of a file
    pub fn save_file_version(
        file_id: &str,
        version_id: &str,
        content: &str,
    ) -> Result<(), ServiceError> {
        ensure_file_version_dir(file_id).map_err(|e| {
            error!("Failed to create version directory: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let version_path = get_version_path(file_id, version_id);
        debug!("Saving version {} to path: {:?}", version_id, version_path);

        fs::write(&version_path, content).map_err(|e| {
            error!("Failed to write version file: {:?}", e);
            ServiceError::InternalServerError
        })?;

        Ok(())
    }

    // Gets the content of a specific version
    pub fn get_file_version_content(
        file_id: &str,
        version_id: &str,
    ) -> Result<String, ServiceError> {
        let version_path = get_version_path(file_id, version_id);
        debug!("Reading version {} from path: {:?}", version_id, version_path);

        if !version_path.exists() {
            error!("Version file not found: {:?}", version_path);
            return Err(ServiceError::NotFound);
        }

        fs::read_to_string(&version_path).map_err(|e| {
            error!("Failed to read version file: {:?}", e);
            ServiceError::InternalServerError
        })
    }

    // Loads the versioned file metadata
    pub fn load_versioned_file_metadata(file_id: &str) -> Result<VersionedFileMetadata, ServiceError> {
        let metadata_path = get_version_metadata_path(file_id);
        debug!("Loading versioned metadata from: {:?}", metadata_path);

        if !metadata_path.exists() {
            warn!("No versioned metadata found for file: {}", file_id);
            // Return a new empty metadata structure
            return Ok(VersionedFileMetadata {
                file_id: file_id.to_string(),
                file_name: "unknown.md".to_string(), // This will be updated when saving
                current_version: "initial".to_string(),
                versions: HashMap::new(),
                branches: HashMap::new(),
                active_editors: Vec::new(),
                last_modified: Utc::now(),
                team_id: None,
                owner_id: "unknown".to_string(), // This will be updated when saving
            });
        }

        let metadata_str = fs::read_to_string(&metadata_path).map_err(|e| {
            error!("Failed to read versioned metadata: {:?}", e);
            ServiceError::InternalServerError
        })?;

        serde_json::from_str::<VersionedFileMetadata>(&metadata_str).map_err(|e| {
            error!("Failed to parse versioned metadata: {:?}", e);
            ServiceError::InternalServerError
        })
    }

    // Saves the versioned file metadata
    pub fn save_versioned_file_metadata(metadata: &VersionedFileMetadata) -> Result<(), ServiceError> {
        ensure_file_version_dir(&metadata.file_id).map_err(|e| {
            error!("Failed to create version directory: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let metadata_path = get_version_metadata_path(&metadata.file_id);
        debug!("Saving versioned metadata to: {:?}", metadata_path);

        let metadata_str = serde_json::to_string_pretty(metadata).map_err(|e| {
            error!("Failed to serialize versioned metadata: {:?}", e);
            ServiceError::InternalServerError
        })?;

        fs::write(&metadata_path, metadata_str).map_err(|e| {
            error!("Failed to write versioned metadata: {:?}", e);
            ServiceError::InternalServerError
        })?;

        Ok(())
    }

    // Create an initial version for a file
    pub fn initialize_file_versioning(
        file_id: &str,
        file_name: &str,
        content: &str,
        owner_id: &str,
        team_id: Option<String>,
    ) -> Result<VersionedFileMetadata, ServiceError> {
        ensure_file_version_dir(file_id).map_err(|e| {
            error!("Failed to create version directory: {:?}", e);
            ServiceError::InternalServerError
        })?;

        // Generate initial version ID
        let version_id = Uuid::new_v4().to_string();

        // Calculate content hash
        let content_hash = calculate_content_hash(content);

        // Create the initial version
        let initial_version = FileVersion {
            version_id: version_id.clone(),
            timestamp: Utc::now(),
            user_id: owner_id.to_string(),
            username: None, // Will be populated when returning to client
            message: Some("Initial version".to_string()),
            content_hash,
        };

        // Create an initial versioned metadata
        let mut versions = HashMap::new();
        versions.insert(version_id.clone(), initial_version);

        let metadata = VersionedFileMetadata {
            file_id: file_id.to_string(),
            file_name: file_name.to_string(),
            current_version: version_id.clone(),
            versions,
            branches: HashMap::new(),
            active_editors: Vec::new(),
            last_modified: Utc::now(),
            team_id,
            owner_id: owner_id.to_string(),
        };

        // Save the metadata
        save_versioned_file_metadata(&metadata)?;

        // Save the initial content
        save_file_version(file_id, &version_id, content)?;

        Ok(metadata)
    }

    // Register an active editor for a file
    pub fn register_active_editor(
        file_id: &str,
        user_id: &str,
        branch: Option<String>,
    ) -> Result<Vec<ActiveEditor>, ServiceError> {
        let mut metadata = load_versioned_file_metadata(file_id)?;

        // Remove existing entries for this user (in case of reconnection)
        metadata.active_editors.retain(|editor| editor.user_id != user_id);

        // Add the new active editor
        let editor = ActiveEditor {
            user_id: user_id.to_string(),
            username: None, // Will be populated when returning to client
            editing_since: Utc::now(),
            branch,
        };

        metadata.active_editors.push(editor);

        // Save the updated metadata
        save_versioned_file_metadata(&metadata)?;

        Ok(metadata.active_editors)
    }

    // Unregister an active editor
    pub fn unregister_active_editor(
        file_id: &str,
        user_id: &str,
    ) -> Result<Vec<ActiveEditor>, ServiceError> {
        let mut metadata = load_versioned_file_metadata(file_id)?;

        // Remove entries for this user
        metadata.active_editors.retain(|editor| editor.user_id != user_id);

        // Save the updated metadata
        save_versioned_file_metadata(&metadata)?;

        Ok(metadata.active_editors)
    }

    // Check if file has active editors (excluding the specified user)
    pub fn has_other_active_editors(
        file_id: &str,
        current_user_id: &str,
    ) -> Result<bool, ServiceError> {
        let metadata = load_versioned_file_metadata(file_id)?;

        // Check if any other users are editing this file
        Ok(metadata.active_editors.iter().any(|editor| editor.user_id != current_user_id))
    }

    // Create a new branch for a file
    pub fn create_branch(
        file_id: &str,
        branch_name: &str,
        base_version: &str,
        user_id: &str,
        initial_content: Option<&str>,
    ) -> Result<FileBranch, ServiceError> {
        let mut metadata = load_versioned_file_metadata(file_id)?;

        // Validate the base version exists
        if !metadata.versions.contains_key(base_version) && base_version != "initial" {
            return Err(ServiceError::BadRequest(format!("Base version {} not found", base_version)));
        }

        // Generate branch ID
        let branch_id = Uuid::new_v4().to_string();

        // Create new branch version ID if content is provided
        let head_version = if let Some(content) = initial_content {
            let version_id = Uuid::new_v4().to_string();
            let content_hash = calculate_content_hash(content);

            // Create version entry
            let version = FileVersion {
                version_id: version_id.clone(),
                timestamp: Utc::now(),
                user_id: user_id.to_string(),
                username: None,
                message: Some(format!("Created branch: {}", branch_name)),
                content_hash,
            };

            // Save version content
            save_file_version(file_id, &version_id, content)?;

            // Add to versions map
            metadata.versions.insert(version_id.clone(), version);

            version_id
        } else {
            base_version.to_string()
        };

        // Create branch
        let branch = FileBranch {
            branch_id: branch_id.clone(),
            name: branch_name.to_string(),
            created_by: user_id.to_string(),
            created_at: Utc::now(),
            base_version: base_version.to_string(),
            head_version,
        };

        // Add to branches map
        metadata.branches.insert(branch_id.clone(), branch.clone());

        // Save metadata
        save_versioned_file_metadata(&metadata)?;

        Ok(branch)
    }

    // Update a branch's head version
    pub fn update_branch_head(
        file_id: &str,
        branch_id: &str,
        new_head_version: &str,
    ) -> Result<(), ServiceError> {
        let mut metadata = load_versioned_file_metadata(file_id)?;

        // Update the branch
        if let Some(branch) = metadata.branches.get_mut(branch_id) {
            branch.head_version = new_head_version.to_string();
            save_versioned_file_metadata(&metadata)?;
            Ok(())
        } else {
            Err(ServiceError::BadRequest(format!("Branch {} not found", branch_id)))
        }
    }

    // Get all versions for a file, optionally filtered by branch
    pub fn get_file_versions(
        file_id: &str,
        branch: Option<&str>,
        limit: Option<usize>,
        skip: Option<usize>,
    ) -> Result<(Vec<FileVersion>, usize, String), ServiceError> {
        let metadata = load_versioned_file_metadata(file_id)?;

        let versions: Vec<FileVersion> = if let Some(branch_id) = branch {
            // Get branch-specific versions
            if let Some(branch) = metadata.branches.get(branch_id) {
                // Get all versions in the branch history
                vec![metadata.versions.get(&branch.head_version)
                    .ok_or_else(|| ServiceError::InternalServerError)?
                    .clone()]
            } else {
                return Err(ServiceError::BadRequest(format!("Branch {} not found", branch_id)));
            }
        } else {
            // Get main branch versions
            metadata.versions.values().cloned().collect()
        };

        // Sort by timestamp (newest first)
        let mut sorted_versions = versions;
        sorted_versions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Total count
        let total_count = sorted_versions.len();

        // Apply pagination
        let skip_count = skip.unwrap_or(0);
        let paginated_versions = if let Some(limit_count) = limit {
            sorted_versions.into_iter()
                .skip(skip_count)
                .take(limit_count)
                .collect()
        } else {
            sorted_versions.into_iter()
                .skip(skip_count)
                .collect()
        };

        Ok((paginated_versions, total_count, metadata.current_version))
    }
}

pub mod diff_utils {
    use super::*;

    // Compare two text versions and identify changes and conflicts
    pub fn compare_versions(
        base_content: &str,
        your_content: &str,
        their_content: &str,
    ) -> DiffResponse {
        // First pass: get line-by-line diffs
        let your_changes = diff_text(base_content, your_content);
        let their_changes = diff_text(base_content, their_content);

        // Detect conflicts
        let conflicts = detect_conflicts(&your_changes, &their_changes, base_content, your_content, their_content);

        // Determine if auto-merge is possible
        let can_auto_merge = conflicts.is_empty();

        DiffResponse {
            base_version: "base".to_string(), // These will be replaced with actual version IDs
            compare_version: "compare".to_string(),
            changes: [your_changes, their_changes].concat(),
            conflicts,
            can_auto_merge,
        }
    }

    // Get changes between two text versions
    fn diff_text(old_text: &str, new_text: &str) -> Vec<TextChange> {
        let mut changes = Vec::new();
        let diff = TextDiff::from_lines(old_text, new_text);

        let mut line_number = 0;

        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Delete => {
                    let start_line = line_number;
                    let end_line = line_number + 1;
                    changes.push(TextChange {
                        start_line,
                        end_line,
                        content: change.value().to_string(),
                    });
                    line_number += 1;
                },
                ChangeTag::Insert => {
                    let start_line = line_number;
                    let end_line = line_number;
                    changes.push(TextChange {
                        start_line,
                        end_line,
                        content: change.value().to_string(),
                    });
                },
                ChangeTag::Equal => {
                    line_number += 1;
                }
            }
        }

        changes
    }

    // Detect conflicts between two sets of changes
    fn detect_conflicts(
        your_changes: &[TextChange],
        their_changes: &[TextChange],
        base_content: &str,
        your_content: &str,
        their_content: &str,
    ) -> Vec<Conflict> {
        let mut conflicts = Vec::new();
        let base_lines: Vec<&str> = base_content.lines().collect();
        let your_lines: Vec<&str> = your_content.lines().collect();
        let their_lines: Vec<&str> = their_content.lines().collect();

        // Check for overlapping changes
        for your_change in your_changes {
            for their_change in their_changes {
                if changes_overlap(your_change, their_change) {
                    // Extract the relevant content sections
                    let base_section = extract_lines(&base_lines, your_change.start_line, your_change.end_line);
                    let your_section = extract_lines(&your_lines, your_change.start_line, your_change.end_line);
                    let their_section = extract_lines(&their_lines, their_change.start_line, their_change.end_line);

                    conflicts.push(Conflict {
                        start_line: your_change.start_line,
                        end_line: your_change.end_line,
                        base_content: base_section,
                        your_content: your_section,
                        current_content: their_section,
                    });
                }
            }
        }

        conflicts
    }

    // Check if two changes overlap
    fn changes_overlap(change1: &TextChange, change2: &TextChange) -> bool {
        // Two changes overlap if:
        // 1. The start of change1 is between the start and end of change2
        // 2. The end of change1 is between the start and end of change2
        // 3. The start of change2 is between the start and end of change1
        // 4. The end of change2 is between the start and end of change1

        (change1.start_line >= change2.start_line && change1.start_line <= change2.end_line) ||
            (change1.end_line >= change2.start_line && change1.end_line <= change2.end_line) ||
            (change2.start_line >= change1.start_line && change2.start_line <= change1.end_line) ||
            (change2.end_line >= change1.start_line && change2.end_line <= change1.end_line)
    }

    // Extract lines from a text slice
    fn extract_lines(lines: &[&str], start_line: usize, end_line: usize) -> String {
        let start = start_line.min(lines.len());
        let end = end_line.min(lines.len());

        lines[start..end].join("\n")
    }

    // Try to auto-merge changes
    pub fn attempt_auto_merge(
        base_content: &str,
        your_content: &str,
        their_content: &str,
    ) -> Option<String> {
        let diff_result = compare_versions(base_content, your_content, their_content);

        if !diff_result.can_auto_merge {
            return None;
        }

        // A simple 3-way merge algorithm
        let base_lines: Vec<&str> = base_content.lines().collect();
        let your_lines: Vec<&str> = your_content.lines().collect();
        let their_lines: Vec<&str> = their_content.lines().collect();

        let mut result_lines: Vec<String> = Vec::new();

        // Maximum line count
        let max_lines = your_lines.len().max(their_lines.len()).max(base_lines.len());

        for i in 0..max_lines {
            if i < your_lines.len() && i < their_lines.len() && i < base_lines.len() {
                // All three versions have this line
                if your_lines[i] != base_lines[i] && their_lines[i] != base_lines[i] {
                    if your_lines[i] == their_lines[i] {
                        // Both made the same change
                        result_lines.push(your_lines[i].to_string());
                    } else {
                        // Conflict
                        return None;
                    }
                } else if your_lines[i] != base_lines[i] {
                    // You changed this line
                    result_lines.push(your_lines[i].to_string());
                } else if their_lines[i] != base_lines[i] {
                    // They changed this line
                    result_lines.push(their_lines[i].to_string());
                } else {
                    // No changes
                    result_lines.push(base_lines[i].to_string());
                }
            } else if i < your_lines.len() && i < base_lines.len() {
                // Your version and base have this line
                result_lines.push(your_lines[i].to_string());
            } else if i < their_lines.len() && i < base_lines.len() {
                // Their version and base have this line
                result_lines.push(their_lines[i].to_string());
            } else if i < your_lines.len() {
                // Only your version has this line
                result_lines.push(your_lines[i].to_string());
            } else if i < their_lines.len() {
                // Only their version has this line
                result_lines.push(their_lines[i].to_string());
            }
        }

        Some(result_lines.join("\n"))
    }

    // Create a merged text with conflict markers
    pub fn create_marked_merge(
        base_content: &str,
        your_content: &str,
        their_content: &str,
    ) -> String {
        let diff_result = compare_versions(base_content, your_content, their_content);

        if diff_result.conflicts.is_empty() {
            // If there are no conflicts, attempt to auto-merge
            return attempt_auto_merge(base_content, your_content, their_content)
                .unwrap_or_else(|| their_content.to_string());
        }

        // Split the content into lines
        let their_lines: Vec<&str> = their_content.lines().collect();

        // Create a mutable copy to work with
        let mut result_lines: Vec<String> = their_lines.iter().map(|s| s.to_string()).collect();

        // Apply conflict markers for each conflict
        // Process in reverse order to avoid affecting line numbers
        for conflict in diff_result.conflicts.iter().rev() {
            let start = conflict.start_line;
            let end = conflict.end_line;

            // Replace the lines with conflict markers
            let conflict_section: Vec<String> = vec![
                "<<<<<<< CURRENT CHANGES".to_string(),
                conflict.current_content.clone(),
                "=======".to_string(),
                conflict.your_content.clone(),
                ">>>>>>> YOUR CHANGES".to_string(),
            ];

            // Replace the affected lines
            if start < result_lines.len() {
                let replace_end = end.min(result_lines.len());
                result_lines.splice(start..replace_end, conflict_section);
            } else {
                // Append if we're past the end
                result_lines.extend(conflict_section);
            }
        }

        result_lines.join("\n")
    }
}

// Helper to calculate a hash for content
fn calculate_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

// Extract content from text with conflict markers
pub fn extract_resolved_content(content: &str) -> String {
    // Regular expressions to find and remove conflict markers
    let conflict_start_re = Regex::new(r"<<<<<<< .*\n").unwrap();
    let conflict_separator_re = Regex::new(r"=======\n").unwrap();
    let conflict_end_re = Regex::new(r">>>>>>> .*\n").unwrap();
    let mut result = content.to_string(); // assumes no conflicts present

    // If conflict markers are found
    if conflict_start_re.is_match(&result) {
        let lines: Vec<&str> = content.lines().collect();
        let mut result_lines = Vec::new();
        let mut in_conflict = false;
        let mut current_section = true; // true for current changes, false for your changes

        for line in lines {
            if line.starts_with("<<<<<<< ") {
                in_conflict = true;
                current_section = true;
                continue;
            } else if line == "=======" {
                current_section = false;
                continue;
            } else if line.starts_with(">>>>>>> ") {
                in_conflict = false;
                continue;
            }

            if !in_conflict || !current_section {
                result_lines.push(line);
            }
        }

        result = result_lines.join("\n");
    }

    result
}