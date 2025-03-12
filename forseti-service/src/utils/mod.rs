use crate::models::{Claims, ServiceError, User};
use actix_web::http::header;
use actix_web::{dev::ServiceRequest, Error, HttpMessage, HttpRequest};
use bcrypt::{hash, verify, DEFAULT_COST};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use log::{debug, error, info, warn};
use std::env;
use std::fs;
use std::path::Path;
use uuid::Uuid;

// Import the version control module
pub mod version_control;
pub(crate) mod file_lock;

// UserContext for storing user information in request extensions
#[derive(Debug, Clone)]
pub struct UserContext {
    pub user_id: String,
    pub active_team_id: Option<String>,
}

// Helper function to get user_id from request
pub fn get_user_id_from_request(req: &HttpRequest) -> Result<String, ServiceError> {
    if let Some(user_ctx) = req.extensions().get::<UserContext>() {
        Ok(user_ctx.user_id.clone())
    } else if let Some(claims) = req.extensions().get::<Claims>() {
        Ok(claims.sub.clone())
    } else {
        Err(ServiceError::Unauthorized)
    }
}

// Helper function to get active team from request
pub fn get_active_team_from_request(req: &HttpRequest) -> Option<String> {
    if let Some(user_ctx) = req.extensions().get::<UserContext>() {
        user_ctx.active_team_id.clone()
    } else if let Some(claims) = req.extensions().get::<Claims>() {
        claims.active_team_id.clone()
    } else {
        None
    }
}

// Helper function to get username from email
pub fn get_username_from_email(email: &str) -> String {
    email.split('@').next().unwrap_or("user").to_string()
}

// JWT utility functions
pub mod jwt {
    use super::*;

    // Get JWT secret from environment or use default
    fn get_jwt_secret() -> String {
        env::var("JWT_SECRET").unwrap_or_else(|_| "laminotes_super_secret_key".to_string())
    }

    // Generate a new JWT token for a user
    pub fn generate_token(user: &User, active_team_id: Option<String>) -> Result<String, ServiceError> {
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
            active_team_id,
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
            .map_err(|e| {
                warn!("Token validation error: {:?}", e);
                ServiceError::Unauthorized
            })
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
        // Ensure users directory exists
        if !Path::new(USERS_DIR).exists() {
            fs::create_dir_all(USERS_DIR).map_err(|e| {
                error!("Failed to create users directory: {:?}", e);
                ServiceError::InternalServerError
            })?;
        }

        let user_path = format!("{}/{}.json", USERS_DIR, user.id);

        // Save user data as JSON
        fs::write(
            &user_path,
            serde_json::to_string(&user).map_err(|e| {
                error!("Failed to serialize user: {:?}", e);
                ServiceError::InternalServerError
            })?,
        )
            .map_err(|e| {
                error!("Failed to write user file: {:?}", e);
                ServiceError::InternalServerError
            })
    }

