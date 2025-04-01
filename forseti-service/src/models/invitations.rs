// forseti-service/src/models/invitation.rs
use crate::models::TeamRole;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

// Status for team invitations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InvitationStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "declined")]
    Declined,
    #[serde(rename = "expired")]
    Expired,
}

// Team invitation model
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TeamInvitation {
    pub id: String,
    pub team_id: String,
    pub team_name: Option<String>, // Populated when retrieving
    pub invited_email: String,
    pub invited_by: String,
    pub invited_by_name: Option<String>, // Populated when retrieving
    pub role: TeamRole,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: InvitationStatus,
}

// Request to create a new invitation
#[derive(Serialize, Deserialize, Debug)]
pub struct CreateInvitationRequest {
    pub email: String,
    pub role: TeamRole,
}

// Response when updating invitation status
#[derive(Serialize, Deserialize, Debug)]
pub struct InvitationResponse {
    pub id: String,
    pub status: InvitationStatus,
    pub message: String,
}

impl TeamInvitation {
    // Create a new invitation with default values
    pub fn new(team_id: String, invited_email: String, invited_by: String, role: TeamRole) -> Self {
        let now = Utc::now();
        // Invitations expire after 7 days by default
        let expires_at = now + Duration::days(7);

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            team_id,
            team_name: None,
            invited_email,
            invited_by,
            invited_by_name: None,
            role,
            created_at: now,
            expires_at,
            status: InvitationStatus::Pending,
        }
    }

    // Check if invitation is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}