// src/routes/mod.rs
pub mod file_routes;
pub mod auth_routes;
pub mod team_routes;
pub mod version_routes;


pub use crate::utils::file_lock::lock_routes;