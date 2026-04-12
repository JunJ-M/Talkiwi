use anyhow::Context;
use talkiwi_core::traits::intent::{IntentProvider, IntentRaw};

const DEFAULT_MODEL: &str = "qwen2.5:7b";
/// Timeout for Ollama API calls. First inference may be slower due to
/// model loading; subsequent calls are typically < 5s for small models.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Ollama-based IntentProvider using the raw HTTP `/api/generate` endpoint.
///
/// Avoids the `ollama-rs` crate to sidestep Rust version constraints.
/// Sends a system prompt + user prompt and parses JSON output into `IntentRaw`.
pub struct OllamaProvider {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Create a new OllamaProvider.
    ///
    /// # Panics
    /// Panics if the TLS backend is unavailable (configuration-time invariant).
    pub fn new(base_url: impl Into<String>, model: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .expect("TLS backend unavailable — cannot create HTTP client");

        Self {
            base_url: base_url.into(),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            client,
        }
    }

    /// Create a provider connecting to the default local Ollama instance.
    pub fn default_local() -> Self {
        Self::new("http://localhost:11434", None)
    }
}

#[derive(serde::Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    format: &'a str,
    stream: bool,
}

#[derive(serde::Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[async_trait::async_trait]
impl IntentProvider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }

    fn name(&self) -> &str {
        "Ollama Local LLM"
    }

    fn requires_network(&self) -> bool {
        false
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                // Consume body to avoid leaking the connection
                let _ = resp.bytes().await;
                true
            }
            Err(_) => false,
        }
    }

    async fn restructure(
        &self,
        transcript: &str,
        events_summary: &str,
        system_prompt: &str,
    ) -> anyhow::Result<IntentRaw> {
        let user_prompt = format!("口语转录:\n{}\n\n操作事件:\n{}", transcript, events_summary);

        let request_body = OllamaGenerateRequest {
            model: &self.model,
            prompt: &user_prompt,
            system: system_prompt,
            format: "json",
            stream: false,
        };

        let url = format!("{}/api/generate", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .context("failed to call Ollama API")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error ({}): {}", status, body);
        }

        let ollama_resp: OllamaGenerateResponse = response
            .json()
            .await
            .context("failed to parse Ollama response")?;

        let raw: IntentRaw = serde_json::from_str(&ollama_resp.response)
            .context("failed to parse LLM JSON output as IntentRaw")?;

        Ok(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_provider_metadata() {
        let provider = OllamaProvider::default_local();
        assert_eq!(provider.id(), "ollama");
        assert_eq!(provider.name(), "Ollama Local LLM");
        assert!(!provider.requires_network());
    }

    #[test]
    fn parse_valid_intent_raw_json() {
        let json = r#"{
            "task": "重写这段代码",
            "intent": "rewrite",
            "constraints": ["使用 Rust"],
            "missing_context": [],
            "restructured_speech": "请重写选中的代码段"
        }"#;

        let raw: IntentRaw = serde_json::from_str(json).unwrap();
        assert_eq!(raw.task, "重写这段代码");
        assert_eq!(raw.intent, "rewrite");
        assert_eq!(raw.constraints, vec!["使用 Rust"]);
    }

    #[test]
    fn parse_malformed_json_returns_error() {
        let json = r#"{ "task": "incomplete"#;
        let result: Result<IntentRaw, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
