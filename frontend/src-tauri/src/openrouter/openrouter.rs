use serde::{Deserialize, Serialize};
use tauri::command;
use reqwest::Client;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    pub context_length: Option<u32>,
    pub prompt_price: Option<String>,
    pub completion_price: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterApiModel {
    id: String,
    name: Option<String>,
    context_length: Option<u32>,
    #[serde(default)]
    top_provider: Option<TopProvider>,
    #[serde(default)]
    pricing: Option<Pricing>,
}

#[derive(Debug, Deserialize, Default)]
struct TopProvider {
    context_length: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct Pricing {
    prompt: Option<String>,
    completion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    data: Vec<OpenRouterApiModel>,
}

#[command]
pub async fn get_openrouter_models() -> Result<Vec<OpenRouterModel>, String> {
    // Fail fast when offline, but keep the old 30s total budget (reqwest's
    // blocking default): the total timeout also covers downloading the large
    // model list body.
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let response = client
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await
        .map_err(|e| format!("Failed to make HTTP request: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP request failed with status: {}", response.status()));
    }

    let api_response: OpenRouterResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    Ok(map_models(api_response))
}

fn map_models(api_response: OpenRouterResponse) -> Vec<OpenRouterModel> {
    api_response
        .data
        .into_iter()
        .map(|m| OpenRouterModel {
            id: m.id,
            name: m.name.unwrap_or_else(|| "Unknown".to_string()),
            context_length: m.top_provider
                .as_ref()
                .and_then(|tp| tp.context_length)
                .or(m.context_length),
            prompt_price: m.pricing.as_ref().and_then(|p| p.prompt.clone()),
            completion_price: m.pricing.as_ref().and_then(|p| p.completion.clone()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_models_preserves_fallbacks_and_precedence() {
        let json = r#"{"data":[
            {"id":"a","name":"A","context_length":1000,
             "top_provider":{"context_length":2000},
             "pricing":{"prompt":"0.1","completion":"0.2"}},
            {"id":"b","context_length":3000,"top_provider":{}},
            {"id":"c"}
        ]}"#;
        let resp: OpenRouterResponse = serde_json::from_str(json).unwrap();
        let models = map_models(resp);

        assert_eq!(models.len(), 3);
        assert_eq!(models[0].name, "A");
        assert_eq!(models[0].context_length, Some(2000)); // top_provider wins
        assert_eq!(models[0].prompt_price.as_deref(), Some("0.1"));
        assert_eq!(models[0].completion_price.as_deref(), Some("0.2"));
        assert_eq!(models[1].name, "Unknown");
        assert_eq!(models[1].context_length, Some(3000)); // falls back to model
        assert_eq!(models[2].context_length, None);
        assert_eq!(models[2].prompt_price, None);
    }
}
