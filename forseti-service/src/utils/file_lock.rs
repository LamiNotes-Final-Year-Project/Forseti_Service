use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use chrono::Utc;
use log::{debug, info, warn, error};
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error as ActixError,
    HttpMessage, HttpResponse,
};
use futures::future::{ok, ready, Ready};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::fs;
use std::path::Path;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

use crate::models::ServiceError;
use crate::utils::UserContext;
use crate::utils::version_control::version_storage;

// File lock entry
#[derive(Clone, Debug)]
struct FileLock {
    user_id: String,
    file_id: String,
    acquired_at: Instant,
    expires_at: Instant,
    // Lock duration in seconds
    duration: u64,
}

// Global lock registry
#[derive(Clone)]
pub struct FileLockRegistry {
    locks: Arc<Mutex<HashMap<String, FileLock>>>,
}

impl FileLockRegistry {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Try to acquire a lock for a file
    pub fn try_acquire_lock(&self, file_id: &str, user_id: &str, duration_secs: u64) -> Result<bool, String> {
        let mut locks = self.locks.lock().map_err(|e| format!("Lock error: {:?}", e))?;

        // Check if file is already locked
        let lock_exists = locks.get(file_id).cloned();

        if let Some(lock) = lock_exists {
            // If locked by the same user, renew the lock
            if lock.user_id == user_id {
                debug!("Renewing lock for file_id={}, user_id={}", file_id, user_id);
                let now = Instant::now();
                let expires_at = now + Duration::from_secs(duration_secs);

                // Create a new lock with updated expiration
                locks.insert(file_id.to_string(), FileLock {
                    user_id: user_id.to_string(),
                    file_id: file_id.to_string(),
                    acquired_at: lock.acquired_at, // Keep original acquisition time
                    expires_at,
                    duration: duration_secs,
                });

                return Ok(true);
            }

            // Check if lock has expired
            if lock.expires_at <= Instant::now() {
                debug!("Lock expired for file_id={}, previously held by user_id={}",
                  file_id, lock.user_id);
                // Lock expired, remove it
                locks.remove(file_id);
            } else {
                // Lock is still valid and held by another user
                return Ok(false);
            }
        }

        // No valid lock exists, create a new one
        let now = Instant::now();
        let expires_at = now + Duration::from_secs(duration_secs);
        locks.insert(file_id.to_string(), FileLock {
            user_id: user_id.to_string(),
            file_id: file_id.to_string(),
            acquired_at: now,
            expires_at,
            duration: duration_secs,
        });

        debug!("Acquired lock for file_id={}, user_id={}, expires in {} seconds",
          file_id, user_id, duration_secs);
        Ok(true)
    }

    // Release a lock for a file
    pub fn release_lock(&self, file_id: &str, user_id: &str) -> Result<bool, String> {
        let mut locks = self.locks.lock().map_err(|e| format!("Lock error: {:?}", e))?;

        // Check if file is locked by this user
        if let Some(lock) = locks.get(file_id) {
            if lock.user_id == user_id {
                locks.remove(file_id);
                debug!("Released lock for file_id={}, user_id={}", file_id, user_id);
                return Ok(true);
            }
            debug!("Cannot release lock for file_id={}, not owned by user_id={}",
                  file_id, user_id);
            return Ok(false);
        }

        debug!("No lock found for file_id={}", file_id);
        Ok(false)
    }

    // Check if a file is locked
    pub fn is_file_locked(&self, file_id: &str) -> Result<Option<String>, String> {
        let locks = self.locks.lock().map_err(|e| format!("Lock error: {:?}", e))?;

        // Check if file is locked
        if let Some(lock) = locks.get(file_id) {
            // Check if lock has expired
            if lock.expires_at <= Instant::now() {
                return Ok(None);
            }
            return Ok(Some(lock.user_id.clone()));
        }

        Ok(None)
    }

    // Check if a user can edit a file
    pub fn can_user_edit(&self, file_id: &str, user_id: &str) -> Result<bool, String> {
        if let Some(lock_user_id) = self.is_file_locked(file_id)? {
            return Ok(lock_user_id == user_id);
        }
        // No lock, anyone can edit
        Ok(true)
    }

