use serde::{Deserialize, Serialize};
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use log::{info, error};
use std::env;

// Claude API request structure
#[derive(Serialize, Deserialize)]
pub struct ClaudeRequest {
    pub prompt: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub debug: bool,
}

// Claude API response structure
#[derive(Serialize, Deserialize)]
pub struct ClaudeResponse {
    pub text: String,
}

// Internal Anthropic API response structure
#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum AnthropicResponse {
    // Claude 3 response format
    Claude3Response {
        content: Vec<Content>,
    },
    // Legacy response format
    LegacyResponse {
        completion: String,
    }
}

#[derive(Deserialize, Debug)]
struct Content {
    text: String,
    #[serde(rename = "type")]
    content_type: String,
}

// Error type for Claude API calls
#[derive(Debug, thiserror::Error)]
pub enum ClaudeApiError {
    #[error("API key not found")]
    ApiKeyNotFound,
    
    #[error("Request error: {0}")]
    RequestError(String),
    
    #[error("Response error: {0}")]
    ResponseError(String),
}

/// Call the Claude API with the given request
pub async fn call_claude_api(request: ClaudeRequest) -> Result<ClaudeResponse, ClaudeApiError> {
    // Get API key from environment variables
    let api_key = env::var("CLAUDE_API_KEY")
        .map_err(|_| ClaudeApiError::ApiKeyNotFound)?;
    
    // Creates request client
    let client = Client::new();
    
    // Creates request headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    
    // Add Anthropic API key as Bearer token
    headers.insert(
        AUTHORIZATION, 
        HeaderValue::from_str(&format!("Bearer {}", api_key))
            .map_err(|e| ClaudeApiError::RequestError(e.to_string()))?
    );
    headers.insert(
        "Anthropic-Version", 
        HeaderValue::from_static("2023-06-01")
    );
    headers.insert(
        "x-api-key", 
        HeaderValue::from_str(&api_key)
            .map_err(|e| ClaudeApiError::RequestError(e.to_string()))?
    );
    
    // Prepare Anthropic API payload based on model
    let payload = if request.model.starts_with("claude-3") {
        // Claude 3 API format
        let mut payload = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
            "messages": [
                {
                    "role": "user",
                    "content": request.prompt
                }
            ]
        });
        if let Some(system) = &request.system {
            info!("Adding system prompt: {}", system);
            payload["system"] = serde_json::Value::String(system.clone());
        }
        if request.debug {
            info!("Debug mode enabled for Claude API request");
        }
        
        payload
    } else {
        // Older Claude API format (requires Human/Assistant format)
        let formatted_prompt = format!("\n\nHuman: {}\n\nAssistant:", request.prompt);
        
        serde_json::json!({
            "model": request.model,
            "prompt": formatted_prompt,
            "max_tokens_to_sample": request.max_tokens,
            "temperature": request.temperature,
            "stop_sequences": ["\n\nHuman:"]
        })
    };
    
    info!("Making request to Claude API with model: {}", request.model);
    // Corrects the endpoint based on model used
    let api_endpoint = if request.model.starts_with("claude-3") {
        "https://api.anthropic.com/v1/messages"
    } else {
        "https://api.anthropic.com/v1/complete"
    };
    
    info!("Using API endpoint: {}", api_endpoint);
    
    let response = client.post(api_endpoint)
        .headers(headers)
        .json(&payload)
        .send()
        .await
        .map_err(|e| ClaudeApiError::RequestError(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response.text().await
            .unwrap_or_else(|_| "Could not read error response".to_string());
        error!("Claude API error: {}", error_text);
        return Err(ClaudeApiError::ResponseError(error_text));
    }
    
    // Gets the response text
    info!("Parsing Claude API response");
    let response_text = response.text().await
        .map_err(|e| ClaudeApiError::ResponseError(format!("Failed to read response: {}", e)))?;
    let preview = if response_text.len() > 100 {
        format!("{}...", &response_text[..100])
    } else {
        response_text.clone()
    };
    info!("Response preview: {}", preview);
    
    // Attempts to parse the response based on which endpoint was used
    let text = if api_endpoint.ends_with("/messages") {
        match serde_json::from_str::<AnthropicResponse>(&response_text) {
            Ok(AnthropicResponse::Claude3Response { content }) => {
                info!("Parsed Claude 3 response format");
                content.iter()
                    .find(|c| c.content_type == "text")
                    .map(|c| c.text.clone())
                    .unwrap_or_default()
            },
            Ok(AnthropicResponse::LegacyResponse { completion }) => {
                info!("Parsed legacy response format from messages API (unexpected)");
                completion
            },
            Err(e) => {
                error!("Failed to parse Claude 3 API response: {}", e);
                match serde_json::from_str::<serde_json::Value>(&response_text) {
                    Ok(value) => {
                        if let Some(content) = value.get("content") {
                            serde_json::to_string(content).unwrap_or_default()
                        } else {
                            response_text.clone()
                        }
                    },
                    Err(_) => response_text.clone()
                }
            }
        }
    } else {
        match serde_json::from_str::<serde_json::Value>(&response_text) {
            Ok(value) => {
                info!("Parsing direct completion response");
                value.get("completion")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            },
            Err(e) => {
                error!("Failed to parse Claude 2.x API response: {}", e);
                response_text.clone()
            }
        }
    };
    
    Ok(ClaudeResponse { text })
}