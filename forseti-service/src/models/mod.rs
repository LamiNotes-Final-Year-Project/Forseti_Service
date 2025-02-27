use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::fmt;
use actix_web::{HttpResponse, ResponseError};

// File upload and metadata models
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadRequest {
    pub file_content: String,
    pub metadata: Option<FileMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub file_id: Option<String>,
    pub file_name: String,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub last_modified: Option<DateTime<Utc>>,
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
}

// Custom error types
#[derive(Debug)]
pub enum ServiceError {
    InternalServerError,
    BadRequest(String),
    Unauthorized,
    NotFound,
}

// Implement Display for ServiceError
impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServiceError::InternalServerError => write!(f, "Internal Server Error"),
            ServiceError::BadRequest(msg) => write!(f, "BadRequest: {}", msg),
            ServiceError::Unauthorized => write!(f, "Unauthorized"),
            ServiceError::NotFound => write!(f, "Not Found"),
        }
    }
}

// Implement std::error::Error for ServiceError
impl std::error::Error for ServiceError {}

// Implement ResponseError for ServiceError
impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::InternalServerError => HttpResponse::InternalServerError().json("Internal Server Error"),
            ServiceError::BadRequest(ref message) => HttpResponse::BadRequest().json(message),
            ServiceError::Unauthorized => HttpResponse::Unauthorized().json("Unauthorized"),
            ServiceError::NotFound => HttpResponse::NotFound().json("Not Found"),
        }
    }
}