    // Remove expired locks
    pub fn cleanup_expired_locks(&self) -> Result<usize, String> {
        let mut locks = self.locks.lock().map_err(|e| format!("Lock error: {:?}", e))?;
        let now = Instant::now();
        let expired_count = locks.len();

        // Remove expired locks
        locks.retain(|_, lock| lock.expires_at > now);

        let new_count = locks.len();
        let removed = expired_count - new_count;

        if removed > 0 {
            debug!("Removed {} expired locks", removed);
        }

        Ok(removed)
    }

    // Get all locks for debugging
    pub fn get_all_locks(&self) -> Result<Vec<LockInfo>, String> {
        let locks = self.locks.lock().map_err(|e| format!("Lock error: {:?}", e))?;
        let now = Instant::now();

        let lock_infos: Vec<LockInfo> = locks.values().map(|lock| {
            let remaining_secs = if lock.expires_at > now {
                lock.expires_at.duration_since(now).as_secs()
            } else {
                0
            };

            LockInfo {
                file_id: lock.file_id.clone(),
                user_id: lock.user_id.clone(),
                acquired_at: format!("{} seconds ago", now.duration_since(lock.acquired_at).as_secs()),
                expires_in: format!("{} seconds", remaining_secs),
                is_expired: lock.expires_at <= now,
            }
        }).collect();

        Ok(lock_infos)
    }
}

// Lock info for serialization
#[derive(Serialize, Deserialize, Debug)]
pub struct LockInfo {
    pub file_id: String,
    pub user_id: String,
    pub acquired_at: String,
    pub expires_in: String,
    pub is_expired: bool,
}

// Create a singleton lock registry
lazy_static::lazy_static! {
    pub static ref LOCK_REGISTRY: FileLockRegistry = FileLockRegistry::new();
}

// Middleware for file locking
pub struct FileLockMiddleware;

impl<S, B> Transform<S, ServiceRequest> for FileLockMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type Transform = FileLockMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(FileLockMiddlewareService { service }))
    }
}

