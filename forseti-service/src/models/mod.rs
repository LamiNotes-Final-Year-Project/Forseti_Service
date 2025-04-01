// forseti-service/src/models/mod.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::fmt;
use actix_web::{HttpResponse, ResponseError};

// Import the file_version module
pub mod file_version;
pub use file_version::*;

// Add invitation module
pub mod invitations;
pub use invitations::*;

// File upload and metadata models
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadRequest {
    pub file_content: String,
    pub metadata: Option<FileMetadata>,
    pub team_id: Option<String>, // Add team_id field
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub file_id: Option<String>,
    pub file_name: String,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub last_modified: Option<DateTime<Utc>>,
    pub team_id: Option<String>, // Add team_id field
    // New fields for versioning
    pub current_version: Option<String>,
    pub versioned: Option<bool>,
}

// Team models
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum TeamRole {
    Viewer = 0,
    Contributor = 1,
    Owner = 2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TeamMember {
    pub user_id: String,
    pub team_id: String,
    pub role: TeamRole,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub access_expires: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TeamData {
    pub name: String,
}

// User models for authentication
#[derive(Serialize, Deserialize, Debug)]
pub struct UserCredentials {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub email: String,
}

// JWT claims structure for authentication
#[derive(Serialize, Deserialize, Debug)]
pub struct Claims {
    pub sub: String,  // Subject (user ID)
    pub email: String,
    pub exp: usize,   // Expiration time
    pub iat: usize,   // Issued at
    pub active_team_id: Option<String>, // Add field for active team
}

// Custom error types
#[derive(Debug)]
pub enum ServiceError {
    InternalServerError,
    BadRequest(String),
    Unauthorized,
    NotFound,
    Forbidden,
    Conflict(String),
}

// Implement Display for ServiceError
impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServiceError::InternalServerError => write!(f, "Internal Server Error"),
            ServiceError::BadRequest(msg) => write!(f, "BadRequest: {}", msg),
            ServiceError::Unauthorized => write!(f, "Unauthorized"),
            ServiceError::NotFound => write!(f, "Not Found"),
            ServiceError::Forbidden => write!(f, "Forbidden"),
            ServiceError::Conflict(msg) => write!(f, "Conflict: {}", msg),
        }
    }
}

// Implement std::error::Error for ServiceError
impl std::error::Error for ServiceError {}

// Implement ResponseError for ServiceError
impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::InternalServerError =>
                HttpResponse::InternalServerError().json("Internal Server Error"),
            ServiceError::BadRequest(ref message) =>
                HttpResponse::BadRequest().json(message),
            ServiceError::Unauthorized =>
                HttpResponse::Unauthorized().json("Unauthorized"),
            ServiceError::NotFound =>
                HttpResponse::NotFound().json("Not Found"),
            ServiceError::Forbidden =>
                HttpResponse::Forbidden().json("Forbidden: You don't have permission to access this resource"),
            ServiceError::Conflict(ref message) =>
                HttpResponse::Conflict().json(message),
        }
    }
}