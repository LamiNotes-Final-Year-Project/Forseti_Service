use crate::models::{Team, TeamMember, TeamRole, TeamData, ServiceError};
use crate::utils::{get_user_id_from_request, jwt, user_storage, team_storage, fs_utils, invitation_storage};
use actix_web::{get, post, put, delete, web, HttpRequest, HttpResponse};
use chrono::Utc;
use log::{error, info};
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

    // Get team members from storage
    let members = team_storage::get_team_members(&team_id)?;
    
    info!("✅ Found {} team members", members.len());

    Ok(HttpResponse::Ok().json(members))
}

// Get current user's role in a team
#[get("/teams/{team_id}/members/role")]
async fn get_user_role_in_team(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("🔍 Fetching role for user: {} in team: {}", user_id, team_id);

    // Check if user has access to this team
    if !team_storage::user_has_team_access(&user_id, &team_id)? {
        error!("❌ User: {} doesn't have access to team: {}", user_id, team_id);
        return Err(ServiceError::Forbidden);
    }

    // Get user's role
    let role = team_storage::get_user_role_in_team(&user_id, &team_id)?;
    
    info!("✅ User role found: {:?}", role);

    Ok(HttpResponse::Ok().json(json!({ "role": role })))
}

// Get user by ID (for team member display)
#[get("/users/{user_id}")]
async fn get_user_by_id(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let current_user_id = get_user_id_from_request(&req)?;
    let target_user_id = path.into_inner();

    info!("🔍 Fetching user: {}", target_user_id);

    // Get the user
    let user = match user_storage::find_user_by_id(&target_user_id)? {
        Some(user) => user,
        None => {
            error!("❌ User not found: {}", target_user_id);
            return Err(ServiceError::NotFound);
        }
    };

    // For privacy, we'll only return minimal information
    // unless it's the user themselves
    let user_data = if current_user_id == target_user_id {
        json!({
            "user_id": user.id,
            "email": user.email,
            "display_name": user.email.split('@').next().unwrap_or(&user.email)
        })
    } else {
        json!({
            "user_id": user.id,
            "email": user.email,
            "display_name": user.email.split('@').next().unwrap_or(&user.email)
        })
    };

    Ok(HttpResponse::Ok().json(user_data))
}

// Update a team member's role
#[put("/teams/{team_id}/members/{user_id}")]
async fn update_team_member_role(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    data: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ServiceError> {
    let current_user_id = get_user_id_from_request(&req)?;
    let (team_id, target_user_id) = path.into_inner();

    info!("🔄 Updating role for user: {} in team: {}", target_user_id, team_id);

    // Check if current user has owner role (required to change roles)
    if !team_storage::user_has_team_role(&current_user_id, &team_id, TeamRole::Owner)? {
        error!("❌ Only team owners can update member roles");
        return Err(ServiceError::Forbidden);
    }

    // Parse the role from the request
    let role = match data.get("role") {
        Some(serde_json::Value::Number(n)) => {
            let role_num = n.as_i64().unwrap_or(0);
            match role_num {
                0 => TeamRole::Viewer,
                1 => TeamRole::Contributor,
                2 => TeamRole::Owner,
                _ => {
                    return Err(ServiceError::BadRequest(
                        "Invalid role value. Must be 0 (Viewer), 1 (Contributor), or 2 (Owner)".to_string(),
                    ))
                }
            }
        },
        _ => {
            return Err(ServiceError::BadRequest(
                "Invalid or missing 'role' field".to_string(),
            ))
        }
    };

    // Check if the target user is the team owner (can't change owner's role)
    let team = match team_storage::find_team_by_id(&team_id)? {
        Some(team) => team,
        None => {
            error!("❌ Team not found: {}", team_id);
            return Err(ServiceError::NotFound);
        }
    };

    if target_user_id == team.owner_id {
        return Err(ServiceError::BadRequest(
            "Cannot change the team owner's role".to_string(),
        ));
    }

    // Update the team member's role
    team_storage::update_team_member_role(&target_user_id, &team_id, role)?;

    Ok(HttpResponse::Ok().json(json!({
        "message": format!("User role updated to: {:?}", role),
        "user_id": target_user_id,
        "team_id": team_id,
        "role": role
    })))
}

// Remove a member from a team
#[delete("/teams/{team_id}/members/{user_id}")]
async fn remove_team_member(
    req: HttpRequest,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, ServiceError> {
    let current_user_id = get_user_id_from_request(&req)?;
    let (team_id, target_user_id) = path.into_inner();

    info!("🗑️ Removing user: {} from team: {}", target_user_id, team_id);

    // Get the team
    let team = match team_storage::find_team_by_id(&team_id)? {
        Some(team) => team,
        None => {
            error!("❌ Team not found: {}", team_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Cannot remove team owner
    if target_user_id == team.owner_id {
        return Err(ServiceError::BadRequest(
            "Cannot remove the team owner from the team".to_string(),
        ));
    }

    // Users can remove themselves, or owners can remove anyone
    let is_self_removal = current_user_id == target_user_id;
    let is_owner = team_storage::user_has_team_role(&current_user_id, &team_id, TeamRole::Owner)?;

    if !is_self_removal && !is_owner {
        error!("❌ Only team owners can remove other members");
        return Err(ServiceError::Forbidden);
    }

    // Remove the team member
    team_storage::remove_team_member(&target_user_id, &team_id)?;

    Ok(HttpResponse::Ok().json(json!({
        "message": "User removed from team successfully",
        "user_id": target_user_id,
        "team_id": team_id
    })))
}

// Delete a team
#[delete("/teams/{team_id}")]
async fn delete_team(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("🗑️ Deleting team: {}", team_id);

    // Get the team
    let team = match team_storage::find_team_by_id(&team_id)? {
        Some(team) => team,
        None => {
            error!("❌ Team not found: {}", team_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Only the team owner can delete a team
    if team.owner_id != user_id {
        error!("❌ Only the team owner can delete the team");
        return Err(ServiceError::Forbidden);
    }

    // Delete all team members
    team_storage::delete_team_members(&team_id)?;
    
    // Delete all team files
    fs_utils::delete_team_files(&team_id).map_err(|e| {
        error!("❌ Failed to delete team files: {:?}", e);
        ServiceError::InternalServerError
    })?;
    
    // Delete all team invitations
    if let Err(err) = invitation_storage::delete_team_invitations(&team_id) {
        error!("❌ Failed to delete team invitations: {}", err);
        // Continue with deletion even if invitations deletion fails
    }
    
    // Delete the team
    team_storage::delete_team(&team_id)?;

    Ok(HttpResponse::Ok().json(json!({
        "message": "Team deleted successfully",
        "team_id": team_id
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
        .service(get_team_members)
        .service(get_user_role_in_team)
        .service(get_user_by_id)
        .service(update_team_member_role)
        .service(remove_team_member)
        .service(delete_team);
}