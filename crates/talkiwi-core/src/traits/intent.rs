use serde::{Deserialize, Serialize};

/// Raw LLM output from intent restructuring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRaw {
    pub task: String,
    pub intent: String,
    pub constraints: Vec<String>,
    pub missing_context: Vec<String>,
    pub restructured_speech: String,
}

/// Intent provider trait — implemented by Ollama, cloud LLM providers, etc.
#[async_trait::async_trait]
pub trait IntentProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn requires_network(&self) -> bool;
    async fn is_available(&self) -> bool;
    async fn restructure(
        &self,
        transcript: &str,
        events_summary: &str,
        system_prompt: &str,
    ) -> anyhow::Result<IntentRaw>;
}
