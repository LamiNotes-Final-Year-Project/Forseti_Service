use actix_web::{web, HttpResponse, Responder, HttpRequest};
use log::{info, error};
use crate::utils::claude_api::{ClaudeRequest, ClaudeResponse, call_claude_api};
use crate::utils::{get_user_id_from_request, UserContext};

// Configure Claude routes
pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/claude")
            .route(web::post().to(handle_claude_request))
    );
}

/// Handle Claude API request
async fn handle_claude_request(request: web::Json<ClaudeRequest>) -> impl Responder {
    info!("Received Claude API request");
    
    // Call Claude API
    match call_claude_api(request.into_inner()).await {
        Ok(response) => {
            info!("Claude API request successful");
            HttpResponse::Ok().json(response)
        },
        Err(err) => {
            error!("Claude API request failed: {:?}", err);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": format!("{}", err)
            }))
        }
    }
}