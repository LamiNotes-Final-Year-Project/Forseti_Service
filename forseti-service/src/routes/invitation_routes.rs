// forseti-service/src/routes/invitation_routes.rs
use crate::models::{CreateInvitationRequest, InvitationStatus, ServiceError, TeamInvitation, TeamRole};
use crate::utils::{get_user_id_from_request, invitation_storage, team_storage, user_storage};
use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use log::{debug, error, info};
use serde_json::json;

// Create a new team invitation
#[post("/teams/{team_id}/invitations")]
async fn create_invitation(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<CreateInvitationRequest>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("📧 Creating invitation to team: {} for email: {}", team_id, data.email);

    // Verify the team exists
    let team = match team_storage::find_team_by_id(&team_id)? {
        Some(team) => team,
        None => {
            error!("❌ Team not found: {}", team_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Check if user has permission to invite (must be Owner or Contributor)
    if !team_storage::user_has_team_role(&user_id, &team_id, TeamRole::Contributor)? {
        error!("❌ User does not have permission to invite to team: {}", team_id);
        return Err(ServiceError::Forbidden);
    }

    // Check if user being invited already exists
    let invited_user = user_storage::find_user_by_email(&data.email)?;

    // Check if user is already a member of the team
    if let Some(user) = &invited_user {
        if team_storage::user_has_team_access(&user.id, &team_id)? {
            return Err(ServiceError::BadRequest(format!(
                "User is already a member of the team"
            )));
        }
    }

    // Check if there's already a pending invitation for this email and team
    let existing_invitations = invitation_storage::get_invitations_for_email(&data.email)?;
    for invitation in &existing_invitations {
        if invitation.team_id == team_id && invitation.status == InvitationStatus::Pending {
            return Err(ServiceError::BadRequest(
                "An invitation for this user to this team already exists".to_string()
            ));
        }
    }

    // Create the invitation - make sure to clone strings to avoid ownership issues
    let invitation = TeamInvitation::new(
        team_id.clone(),      // Clone to avoid moving ownership
        data.email.clone(),   // Already cloned
        user_id.clone(),      // Clone to avoid moving ownership
        data.role.clone()            // This should implement Copy, so no need to clone
    );

    // Save the invitation
    invitation_storage::save_invitation(&invitation)?;

    info!("✅ Invitation created: {}", invitation.id);

    // Return the invitation
    Ok(HttpResponse::Ok().json(invitation))
}
// Get all invitations for the current user
#[get("/invitations")]
async fn get_user_invitations(req: HttpRequest) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;

    info!("📋 Fetching invitations for user: {}", user_id);

    // Get user's email
    let user = match user_storage::find_user_by_id(&user_id)? {
        Some(user) => user,
        None => {
            error!("❌ User not found: {}", user_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Get all invitations for this email
    let mut invitations = invitation_storage::get_invitations_for_email(&user.email)?;

    // Enrich invitations with team and user names
    for invitation in &mut invitations {
        invitation_storage::enrich_invitation(invitation)?;
    }

    info!("✅ Found {} invitations for user", invitations.len());

    Ok(HttpResponse::Ok().json(invitations))
}

// Get all invitations for a team
#[get("/teams/{team_id}/invitations")]
async fn get_team_invitations(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let team_id = path.into_inner();

    info!("📋 Fetching invitations for team: {}", team_id);

    // Check if user has access to this team
    if !team_storage::user_has_team_access(&user_id, &team_id)? {
        error!("❌ User does not have access to team: {}", team_id);
        return Err(ServiceError::Forbidden);
    }

    // Get all invitations for this team
    let mut invitations = invitation_storage::get_invitations_for_team(&team_id)?;

    // Enrich invitations with team and user names
    for invitation in &mut invitations {
        invitation_storage::enrich_invitation(invitation)?;
    }

    info!("✅ Found {} invitations for team", invitations.len());

    Ok(HttpResponse::Ok().json(invitations))
}

// Respond to an invitation (accept/decline)
#[put("/invitations/{invitation_id}")]
async fn respond_to_invitation(
    req: HttpRequest,
    path: web::Path<String>,
    data: web::Json<serde_json::Value>,
) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let invitation_id = path.into_inner();

    // Parse the status from the request
    let status_str = match data.get("status") {
        Some(serde_json::Value::String(s)) => s.to_lowercase(),
        _ => {
            return Err(ServiceError::BadRequest(
                "Invalid or missing 'status' field".to_string(),
            ))
        }
    };

    let status = match status_str.as_str() {
        "accepted" => InvitationStatus::Accepted,
        "declined" => InvitationStatus::Declined,
        _ => {
            return Err(ServiceError::BadRequest(format!(
                "Invalid status: {}. Must be 'accepted' or 'declined'",
                status_str
            )))
        }
    };

    info!(
        "🔄 Responding to invitation {}: {:?}",
        invitation_id, status
    );

    // Get the invitation
    let invitation = match invitation_storage::find_invitation_by_id(&invitation_id)? {
        Some(inv) => inv,
        None => {
            error!("❌ Invitation not found: {}", invitation_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Get the user's email
    let user = match user_storage::find_user_by_id(&user_id)? {
        Some(user) => user,
        None => {
            error!("❌ User not found: {}", user_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Verify this invitation belongs to the user
    if invitation.invited_email.to_lowercase() != user.email.to_lowercase() {
        error!("❌ Invitation is not for this user");
        return Err(ServiceError::Forbidden);
    }

    // Update the invitation status
    let updated_invitation = invitation_storage::update_invitation_status(&invitation_id, status.clone())?;

    // If accepted, add the user to the team
    if status == InvitationStatus::Accepted {
        // Add the user to the team with the specified role
        let team_member = crate::models::TeamMember {
            user_id: user_id.clone(),
            team_id: invitation.team_id.clone(),
            role: invitation.role,
            access_expires: None,
        };

        team_storage::add_team_member(&team_member)?;

        info!("✅ User added to team: {}", invitation.team_id);
    }

    Ok(HttpResponse::Ok().json(json!({
        "id": invitation_id,
        "status": status_str,
        "message": match status {
            InvitationStatus::Accepted => "Invitation accepted successfully",
            InvitationStatus::Declined => "Invitation declined",
            _ => "Invitation status updated",
        }
    })))
}

// Cancel (delete) an invitation
#[delete("/invitations/{invitation_id}")]
async fn delete_invitation(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse, ServiceError> {
    let user_id = get_user_id_from_request(&req)?;
    let invitation_id = path.into_inner();

    info!("🗑️ Deleting invitation: {}", invitation_id);

    // Get the invitation
    let invitation = match invitation_storage::find_invitation_by_id(&invitation_id)? {
        Some(inv) => inv,
        None => {
            error!("❌ Invitation not found: {}", invitation_id);
            return Err(ServiceError::NotFound);
        }
    };

    // Check if user has permission to delete (must be the inviter or a team owner)
    let is_inviter = invitation.invited_by == user_id;
    let is_team_owner = team_storage::user_has_team_role(&user_id, &invitation.team_id, TeamRole::Owner)?;

    if !is_inviter && !is_team_owner {
        error!("❌ User does not have permission to delete this invitation");
        return Err(ServiceError::Forbidden);
    }

    // Delete the invitation
    invitation_storage::delete_invitation(&invitation_id)?;

    Ok(HttpResponse::Ok().json(json!({
        "message": "Invitation deleted successfully"
    })))
}

// Register all invitation routes
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(create_invitation)
        .service(get_user_invitations)
        .service(get_team_invitations)
        .service(respond_to_invitation)
        .service(delete_invitation);
}