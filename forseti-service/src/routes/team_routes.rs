use crate::models::{Team, TeamMember, TeamRole, TeamData, ServiceError};
use crate::utils::{get_user_id_from_request, jwt, user_storage, team_storage, fs_utils};
use actix_web::{get, post, put, delete, web, HttpRequest, HttpResponse};
use chrono::Utc;
use log::{debug, error, info};
use serde_json::json;
use uuid::Uuid;

// Create a new team
#[post("/teams")]
async fn create_team(req: HttpRequest, team_data: web::Json<TeamData>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;

    info!("📝 Creating new team: {} for user: {}", team_data.name, user_id);

    // Create a new team with the user as owner
    let team_id = Uuid::new_v4().to_string();
    let team = Team {
        id: team_id.clone(),
        name: team_data.name.clone(),
        owner_id: user_id.clone(),
        created_at: Utc::now(),
    };

    // Save the team
    team_storage::save_team(&team)?;

    // Add user as team owner
    let team_member = TeamMember {
        user_id: user_id.clone(),
        team_id: team_id.clone(),
        role: TeamRole::Owner,
        access_expires: None,
    };

    team_storage::add_team_member(&team_member)?;

    // Create team directory
    fs_utils::ensure_team_directory(&team_id).map_err(|e| {
        error!("❌ Failed to create team directory: {:?}", e);
        ServiceError::InternalServerError
    })?;

    info!("✅ Team created successfully: {}", team_id);

    Ok(HttpResponse::Ok().json(team))
}

// Get all teams for the current user
#[get("/teams")]
async fn get_user_teams(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;

    info!("📋 Fetching teams for user: {}", user_id);

    let teams = team_storage::get_teams_for_user(&user_id)?;

    info!("✅ Found {} teams for user: {}", teams.len(), user_id);

    Ok(HttpResponse::Ok().json(teams))
}

// Get a specific team by ID
#[get("/teams/{team_id}")]
async fn get_team(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("🔍 Fetching team: {} for user: {}", team_id, user_id);

    // Check if user has access to this team
    if !team_storage::user_has_team_access(&user_id, &team_id)? {
        error!("❌ User: {} doesn't have access to team: {}", user_id, team_id);
        return Err(ServiceError::Forbidden);
    }

    // Get team details
    let team = match team_storage::find_team_by_id(&team_id)? {
        Some(team) => team,
        None => {
            error!("❌ Team not found: {}", team_id);
            return Err(ServiceError::NotFound);
        }
    };

    info!("✅ Found team: {}", team_id);

    Ok(HttpResponse::Ok().json(team))
}

// Add a user to a team
#[post("/teams/{team_id}/members")]
async fn add_team_member(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<TeamMember>,
) -> Result<HttpResponse, ServiceError> {
    let current_user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("👥 Adding user: {} to team: {}", data.user_id, team_id);

    // Check if current user has owner or contributor role
    if !team_storage::user_has_team_role(&current_user_id, &team_id, TeamRole::Contributor)? {
        error!("❌ User: {} doesn't have permission to add members to team: {}", current_user_id, team_id);
        return Err(ServiceError::Forbidden);
    }

    // Create team member
    let team_member = TeamMember {
        user_id: data.user_id.clone(),
        team_id: team_id.clone(),
        role: data.role.clone(),
        access_expires: data.access_expires,
    };

    // Save team member
    team_storage::add_team_member(&team_member)?;

    info!("✅ User: {} added to team: {} with role: {:?}", data.user_id, team_id, data.role);

    Ok(HttpResponse::Ok().json(team_member))
}

// Switch active team
#[post("/teams/{team_id}/activate")]
async fn activate_team(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("🔄 Activating team: {} for user: {}", team_id, user_id);

    // Check if user has access to this team
    if !team_storage::user_has_team_access(&user_id, &team_id)? {
        error!("❌ User: {} doesn't have access to team: {}", user_id, team_id);
        return Err(ServiceError::Forbidden);
    }

    // Get user to generate new token
    let user = match user_storage::find_user_by_id(&user_id)? {
        Some(user) => user,
        None => {
            error!("❌ User not found: {}", user_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Generate token with active team
    let token = jwt::generate_token(&user, Some(team_id.clone()))?;

    info!("✅ Team activated: {} for user: {}", team_id, user_id);

    Ok(HttpResponse::Ok()
        .append_header(("Authorization", format!("Bearer {}", token)))
        .json(json!({
            "message": "Team activated successfully",
            "token": token,
            "team_id": team_id
        })))
}

// Clear active team (switch to personal files)
#[post("/teams/deactivate")]
async fn deactivate_team(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;

    info!("🔄 Deactivating active team for user: {}", user_id);

    // Get user to generate new token
    let user = match user_storage::find_user_by_id(&user_id)? {
        Some(user) => user,
        None => {
            error!("❌ User not found: {}", user_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Generate token without active team
    let token = jwt::generate_token(&user, None)?;

    info!("✅ Team deactivated for user: {}", user_id);

    Ok(HttpResponse::Ok()
        .append_header(("Authorization", format!("Bearer {}", token)))
        .json(json!({
            "message": "Team deactivated successfully",
            "token": token
        })))
}

// Get team members
#[get("/teams/{team_id}/members")]
async fn get_team_members(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("📋 Fetching members for team: {}", team_id);

    // Check if user has access to this team
    if !team_storage::user_has_team_access(&user_id, &team_id)? {
        error!("❌ User: {} doesn't have access to team: {}", user_id, team_id);
        return Err(ServiceError::Forbidden);
    }

    // Logic to fetch team members would be implemented here
    // For simplicity, we'll just return a success message

    Ok(HttpResponse::Ok().json(json!({
        "message": "Team members feature will be implemented soon"
    })))
}

// Register all team routes
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(create_team)
        .service(get_user_teams)
        .service(get_team)
        .service(add_team_member)
        .service(activate_team)
        .service(deactivate_team)
        .service(get_team_members);
}