//! LLM providers the chat plugin can talk to.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenRouter,
    OpenCodeZen,
    Claude,
}

impl Provider {
    pub const ALL: [Provider; 3] = [
        Provider::OpenRouter,
        Provider::OpenCodeZen,
        Provider::Claude,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "OpenRouter",
            Provider::OpenCodeZen => "OpenCode Zen",
            Provider::Claude => "Claude",
        }
    }

    pub fn credential_id(&self) -> &'static str {
        match self {
            Provider::OpenRouter => "openrouter",
            Provider::OpenCodeZen => "opencode-zen",
            Provider::Claude => "claude",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Provider::OpenRouter => 0,
            Provider::OpenCodeZen => 1,
            Provider::Claude => 2,
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
            Provider::OpenCodeZen => "big-pickle",
            Provider::Claude => "claude-sonnet-4-5",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Provider;

    #[test]
    fn opencode_zen_default_uses_a_published_model_id() {
        assert_eq!(Provider::OpenCodeZen.default_model(), "big-pickle");
        assert_eq!(
            Provider::OpenCodeZen.default_endpoint(),
            "https://opencode.ai/zen/v1/chat/completions"
        );
    }
}