    // Find a user by email
    pub fn find_user_by_email(email: &str) -> Result<Option<User>, ServiceError> {
        let users_dir = Path::new(USERS_DIR);

        if !users_dir.exists() {
            fs::create_dir_all(users_dir).map_err(|e| {
                error!("Failed to create users directory: {:?}", e);
                ServiceError::InternalServerError
            })?;
            return Ok(None);
        }

        for entry in fs::read_dir(users_dir).map_err(|e| {
            error!("Failed to read users directory: {:?}", e);
            ServiceError::InternalServerError
        })? {
            let entry = entry.map_err(|e| {
                error!("Failed to read directory entry: {:?}", e);
                ServiceError::InternalServerError
            })?;
            let path = entry.path();

            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                let content = fs::read_to_string(&path).map_err(|e| {
                    error!("Failed to read user file: {:?}", e);
                    ServiceError::InternalServerError
                })?;

                let user: User = match serde_json::from_str(&content) {
                    Ok(user) => user,
                    Err(e) => {
                        warn!("Failed to parse user JSON: {:?}", e);
                        continue;
                    }
                };

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

        let content = fs::read_to_string(path).map_err(|e| {
            error!("Failed to read user file: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let user: User = serde_json::from_str(&content).map_err(|e| {
            error!("Failed to parse user JSON: {:?}", e);
            ServiceError::InternalServerError
        })?;

        Ok(Some(user))
    }
}

// Team storage utilities
pub mod team_storage {
    use super::*;
    use std::collections::HashMap;
    use crate::models::{Team, TeamMember, TeamRole};

    const TEAMS_DIR: &str = "./storage/teams";
    const TEAM_MEMBERS_DIR: &str = "./storage/team_members";

    // Get the path for a team's file storage
    pub fn get_team_storage_path(team_id: &str) -> String {
        format!("{}/{}", TEAMS_DIR, team_id)
    }

    // Save a team to storage
    pub fn save_team(team: &Team) -> Result<(), ServiceError> {
        // Ensure teams directory exists
        if !Path::new(TEAMS_DIR).exists() {
            fs::create_dir_all(TEAMS_DIR).map_err(|e| {
                error!("Failed to create teams directory: {:?}", e);
                ServiceError::InternalServerError
            })?;
        }

        let team_path = format!("{}/{}.json", TEAMS_DIR, team.id);

        // Save team data as JSON
        fs::write(
            &team_path,
            serde_json::to_string(&team).map_err(|e| {
                error!("Failed to serialize team: {:?}", e);
                ServiceError::InternalServerError
            })?,
        )
            .map_err(|e| {
                error!("Failed to write team file: {:?}", e);
                ServiceError::InternalServerError
            })
    }

    // Add a member to a team
    pub fn add_team_member(team_member: &TeamMember) -> Result<(), ServiceError> {
        // Ensure team members directory exists
        if !Path::new(TEAM_MEMBERS_DIR).exists() {
            fs::create_dir_all(TEAM_MEMBERS_DIR).map_err(|e| {
                error!("Failed to create team members directory: {:?}", e);
                ServiceError::InternalServerError
            })?;
        }

        let member_path = format!("{}/{}_{}.json", TEAM_MEMBERS_DIR, team_member.team_id, team_member.user_id);

        // Save team member data as JSON
        fs::write(
            &member_path,
            serde_json::to_string(&team_member).map_err(|e| {
                error!("Failed to serialize team member: {:?}", e);
                ServiceError::InternalServerError
            })?,
        )
            .map_err(|e| {
                error!("Failed to write team member file: {:?}", e);
                ServiceError::InternalServerError
            })
    }

    // Get all teams for a user
    pub fn get_teams_for_user(user_id: &str) -> Result<Vec<Team>, ServiceError> {
        let mut teams = Vec::new();
        let team_members_dir = Path::new(TEAM_MEMBERS_DIR);

        if !team_members_dir.exists() {
            return Ok(teams);
        }

        // Find all team memberships for this user
        for entry in fs::read_dir(team_members_dir).map_err(|e| {
            error!("Failed to read team members directory: {:?}", e);
            ServiceError::InternalServerError
        })? {
            let entry = entry.map_err(|e| {
                error!("Failed to read directory entry: {:?}", e);
                ServiceError::InternalServerError
            })?;
            let path = entry.path();
            let filename = path.file_name().unwrap().to_string_lossy();

            // Check if this is a file for the current user
            if path.is_file() && filename.contains(&format!("_{}", user_id)) {
                let content = fs::read_to_string(&path).map_err(|e| {
                    error!("Failed to read team member file: {:?}", e);
                    ServiceError::InternalServerError
                })?;

                let team_member: TeamMember = match serde_json::from_str(&content) {
                    Ok(tm) => tm,
                    Err(e) => {
                        warn!("Failed to parse team member JSON: {:?}", e);
                        continue;
                    }
                };

                // Find the team details
                if let Some(team) = find_team_by_id(&team_member.team_id)? {
                    teams.push(team);
                }
            }
        }

        Ok(teams)
    }

    // Find a team by ID
    pub fn find_team_by_id(team_id: &str) -> Result<Option<Team>, ServiceError> {
        let team_path = format!("{}/{}.json", TEAMS_DIR, team_id);
        let path = Path::new(&team_path);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).map_err(|e| {
            error!("Failed to read team file: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let team: Team = serde_json::from_str(&content).map_err(|e| {
            error!("Failed to parse team JSON: {:?}", e);
            ServiceError::InternalServerError
        })?;

        Ok(Some(team))
    }

    // Get a user's role in a team
    pub fn get_user_role_in_team(user_id: &str, team_id: &str) -> Result<Option<TeamRole>, ServiceError> {
        let member_path = format!("{}/{}_{}.json", TEAM_MEMBERS_DIR, team_id, user_id);
        let path = Path::new(&member_path);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).map_err(|e| {
            error!("Failed to read team member file: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let team_member: TeamMember = serde_json::from_str(&content).map_err(|e| {
            error!("Failed to parse team member JSON: {:?}", e);
            ServiceError::InternalServerError
        })?;

        Ok(Some(team_member.role))
    }

    // Check if a user has access to a team
    pub fn user_has_team_access(user_id: &str, team_id: &str) -> Result<bool, ServiceError> {
        match get_user_role_in_team(user_id, team_id)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    // Check if a user has a specific role (or higher) in a team
    pub fn user_has_team_role(user_id: &str, team_id: &str, required_role: TeamRole) -> Result<bool, ServiceError> {
        match get_user_role_in_team(user_id, team_id)? {
            Some(role) => Ok(role >= required_role),
            None => Ok(false),
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
            info!("Creating directory for user: {}", user_id);
            fs::create_dir_all(&user_dir)?;
        }
        Ok(())
    }

    // Ensure a team's storage directory exists
    pub fn ensure_team_directory(team_id: &str) -> io::Result<()> {
        let team_dir = format!("./storage/teams/{}", team_id);
        if !Path::new(&team_dir).exists() {
            info!("Creating directory for team: {}", team_id);
            fs::create_dir_all(&team_dir)?;
        }
        Ok(())
    }

    // List all files in a user's directory
    pub fn list_user_files(user_id: &str) -> io::Result<Vec<String>> {
        let user_dir = format!("./storage/{}", user_id);
        list_files_in_directory(&user_dir)
    }

    // List all files in a team's directory
    pub fn list_team_files(team_id: &str) -> io::Result<Vec<String>> {
        let team_dir = format!("./storage/teams/{}", team_id);
        list_files_in_directory(&team_dir)
    }

    // Helper function to list files in a directory
    fn list_files_in_directory(dir_path: &str) -> io::Result<Vec<String>> {
        let path = Path::new(dir_path);

        if !path.exists() {
            info!("Creating directory: {}", dir_path);
            fs::create_dir_all(path)?;
            return Ok(Vec::new());
        }

        let mut file_names = Vec::new();

        for entry in fs::read_dir(path)? {
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

// Initialize version control storage
pub fn initialize_version_control() -> std::io::Result<()> {
    // Ensure version control directories exist
    version_control::version_storage::ensure_version_storage()
}

// Middleware for JWT authentication
use actix_web::{
    dev::{forward_ready, Service, ServiceResponse, Transform},
    Error as ActixError,
};
use futures::future::{ok, ready, Ready};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// Authentication middleware
pub struct Auth;

impl<S, B> Transform<S, ServiceRequest> for Auth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = ActixError;
    type Transform = AuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddleware { service }))
    }
}

pub struct AuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
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
        // Extract the path for logging
        let path = req.path().to_owned();
        info!("🔒 Auth middleware processing request: {}", path);

        // Check for auth header
        let auth_header = req.headers().get(header::AUTHORIZATION);
        let mut user_id = "public".to_string(); // Default to public
        let mut active_team_id = None;
        let mut authenticated = false;

        if let Some(auth_value) = auth_header {
            if let Ok(auth_str) = auth_value.to_str() {
                info!("🔑 Found Authorization header: {}", auth_str);
                if auth_str.starts_with("Bearer ") {
                    let token = auth_str.trim_start_matches("Bearer ");
                    match jwt::decode_token(token) {
                        Ok(claims) => {
                            info!("✅ Valid token for user_id: {}", claims.sub);
                            user_id = claims.sub.clone();
                            active_team_id = claims.active_team_id.clone();
                            authenticated = true;
                            // Insert claims into request extensions
                            req.extensions_mut().insert(claims);
                        }
                        Err(e) => {
                            warn!("⚠️ Invalid token: {:?}, using public user", e);
                            // Fall back to public
                        }
                    }
                } else {
                    debug!("Auth header does not start with 'Bearer ': {}", auth_str);
                }
            } else {
                debug!("Could not convert auth header to string");
            }
        } else {
            debug!("ℹ️ No auth header found, using public user");
        }

        // Insert UserContext with the determined user_id and team
        info!("🔑 Request will use user_id: {} (authenticated: {}, team: {:?})",
             user_id, authenticated, active_team_id);

        req.extensions_mut().insert(UserContext {
            user_id: user_id.clone(),
            active_team_id: active_team_id.clone()
        });

        // Ensure the user's directory exists
        if let Err(e) = fs_utils::ensure_user_directory(&user_id) {
            error!("❌ Failed to ensure user directory: {:?}", e);
        }

        // If there's an active team, ensure its directory exists
        if let Some(team_id) = &active_team_id {
            if let Err(e) = fs_utils::ensure_team_directory(team_id) {
                error!("❌ Failed to ensure team directory: {:?}", e);
            }
        }

        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            Ok(res)
        })
    }
}