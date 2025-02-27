use crate::models::{Claims, ServiceError, User};
use actix_web::http::header;
use actix_web::{dev::ServiceRequest, Error, HttpMessage};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;
use uuid::Uuid;

// JWT utility functions
pub mod jwt {
    use super::*;

    // Get JWT secret from environment or use default
    fn get_jwt_secret() -> String {
        env::var("JWT_SECRET").unwrap_or_else(|_| "laminotes_super_secret_key".to_string())
    }

    // Generate a new JWT token for a user
    pub fn generate_token(user: &User) -> Result<String, ServiceError> {
        let secret = get_jwt_secret();
        let expiration = Utc::now()
            .checked_add_signed(Duration::days(7))
            .expect("Valid timestamp")
            .timestamp() as usize;

        let claims = Claims {
            sub: user.id.clone(),
            email: user.email.clone(),
            exp: expiration,
            iat: Utc::now().timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_ref()),
        )
            .map_err(|_| ServiceError::InternalServerError)
    }

    // Validate and decode a JWT token
    pub fn decode_token(token: &str) -> Result<Claims, ServiceError> {
        let secret = get_jwt_secret();

        decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_ref()),
            &Validation::default(),
        )
            .map(|data| data.claims)
            .map_err(|_| ServiceError::Unauthorized)
    }

    // Extract JWT from Authorization header
    pub fn extract_token_from_header(auth_header: &str) -> Result<String, ServiceError> {
        if !auth_header.starts_with("Bearer ") {
            return Err(ServiceError::Unauthorized);
        }

        Ok(auth_header.trim_start_matches("Bearer ").to_string())
    }
}

// Password utility functions
pub mod password {
    use super::*;

    // Hash a password using bcrypt
    pub fn hash_password(password: &str) -> Result<String, ServiceError> {
        hash(password, DEFAULT_COST)
            .map_err(|_| ServiceError::InternalServerError)
    }

    // Verify a password against a hash
    pub fn verify_password(password: &str, hash: &str) -> Result<bool, ServiceError> {
        verify(password, hash)
            .map_err(|_| ServiceError::InternalServerError)
    }
}

// User storage utilities
pub mod user_storage {
    use super::*;

    const USERS_DIR: &str = "./storage/users";

    // Get the path for a user's file storage
    pub fn get_user_storage_path(user_id: &str) -> String {
        format!("./storage/{}", user_id)
    }

    // Save a user to storage
    pub fn save_user(user: &User) -> Result<(), ServiceError> {
        let user_path = format!("{}/{}.json", USERS_DIR, user.id);

        fs::write(
            &user_path,
            serde_json::to_string(&user).map_err(|_| ServiceError::InternalServerError)?,
        )
            .map_err(|_| ServiceError::InternalServerError)
    }

    // Find a user by email
    pub fn find_user_by_email(email: &str) -> Result<Option<User>, ServiceError> {
        let users_dir = Path::new(USERS_DIR);

        if !users_dir.exists() {
            fs::create_dir_all(users_dir).map_err(|_| ServiceError::InternalServerError)?;
            return Ok(None);
        }

        for entry in fs::read_dir(users_dir).map_err(|_| ServiceError::InternalServerError)? {
            let entry = entry.map_err(|_| ServiceError::InternalServerError)?;
            let path = entry.path();

            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                let content = fs::read_to_string(&path).map_err(|_| ServiceError::InternalServerError)?;
                let user: User = serde_json::from_str(&content).map_err(|_| ServiceError::InternalServerError)?;

                if user.email == email {
                    return Ok(Some(user));
                }
            }
        }

        Ok(None)
    }

    // Find a user by ID
    pub fn find_user_by_id(id: &str) -> Result<Option<User>, ServiceError> {
        let user_path = format!("{}/{}.json", USERS_DIR, id);
        let path = Path::new(&user_path);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).map_err(|_| ServiceError::InternalServerError)?;
        let user: User = serde_json::from_str(&content).map_err(|_| ServiceError::InternalServerError)?;

        Ok(Some(user))
    }
}

// Middleware for JWT authentication
pub mod auth_middleware {
    use super::*;
    use actix_web::dev::{forward_ready, Service, Transform};
    use actix_web::{error::ErrorUnauthorized, Error};
    use futures::future::{ok, ready, Ready};
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub struct Authentication;

    impl<S, B> Transform<S, ServiceRequest> for Authentication
    where
        S: Service<ServiceRequest, Response = actix_web::dev::ServiceResponse<B>, Error = Error>,
        S::Future: 'static,
        B: 'static,
    {
        type Response = actix_web::dev::ServiceResponse<B>;
        type Error = Error;
        type Transform = AuthenticationMiddleware<S>;
        type InitError = ();
        type Future = Ready<Result<Self::Transform, Self::InitError>>;

        fn new_transform(&self, service: S) -> Self::Future {
            ok(AuthenticationMiddleware { service })
        }
    }

    pub struct AuthenticationMiddleware<S> {
        service: S,
    }

    impl<S, B> Service<ServiceRequest> for AuthenticationMiddleware<S>
    where
        S: Service<ServiceRequest, Response = actix_web::dev::ServiceResponse<B>, Error = Error>,
        S::Future: 'static,
        B: 'static,
    {
        type Response = actix_web::dev::ServiceResponse<B>;
        type Error = Error;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

        forward_ready!(service);

        fn call(&self, req: ServiceRequest) -> Self::Future {
            // Get Authorization header
            let auth_header = req.headers().get(header::AUTHORIZATION);

            if let Some(auth_header) = auth_header {
                if let Ok(auth_str) = auth_header.to_str() {
                    if let Ok(token) = jwt::extract_token_from_header(auth_str) {
                        if let Ok(claims) = jwt::decode_token(&token) {
                            // Add the claims to the request extensions
                            req.extensions_mut().insert(claims);
                            let fut = self.service.call(req);
                            return Box::pin(async move {
                                fut.await
                            });
                        }
                    }
                }
            }

            Box::pin(async move {
                Err(ErrorUnauthorized("Unauthorized"))
            })
        }
    }
}

// File system utilities
pub mod fs_utils {
    use super::*;
    use std::io;

    // Ensure a user's storage directory exists
    pub fn ensure_user_directory(user_id: &str) -> io::Result<()> {
        let user_dir = format!("./storage/{}", user_id);
        if !Path::new(&user_dir).exists() {
            fs::create_dir_all(&user_dir)?;
        }
        Ok(())
    }

    // List all files in a user's directory
    pub fn list_user_files(user_id: &str) -> io::Result<Vec<String>> {
        let user_dir = format!("./storage/{}", user_id);
        let user_path = Path::new(&user_dir);

        if !user_path.exists() {
            fs::create_dir_all(user_path)?;
            return Ok(Vec::new());
        }

        let mut file_names = Vec::new();

        for entry in fs::read_dir(user_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                // Skip metadata files
                if let Some(filename) = path.file_name() {
                    if let Some(filename_str) = filename.to_str() {
                        if !filename_str.ends_with(".meta") {
                            file_names.push(filename_str.to_owned());
                        }
                    }
                }
            }
        }

        Ok(file_names)
    }
}