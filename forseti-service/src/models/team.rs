use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum TeamRole {
    Viewer = 0,
    Contributor = 1,
    Owner = 2,
}

#[derive(Serialize, Deserialize)]
pub struct TeamMember {
    pub user_id: String,
    pub team_id: String,
    pub role: TeamRole,
    pub access_expires: Option<DateTime<Utc>>, // For viewers
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TeamData {
    pub name: String,
}