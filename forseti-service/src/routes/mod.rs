// src/routes/mod.rs
pub mod file_routes;
pub mod auth_routes;
pub mod team_routes;
pub mod version_routes;
pub mod invitation_routes;

// Re-export the file_lock module from utils for easier access to lock_routes
pub mod file_lock {
    pub use crate::utils::file_lock::lock_routes;
}