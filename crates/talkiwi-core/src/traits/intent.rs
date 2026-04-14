use serde::{Deserialize, Serialize};

/// A single reference resolved by the LLM — links a spoken phrase to an event by index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawReference {
    /// The spoken phrase (e.g. "这段代码", "刚才那个截图")
    pub spoken_text: String,
    /// 0-based index into the events list provided in the prompt.
    pub event_index: usize,
    /// Why the LLM believes this phrase refers to this event.
    pub reason: String,
}

/// Raw LLM output from intent restructuring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRaw {
    pub task: String,
    pub intent: String,
    pub constraints: Vec<String>,
    pub missing_context: Vec<String>,
    pub restructured_speech: String,
    /// LLM-resolved references: which spoken phrases map to which events.
    #[serde(default)]
    pub references: Vec<RawReference>,
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
