//! LLM providers the chat plugin can talk to.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenRouter,
    OpenCodeZen,
    Claude,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::OpenRouter, Provider::OpenCodeZen, Provider::Claude];

    pub fn name(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "OpenRouter",
            Provider::OpenCodeZen => "OpenCode Zen",
            Provider::Claude => "Claude",
        }
    }

    pub fn default_endpoint(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            Provider::OpenCodeZen => "https://opencode.ai/zen/v1/chat/completions",
            Provider::Claude => "https://api.anthropic.com/v1/messages",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "openai/gpt-4o-mini",
            Provider::OpenCodeZen => "zen",
            Provider::Claude => "claude-sonnet-4-5",
        }
    }
}
