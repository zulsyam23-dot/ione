//! Built-in AI chat plugin: talks to OpenRouter, OpenCode Zen (OpenAI-compatible)
//! and Anthropic Claude. Loaded through the same plugin API as third-party
//! plugins — it is not special-cased anywhere in the app shell.
//!
//! Layout of this module:
//! - `provider`: which LLM backends exist and their defaults.
//! - `client`:   blocking HTTP calls to those backends (run on a worker thread).
//! - `ui`:       all egui rendering for the dock panel.
//! - `mod.rs`:   plugin state + lifecycle (`Plugin` impl) here.

mod client;
mod provider;
mod ui;

use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;

use provider::Provider;

use crate::plugin::{Plugin, PluginContext};

pub(crate) struct ChatMessage {
    pub role: &'static str, // "user" | "assistant" | "error"
    pub text: String,
}

pub struct AiChatPlugin {
    open: bool,
    provider: Provider,
    endpoint: String,
    model: String,
    api_key: String,
    keys: [String; 3],
    models: [String; 3],
    endpoints: [String; 3],
    pub(crate) input: String,
    pub(crate) messages: Vec<ChatMessage>,
    pending: bool,
    rx: Option<Receiver<Result<String, String>>>,
}

impl AiChatPlugin {
    pub fn new() -> Self {
        Self {
            open: false,
            provider: Provider::OpenRouter,
            endpoint: Provider::OpenRouter.default_endpoint().to_string(),
            model: Provider::OpenRouter.default_model().to_string(),
            api_key: String::new(),
            keys: Default::default(),
            models: [
                Provider::OpenRouter.default_model().to_string(),
                Provider::OpenCodeZen.default_model().to_string(),
                Provider::Claude.default_model().to_string(),
            ],
            endpoints: [
                Provider::OpenRouter.default_endpoint().to_string(),
                Provider::OpenCodeZen.default_endpoint().to_string(),
                Provider::Claude.default_endpoint().to_string(),
            ],
            input: String::new(),
            messages: Vec::new(),
            pending: false,
            rx: None,
        }
    }

    fn idx(&self) -> usize {
        self.provider as usize
    }

    pub(crate) fn sync_fields(&mut self) {
        let i = self.idx();
        self.endpoint = self.endpoints[i].clone();
        self.model = self.models[i].clone();
        self.api_key = self.keys[i].clone();
    }

    pub(crate) fn save_fields(&mut self) {
        let i = self.idx();
        self.endpoints[i] = self.endpoint.clone();
        self.models[i] = self.model.clone();
        self.keys[i] = self.api_key.clone();
    }

    pub(crate) fn provider(&self) -> Provider {
        self.provider
    }

    pub(crate) fn set_provider(&mut self, prov: Provider) {
        self.save_fields();
        self.provider = prov;
        self.sync_fields();
    }

    pub(crate) fn endpoint_mut(&mut self) -> &mut String {
        &mut self.endpoint
    }

    pub(crate) fn model_mut(&mut self) -> &mut String {
        &mut self.model
    }

    pub(crate) fn api_key_mut(&mut self) -> &mut String {
        &mut self.api_key
    }

    pub(crate) fn clear(&mut self) {
        self.messages.clear();
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending
    }

    pub fn send(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() || self.pending {
            return;
        }
        if self.api_key.trim().is_empty() {
            self.messages.push(ChatMessage {
                role: "error",
                text: "API key belum diisi.".into(),
            });
            return;
        }
        self.messages.push(ChatMessage {
            role: "user",
            text: text.clone(),
        });
        self.input.clear();
        self.pending = true;

        let provider = self.provider;
        let endpoint = self.endpoint.clone();
        let model = self.model.clone();
        let api_key = self.api_key.clone();
        let messages: Vec<(String, String)> = self
            .messages
            .iter()
            .filter(|m| m.role != "error")
            .map(|m| (m.role.to_string(), m.text.clone()))
            .collect();

        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        thread::spawn(move || {
            let result = match provider {
                Provider::Claude => client::call_claude(&endpoint, &api_key, &model, &messages),
                _ => client::call_openai_compatible(&endpoint, &api_key, &model, &messages),
            };
            let _ = tx.send(result);
        });
    }

    fn poll(&mut self) -> bool {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(Ok(text)) => {
                    self.messages.push(ChatMessage {
                        role: "assistant",
                        text,
                    });
                    self.pending = false;
                    self.rx = None;
                    return true;
                }
                Ok(Err(e)) => {
                    self.messages.push(ChatMessage {
                        role: "error",
                        text: e,
                    });
                    self.pending = false;
                    self.rx = None;
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = false;
                    self.rx = None;
                    return true;
                }
            }
        }
        false
    }
}

impl Default for AiChatPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for AiChatPlugin {
    fn name(&self) -> &str {
        "ai-chat"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn toggle_window(&mut self) {
        self.open = !self.open;
    }

    fn on_ui(&mut self, egui_ctx: &egui::Context, _pctx: &mut PluginContext) {
        // Ctrl+Shift+A toggles the chat panel.
        if egui_ctx.input(|i| {
            i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::A)
        }) {
            self.open = !self.open;
        }

        if self.poll() {
            egui_ctx.request_repaint();
        }
        // Nothing else would wake the UI up while a request is in flight, so
        // poll on a timer — otherwise the response (or spinner) stalls.
        if self.pending {
            egui_ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }
    }

    fn dock_open(&self) -> bool {
        self.open
    }

    fn on_dock(&mut self, ui: &mut egui::Ui, pctx: &mut PluginContext) {
        ui::render(self, ui, &pctx.palette);
    }
}
