#[cfg(test)]
mod tests {
    use actix_web::{test, web, App};
    use forseti_service::routes::version_routes;
    use forseti_service::utils::Auth;
    use forseti_service::utils::file_lock::FileLockMiddleware;
    use std::fs;
    use std::path::Path;
    use uuid::Uuid;
    use serde_json::json;

    // Helper function to create test file
    async fn create_test_file(app: &mut test::TestApp, content: &str) -> (String, String) {
        // Generate a unique file name
        let file_id = Uuid::new_v4().to_string();
        let file_name = format!("test_file_{}.md", file_id);

        // Create upload request
        let request = test::TestRequest::post()
            .uri(&format!("/upload/{}", file_name))
            .set_json(&json!({
                "file_content": content,
                "metadata": {
                    "file_name": file_name
                }
            }))
            .to_request();

        let response: serde_json::Value = test::call_and_read_body_json(app, request).await;

        (file_id, file_name)
    }

    #[actix_rt::test]
    async fn test_file_versioning() {
        // Initialize test environment
        let initial_content = "# Test File\n\nThis is version 1.";
        let second_content = "# Test File\n\nThis is version 2.";

        // Create test app
        let mut app = test::init_service(
            App::new()
                .wrap(Auth)
                .wrap(FileLockMiddleware)
                .configure(|cfg| {
                    version_routes::init_routes(cfg);
                })
        ).await;

        // Create test file
        let (file_id, file_name) = create_test_file(&mut app, initial_content).await;

        // Start editing
        let edit_request = test::TestRequest::post()
            .uri(&format!("/files/{}/edit", file_id))
            .set_json(&json!({}))
            .to_request();

        let edit_response: serde_json::Value = test::call_and_read_body_json(&mut app, edit_request).await;
        assert!(edit_response.get("active_editors").is_some(), "Should return active editors");

        // Get version history
        let history_request = test::TestRequest::get()
            .uri(&format!("/files/{}/history", file_id))
            .to_request();

        let history_response: serde_json::Value = test::call_and_read_body_json(&mut app, history_request).await;
        let versions = history_response["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 1, "Should have initial version");

        // Get the initial version id
        let initial_version = history_response["current_version"].as_str().unwrap().to_string();

        // Save new version
        let save_request = test::TestRequest::post()
            .uri(&format!("/files/{}/save", file_id))
            .set_json(&json!({
                "content": second_content,
                "base_version": initial_version,
                "message": "Updated content"
            }))
            .to_request();

        let save_response: serde_json::Value = test::call_and_read_body_json(&mut app, save_request).await;
        let status = save_response["status"].as_str().unwrap();
        assert_eq!(status, "saved", "Should save successfully");

        // Get updated version history
        let updated_history_request = test::TestRequest::get()
            .uri(&format!("/files/{}/history", file_id))
            .to_request();

        let updated_history: serde_json::Value = test::call_and_read_body_json(&mut app, updated_history_request).await;
        let updated_versions = updated_history["versions"].as_array().unwrap();
        assert_eq!(updated_versions.len(), 2, "Should have two versions now");

        // Get specific version content
        let version_request = test::TestRequest::get()
            .uri(&format!("/files/{}/versions/{}", file_id, initial_version))
            .to_request();

        let version_content = test::call_and_read_body_to_string(&mut app, version_request).await;
        assert_eq!(version_content, initial_content, "Should return initial content");

        // Release editing lock
        let release_request = test::TestRequest::post()
            .uri(&format!("/files/{}/release", file_id))
            .to_request();

        let _: serde_json::Value = test::call_and_read_body_json(&mut app, release_request).await;

        // Clean up
        if Path::new(&format!("./storage/versions/{}", file_id)).exists() {
            fs::remove_dir_all(format!("./storage/versions/{}", file_id)).unwrap();
        }
    }

    #[actix_rt::test]
    async fn test_conflict_detection() {
        // Initialize test environment
        let initial_content = "# Test File\n\nLine 1\nLine 2\nLine 3";
        let user1_content = "# Test File\n\nLine 1 modified by user 1\nLine 2\nLine 3";
        let user2_content = "# Test File\n\nLine 1\nLine 2 modified by user 2\nLine 3";

        // Create test app
        let mut app = test::init_service(
            App::new()
                .wrap(Auth)
                .wrap(FileLockMiddleware)
                .configure(|cfg| {
                    version_routes::init_routes(cfg);
                })
        ).await;

        // Create test file
        let (file_id, file_name) = create_test_file(&mut app, initial_content).await;

        // Get version history to get initial version
        let history_request = test::TestRequest::get()
            .uri(&format!("/files/{}/history", file_id))
            .to_request();

        let history_response: serde_json::Value = test::call_and_read_body_json(&mut app, history_request).await;
        let initial_version = history_response["current_version"].as_str().unwrap().to_string();

        // First save (user 1)
        let save1_request = test::TestRequest::post()
            .uri(&format!("/files/{}/save", file_id))
            .set_json(&json!({
                "content": user1_content,
                "base_version": initial_version,
                "message": "User 1 changes"
            }))
            .to_request();

        let save1_response: serde_json::Value = test::call_and_read_body_json(&mut app, save1_request).await;
        let status1 = save1_response["status"].as_str().unwrap();
        assert_eq!(status1, "saved", "First save should succeed");
        let user1_version = save1_response["new_version"].as_str().unwrap().to_string();

        // Second save with outdated base version (user 2)
        let save2_request = test::TestRequest::post()
            .uri(&format!("/files/{}/save", file_id))
            .set_json(&json!({
                "content": user2_content,
                "base_version": initial_version, // Using old base version
                "message": "User 2 changes"
            }))
            .to_request();

        let save2_response: serde_json::Value = test::call_and_read_body_json(&mut app, save2_request).await;
        let status2 = save2_response["status"].as_str().unwrap();

        // Should auto-merge since changes are in different lines
        assert_eq!(status2, "auto_merged", "Should auto-merge non-conflicting changes");

        // Clean up
        if Path::new(&format!("./storage/versions/{}", file_id)).exists() {
            fs::remove_dir_all(format!("./storage/versions/{}", file_id)).unwrap();
        }
    }

    #[actix_rt::test]
    async fn test_manual_conflict_resolution() {
        // Initialize test environment
        let initial_content = "# Test File\n\nLine 1\nLine 2\nLine 3";
        let user1_content = "# Test File\n\nLine 1 modified by user 1\nLine 2\nLine 3";
        let user2_content = "# Test File\n\nLine 1 modified by user 2\nLine 2\nLine 3";

        // Create test app
        let mut app = test::init_service(
            App::new()
                .wrap(Auth)
                .wrap(FileLockMiddleware)
                .configure(|cfg| {
                    version_routes::init_routes(cfg);
                })
        ).await;

        // Create test file
        let (file_id, file_name) = create_test_file(&mut app, initial_content).await;

        // Get version history to get initial version
        let history_request = test::TestRequest::get()
            .uri(&format!("/files/{}/history", file_id))
            .to_request();

        let history_response: serde_json::Value = test::call_and_read_body_json(&mut app, history_request).await;
        let initial_version = history_response["current_version"].as_str().unwrap().to_string();

        // First save (user 1)
        let save1_request = test::TestRequest::post()
            .uri(&format!("/files/{}/save", file_id))
            .set_json(&json!({
                "content": user1_content,
                "base_version": initial_version,
                "message": "User 1 changes"
            }))
            .to_request();

        let save1_response: serde_json::Value = test::call_and_read_body_json(&mut app, save1_request).await;
        let user1_version = save1_response["new_version"].as_str().unwrap().to_string();

        // Second save with conflicting changes (user 2)
        let save2_request = test::TestRequest::post()
            .uri(&format!("/files/{}/save", file_id))
            .set_json(&json!({
                "content": user2_content,
                "base_version": initial_version, // Using old base version
                "message": "User 2 changes"
            }))
            .to_request();

        let save2_response: serde_json::Value = test::call_and_read_body_json(&mut app, save2_request).await;
        let status2 = save2_response["status"].as_str().unwrap();

        // Should detect conflict since changes are on the same line
        assert_eq!(status2, "conflict", "Should detect conflict on same line");

        // Resolve conflict
        let merged_content = "# Test File\n\nLine 1 merged\nLine 2\nLine 3";
        let resolve_request = test::TestRequest::post()
            .uri(&format!("/files/{}/resolve-conflicts", file_id))
            .set_json(&json!({
                "content": merged_content,
                "base_version": initial_version,
                "current_version": user1_version,
                "message": "Manually resolved conflict"
            }))
            .to_request();

        let resolve_response: serde_json::Value = test::call_and_read_body_json(&mut app, resolve_request).await;
        let resolve_status = resolve_response["status"].as_str().unwrap();
        assert_eq!(resolve_status, "saved", "Should save resolved content");

        // Clean up
        if Path::new(&format!("./storage/versions/{}", file_id)).exists() {
            fs::remove_dir_all(format!("./storage/versions/{}", file_id)).unwrap();
        }
    }

    #[actix_rt::test]
    async fn test_branch_creation() {
        // Initialize test environment
        let initial_content = "# Test File\n\nThis is the main branch.";
        let branch_content = "# Test File\n\nThis is a feature branch.";

        // Create test app
        let mut app = test::init_service(
            App::new()
                .wrap(Auth)
                .wrap(FileLockMiddleware)
                .configure(|cfg| {
                    version_routes::init_routes(cfg);
                })
        ).await;

        // Create test file
        let (file_id, file_name) = create_test_file(&mut app, initial_content).await;

        // Get version history to get initial version
        let history_request = test::TestRequest::get()
            .uri(&format!("/files/{}/history", file_id))
            .to_request();

        let history_response: serde_json::Value = test::call_and_read_body_json(&mut app, history_request).await;
        let initial_version = history_response["current_version"].as_str().unwrap().to_string();

        // Create branch
        let branch_request = test::TestRequest::post()
            .uri(&format!("/files/{}/branches", file_id))
            .set_json(&json!({
                "name": "feature-branch",
                "base_version": initial_version,
                "content": branch_content
            }))
            .to_request();

        let branch_response: serde_json::Value = test::call_and_read_body_json(&mut app, branch_request).await;
        assert!(branch_response.get("branch_id").is_some(), "Should return branch ID");

        // Get branch history
        let branch_history_request = test::TestRequest::get()
            .uri(&format!("/files/{}/history?branch={}", file_id, branch_response["branch_id"].as_str().unwrap()))
            .to_request();

        let branch_history: serde_json::Value = test::call_and_read_body_json(&mut app, branch_history_request).await;
        let branch_versions = branch_history["versions"].as_array().unwrap();
        assert!(branch_versions.len() > 0, "Should have branch versions");

        // Clean up
        if Path::new(&format!("./storage/versions/{}", file_id)).exists() {
            fs::remove_dir_all(format!("./storage/versions/{}", file_id)).unwrap();
        }
    }
}