// forseti-service/src/utils/invitation_storage.rs
use crate::models::{InvitationStatus, ServiceError, TeamInvitation, TeamRole};
use crate::utils::{fs_utils, team_storage, user_storage};
use log::{debug, error, info, warn};
use std::fs;
use std::path::Path;

const INVITATIONS_DIR: &str = "./storage/invitations";

// Initialize invitations directory
pub fn ensure_invitations_dir() -> std::io::Result<()> {
    let dir = Path::new(INVITATIONS_DIR);
    if !dir.exists() {
        info!("Creating invitations directory");
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

// Save invitation to storage
pub fn save_invitation(invitation: &TeamInvitation) -> Result<(), ServiceError> {
    ensure_invitations_dir().map_err(|e| {
        error!("Failed to create invitations directory: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let invitation_path = format!("{}/{}.json", INVITATIONS_DIR, invitation.id);
    let invitation_json = serde_json::to_string_pretty(invitation).map_err(|e| {
        error!("Failed to serialize invitation: {:?}", e);
        ServiceError::InternalServerError
    })?;

    fs::write(&invitation_path, invitation_json).map_err(|e| {
        error!("Failed to save invitation: {:?}", e);
        ServiceError::InternalServerError
    })?;

    info!("✅ Saved invitation: {}", invitation.id);
    Ok(())
}

// Find invitation by ID
pub fn find_invitation_by_id(invitation_id: &str) -> Result<Option<TeamInvitation>, ServiceError> {
    let invitation_path = format!("{}/{}.json", INVITATIONS_DIR, invitation_id);
    let path = Path::new(&invitation_path);

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|e| {
        error!("Failed to read invitation file: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let invitation: TeamInvitation = serde_json::from_str(&content).map_err(|e| {
        error!("Failed to parse invitation JSON: {:?}", e);
        ServiceError::InternalServerError
    })?;

    Ok(Some(invitation))
}

// Delete invitation
pub fn delete_invitation(invitation_id: &str) -> Result<bool, ServiceError> {
    let invitation_path = format!("{}/{}.json", INVITATIONS_DIR, invitation_id);
    let path = Path::new(&invitation_path);

    if !path.exists() {
        return Ok(false);
    }

    fs::remove_file(path).map_err(|e| {
        error!("Failed to delete invitation file: {:?}", e);
        ServiceError::InternalServerError
    })?;

    info!("✅ Deleted invitation: {}", invitation_id);
    Ok(true)
}

// Get all invitations for a user by email
pub fn get_invitations_for_email(email: &str) -> Result<Vec<TeamInvitation>, ServiceError> {
    let mut invitations = Vec::new();
    ensure_invitations_dir().map_err(|e| {
        error!("Failed to ensure invitations directory: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let dir = Path::new(INVITATIONS_DIR);

    // Check if directory exists first
    if !dir.exists() {
        return Ok(Vec::new());
    }

    for entry_result in fs::read_dir(dir).map_err(|e| {
        error!("Failed to read invitations directory: {:?}", e);
        ServiceError::InternalServerError
    })? {
        let entry = entry_result.map_err(|e| {
            error!("Failed to read directory entry: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
            let content = fs::read_to_string(&path).map_err(|e| {
                error!("Failed to read invitation file: {:?}", e);
                ServiceError::InternalServerError
            })?;

            let invitation: TeamInvitation = match serde_json::from_str(&content) {
                Ok(inv) => inv,
                Err(e) => {
                    warn!("Failed to parse invitation JSON: {:?}", e);
                    continue;
                }
            };

            // Add invitation if it matches the email and is not expired
            if invitation.invited_email.to_lowercase() == email.to_lowercase() {
                // Check if invitation has expired
                if invitation.is_expired() && invitation.status == InvitationStatus::Pending {
                    // Update status to expired
                    let mut updated = invitation.clone();
                    updated.status = InvitationStatus::Expired;
                    save_invitation(&updated)?;
                    invitations.push(updated);
                } else {
                    invitations.push(invitation);
                }
            }
        }
    }

    Ok(invitations)
}

// Get all invitations for a team
pub fn get_invitations_for_team(team_id: &str) -> Result<Vec<TeamInvitation>, ServiceError> {
    let mut invitations = Vec::new();
    ensure_invitations_dir().map_err(|e| {
        error!("Failed to ensure invitations directory: {:?}", e);
        ServiceError::InternalServerError
    })?;

    let dir = Path::new(INVITATIONS_DIR);

    // Check if directory exists first
    if !dir.exists() {
        return Ok(Vec::new());
    }

    for entry_result in fs::read_dir(dir).map_err(|e| {
        error!("Failed to read invitations directory: {:?}", e);
        ServiceError::InternalServerError
    })? {
        let entry = entry_result.map_err(|e| {
            error!("Failed to read directory entry: {:?}", e);
            ServiceError::InternalServerError
        })?;

        let path = entry.path();
        if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
            let content = fs::read_to_string(&path).map_err(|e| {
                error!("Failed to read invitation file: {:?}", e);
                ServiceError::InternalServerError
            })?;

            let invitation: TeamInvitation = match serde_json::from_str(&content) {
                Ok(inv) => inv,
                Err(e) => {
                    warn!("Failed to parse invitation JSON: {:?}", e);
                    continue;
                }
            };

            // Add invitation if it matches the team
            if invitation.team_id == team_id {
                // Check if invitation has expired
                if invitation.is_expired() && invitation.status == InvitationStatus::Pending {
                    // Update status to expired
                    let mut updated = invitation.clone();
                    updated.status = InvitationStatus::Expired;
                    save_invitation(&updated)?;
                    invitations.push(updated);
                } else {
                    invitations.push(invitation);
                }
            }
        }
    }

    Ok(invitations)
}

// Update invitation status
pub fn update_invitation_status(
    invitation_id: &str,
    status: InvitationStatus,
) -> Result<TeamInvitation, ServiceError> {
    let invitation = match find_invitation_by_id(invitation_id)? {
        Some(inv) => inv,
        None => return Err(ServiceError::NotFound),
    };

    // Don't update if already accepted or declined
    if invitation.status != InvitationStatus::Pending {
        return Err(ServiceError::BadRequest(format!(
            "Invitation is already {}",
            match invitation.status {
                InvitationStatus::Accepted => "accepted",
                InvitationStatus::Declined => "declined",
                InvitationStatus::Expired => "expired",
                _ => "processed",
            }
        )));
    }

    // Check if invitation has expired
    if invitation.is_expired() {
        let mut expired = invitation.clone();
        expired.status = InvitationStatus::Expired;
        save_invitation(&expired)?;
        return Err(ServiceError::BadRequest("Invitation has expired".to_string()));
    }

    // Update the invitation status
    let mut updated = invitation.clone();
    updated.status = status;
    save_invitation(&updated)?;

    Ok(updated)
}

// Helper function to enrich invitation with team and user names
pub fn enrich_invitation(invitation: &mut TeamInvitation) -> Result<(), ServiceError> {
    // Add team name
    if let Some(team) = team_storage::find_team_by_id(&invitation.team_id)? {
        invitation.team_name = Some(team.name);
    }

    // Add inviter name (use email if user not found)
    if let Some(user) = user_storage::find_user_by_id(&invitation.invited_by)? {
        invitation.invited_by_name = Some(user.email.clone());
    }

    Ok(())
}

// Delete all invitations for a team
pub fn delete_team_invitations(team_id: &str) -> Result<usize, ServiceError> {
    let invitations = get_invitations_for_team(team_id)?;
    let mut deleted_count = 0;
    
    for invitation in invitations {
        if delete_invitation(&invitation.id)? {
            deleted_count += 1;
        }
    }
    
    info!("✅ Deleted {} invitations for team: {}", deleted_count, team_id);
    Ok(deleted_count)
}