pub struct FileLockMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for FileLockMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Ignore GET requests or requests that aren't to specific version endpoints
        let method = req.method().clone();
        let path = req.path().to_owned();

        // Periodically clean up expired locks
        if let Err(e) = LOCK_REGISTRY.cleanup_expired_locks() {
            warn!("Error cleaning up expired locks: {}", e);
        }

        if method == actix_web::http::Method::POST &&
            (path.contains("/files/") &&
                (path.ends_with("/save") || path.ends_with("/edit"))) {

            // Extract file_id from path
            // Path format: /files/{file_id}/{action}
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() >= 3 {
                let file_id = parts[2];

                // Get user_id from request
                if let Some(context) = req.extensions().get::<UserContext>() {
                    let user_id = &context.user_id;

                    // Check if file is being edited by someone else
                    match LOCK_REGISTRY.is_file_locked(file_id) {
                        Ok(Some(lock_user_id)) if lock_user_id != *user_id => {
                            info!("🔒 File {file_id} is locked by user {lock_user_id}, current user is {user_id}");

                            // Get additional info about who is editing
                            let mut editors = Vec::new();
                            if let Ok(metadata) = version_storage::load_versioned_file_metadata(file_id) {
                                editors = metadata.active_editors;
                            }

                            // Return a conflict response with proper type
                            let error_response = HttpResponse::Conflict()
                                .json(serde_json::json!({
                                "status": "locked",
                                "message": "This file is currently being edited by another user",
                                "lock_holder": lock_user_id,
                                "active_editors": editors
                            }));

                            return Box::pin(async move {
                                let err = actix_web::error::ErrorConflict(serde_json::json!({
                                    "status": "locked",
                                    "message": "This file is currently being edited by another user",
                                    "lock_holder": lock_user_id,
                                    "active_editors": editors
                                }));

                                Err(err.into())
                            });
                        },
                        Ok(Some(_)) => {
                            // File is locked by current user, allow the request
                            debug!("File {file_id} is locked by current user {user_id}, allowing request");
                        },
                        Ok(None) => {
                            // File is not locked, allow the request
                            if path.ends_with("/edit") {
                                // If this is an edit request, try to acquire a lock
                                match LOCK_REGISTRY.try_acquire_lock(file_id, user_id, 300) { // 5 minute lock
                                    Ok(true) => {
                                        debug!("Acquired lock for file {file_id} by user {user_id}");
                                    },
                                    Ok(false) => {
                                        warn!("Failed to acquire lock for file {file_id} by user {user_id}, but no existing lock was found");
                                    },
                                    Err(e) => {
                                        error!("Error acquiring lock: {}", e);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            error!("Error checking file lock: {}", e);
                        }
                    }
                }
            }
        } else if method == actix_web::http::Method::POST && path.contains("/files/") && path.ends_with("/release") {
            // Handle releasing locks when explicitly requested
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() >= 3 {
                let file_id = parts[2];

                if let Some(context) = req.extensions().get::<UserContext>() {
                    let user_id = &context.user_id;

                    // Try to release the lock
                    if let Err(e) = LOCK_REGISTRY.release_lock(file_id, user_id) {
                        error!("Error releasing lock: {}", e);
                    }
                }
            }
        }

        // Continue with the request
        let fut = self.service.call(req);
        Box::pin(async move {
            fut.await
        })
    }
}
// Routes for managing file locks (for admin and debugging)
pub mod lock_routes {
    use super::*;
    use actix_web::{web, get, post, delete, HttpRequest, HttpResponse};
    use crate::utils::get_user_id_from_request;

    // Get all locks (admin only)
    #[get("/admin/locks")]
    pub async fn get_all_locks(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
        let user_id = get_user_id_from_request(&req)?;

        //TODO: implement admin only

        match LOCK_REGISTRY.get_all_locks() {
            Ok(locks) => Ok(HttpResponse::Ok().json(locks)),
            Err(e) => Err(ServiceError::InternalServerError)
        }
    }

    // Manually acquire a lock
    #[post("/files/{file_id}/lock")]
    pub async fn acquire_lock(
        req: HttpRequest,
        path: web::Path<String>,
    ) -> Result<HttpResponse, ServiceError> {
        let user_id = get_user_id_from_request(&req)?;
        let file_id = path.into_inner();

        match LOCK_REGISTRY.try_acquire_lock(&file_id, &user_id, 300) {
            Ok(true) => Ok(HttpResponse::Ok().json(serde_json::json!({
                "status": "locked",
                "message": "Lock acquired successfully",
                "file_id": file_id,
                "user_id": user_id
            }))),
            Ok(false) => {
                // Get lock holder
                match LOCK_REGISTRY.is_file_locked(&file_id) {
                    Ok(Some(lock_holder)) => Err(ServiceError::Conflict(
                        format!("File is already locked by user {}", lock_holder)
                    )),
                    _ => Err(ServiceError::InternalServerError)
                }
            },
            Err(e) => Err(ServiceError::InternalServerError)
        }
    }

    // Manually release a lock
    #[delete("/files/{file_id}/lock")]
    pub async fn release_lock(
        req: HttpRequest,
        path: web::Path<String>,
    ) -> Result<HttpResponse, ServiceError> {
        let user_id = get_user_id_from_request(&req)?;
        let file_id = path.into_inner();

        match LOCK_REGISTRY.release_lock(&file_id, &user_id) {
            Ok(true) => Ok(HttpResponse::Ok().json(serde_json::json!({
                "status": "released",
                "message": "Lock released successfully",
                "file_id": file_id
            }))),
            Ok(false) => {
                // Check if locked by someone else
                match LOCK_REGISTRY.is_file_locked(&file_id) {
                    Ok(Some(lock_holder)) if lock_holder != user_id => Err(ServiceError::Forbidden),
                    _ => Err(ServiceError::NotFound)
                }
            },
            Err(e) => Err(ServiceError::InternalServerError)
        }
    }

    // Check lock status
    #[get("/files/{file_id}/lock")]
    pub async fn check_lock(
        req: HttpRequest,
        path: web::Path<String>,
    ) -> Result<HttpResponse, ServiceError> {
        let file_id = path.into_inner();

        match LOCK_REGISTRY.is_file_locked(&file_id) {
            Ok(Some(user_id)) => Ok(HttpResponse::Ok().json(serde_json::json!({
                "status": "locked",
                "locked_by": user_id,
                "file_id": file_id
            }))),
            Ok(None) => Ok(HttpResponse::Ok().json(serde_json::json!({
                "status": "unlocked",
                "file_id": file_id
            }))),
            Err(e) => Err(ServiceError::InternalServerError)
        }
    }

    // Configure lock routes
    pub fn init_routes(cfg: &mut web::ServiceConfig) {
        cfg.service(get_all_locks)
            .service(acquire_lock)
            .service(release_lock)
            .service(check_lock);
    }
}