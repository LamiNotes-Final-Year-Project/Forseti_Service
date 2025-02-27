use crate::models::{Claims, LoginResponse, ServiceError, User, UserCredentials};
use crate::utils::{jwt, password, user_storage, fs_utils};
use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

// Register a new user
#[post("/auth/register")]
async fn register(credentials: web::Json<UserCredentials>) -> Result<HttpResponse, ServiceError> {
    // Check if the email already exists
    if user_storage::find_user_by_email(&credentials.email)?.is_some() {
        return Err(ServiceError::BadRequest("Email already registered".to_string()));
    }

    // Create a new user
    let user = User {
        id: Uuid::new_v4().to_string(),
        email: credentials.email.clone(),
        password_hash: password::hash_password(&credentials.password)?,
        created_at: Utc::now(),
    };

    // Save the user
    user_storage::save_user(&user)?;

    // Create user storage directory
    fs_utils::ensure_user_directory(&user.id)
        .map_err(|_| ServiceError::InternalServerError)?;

    Ok(HttpResponse::Ok().json(json!({
        "message": "User registered successfully",
        "user_id": user.id
    })))
}

// Login and get JWT token
#[post("/auth/login")]
async fn login(credentials: web::Json<UserCredentials>) -> Result<HttpResponse, ServiceError> {
    // Find the user by email
    let user = match user_storage::find_user_by_email(&credentials.email)? {
        Some(user) => user,
        None => return Err(ServiceError::Unauthorized),
    };

    // Verify password
    if !password::verify_password(&credentials.password, &user.password_hash)? {
        return Err(ServiceError::Unauthorized);
    }

    // Generate JWT token
    let token = jwt::generate_token(&user)?;

    // Return token in headers as well as response body
    let response = LoginResponse {
        token: token.clone(),
        user_id: user.id,
        email: user.email,
    };

    Ok(HttpResponse::Ok()
        .append_header(("Authorization", format!("Bearer {}", token)))
        .json(response))
}

// Get current user info (requires authentication)
#[get("/auth/me")]
async fn me(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
    // Extract user claims from request extensions
    if let Some(claims) = req.extensions().get::<Claims>() {
        // Get user details from storage
        if let Some(user) = user_storage::find_user_by_id(&claims.sub)? {
            return Ok(HttpResponse::Ok().json(json!({
                "user_id": user.id,
                "email": user.email,
                "created_at": user.created_at
            })));
        }
    }

    Err(ServiceError::Unauthorized)
}

// Register all auth routes
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(register)
        .service(login)
        .service(me);
}