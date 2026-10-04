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
    keys_loaded: [bool; 3],
    keys_saved: [bool; 3],
    pub(crate) input: String,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) connection_status: Option<(bool, String)>,
    pending: bool,
    pending_action: Option<RequestAction>,
    rx: Option<Receiver<RequestResult>>,
}

#[derive(Clone, Copy)]
enum RequestAction {
    SendMessage,
    TestConnection,
}

struct RequestResult {
    action: RequestAction,
    result: Result<String, String>,
}

impl AiChatPlugin {
    pub fn new() -> Self {
        let mut plugin = Self {
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
            keys_loaded: [false; 3],
            keys_saved: [false; 3],
            input: String::new(),
            messages: Vec::new(),
            connection_status: None,
            pending: false,
            pending_action: None,
            rx: None,
        };
        plugin.load_provider_credential(Provider::OpenRouter);
        plugin.sync_fields();
        plugin
    }

    fn load_provider_credential(&mut self, provider: Provider) {
        let idx = provider.index();
        match load_credential(provider) {
            Ok(Some(key)) => {
                self.keys[idx] = key;
                self.keys_saved[idx] = true;
                self.keys_loaded[idx] = true;
            }
            Ok(None) => self.keys_loaded[idx] = true,
            Err(error) => {
                self.connection_status = Some((
                    false,
                    format!(
                        "Key {} tidak dapat dibaca dari penyimpanan aman: {error}",
                        provider.name()
                    ),
                ));
            }
        }
    }

    fn idx(&self) -> usize {
        self.provider.index()
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
        if self.provider == prov {
            return;
        }
        self.save_fields();
        self.provider = prov;
        self.connection_status = None;
        if !self.keys_loaded[self.idx()] {
            self.load_provider_credential(prov);
        }
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

    pub(crate) fn mark_api_key_changed(&mut self) {
        self.keys_saved[self.idx()] = false;
        self.connection_status = None;
    }

    pub(crate) fn api_key_saved(&self) -> bool {
        self.keys_saved[self.idx()]
    }

    pub(crate) fn save_api_key(&mut self) {
        let idx = self.idx();
        let result = store_credential(self.provider, &self.api_key);
        match result {
            Ok(()) => {
                self.keys[idx] = self.api_key.trim().to_string();
                self.keys_loaded[idx] = true;
                self.keys_saved[idx] = !self.api_key.trim().is_empty();
                self.connection_status = Some((
                    true,
                    if self.api_key.trim().is_empty() {
                        "API key tersimpan telah dihapus dari penyimpanan aman.".into()
                    } else {
                        "API key tersimpan aman di credential manager sistem.".into()
                    },
                ));
            }
            Err(error) => {
                self.keys_saved[idx] = false;
                self.connection_status = Some((
                    false,
                    format!(
                        "API key tidak dapat disimpan dengan aman: {error}. Key tetap tersedia hanya selama aplikasi berjalan."
                    ),
                ));
            }
        }
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
        if let Err(error) = validate_settings(&self.endpoint, &self.model, &self.api_key) {
            self.connection_status = Some((false, error.clone()));
            self.messages.push(ChatMessage {
                role: "error",
                text: error,
            });
            return;
        }
        self.messages.push(ChatMessage {
            role: "user",
            text: text.clone(),
        });
        self.input.clear();
        let messages: Vec<(String, String)> = self
            .messages
            .iter()
            .filter(|m| m.role != "error")
            .map(|m| (m.role.to_string(), m.text.clone()))
            .collect();

        self.start_request(RequestAction::SendMessage, messages);
    }

    pub(crate) fn test_connection(&mut self) {
        if self.pending {
            return;
        }
        self.start_request(RequestAction::TestConnection, Vec::new());
    }

    fn start_request(&mut self, action: RequestAction, messages: Vec<(String, String)>) {
        if let Err(error) = validate_settings(&self.endpoint, &self.model, &self.api_key) {
            self.connection_status = Some((false, error.clone()));
            if matches!(action, RequestAction::SendMessage) {
                self.messages.push(ChatMessage {
                    role: "error",
                    text: error,
                });
            }
            return;
        }

        self.pending = true;
        self.pending_action = Some(action);
        self.connection_status = Some((
            false,
            match action {
                RequestAction::SendMessage => "Mengirim permintaan ke provider…".into(),
                RequestAction::TestConnection => "Menguji koneksi dan kredensial…".into(),
            },
        ));

        let provider = self.provider;
        let endpoint = self.endpoint.trim().to_string();
        let model = self.model.trim().to_string();
        let api_key = self.api_key.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let worker = thread::Builder::new()
            .name("ione-ai-chat".into())
            .spawn(move || {
                let result = match action {
                    RequestAction::SendMessage => {
                        client::send_message(provider, &endpoint, &api_key, &model, &messages)
                    }
                    RequestAction::TestConnection => {
                        client::test_connection(provider, &endpoint, &api_key, &model)
                            .map(|()| "Connection verified".to_string())
                    }
                };
                let _ = tx.send(RequestResult { action, result });
            });
        if let Err(error) = worker {
            self.pending = false;
            self.pending_action = None;
            self.rx = None;
            let message = format!("Tidak dapat memulai permintaan AI: {error}");
            self.connection_status = Some((false, message.clone()));
            if matches!(action, RequestAction::SendMessage) {
                self.messages.push(ChatMessage {
                    role: "error",
                    text: message,
                });
            };
        }
    }

    fn poll(&mut self) -> bool {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(result) => self.finish_request(result),
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.finish_request(RequestResult {
                        action: self.pending_action.unwrap_or(RequestAction::SendMessage),
                        result: Err("Proses permintaan AI berhenti tanpa mengirim hasil.".into()),
                    });
                }
            }
            self.pending = false;
            self.pending_action = None;
            self.rx = None;
            return true;
        }
        false
    }

    fn finish_request(&mut self, result: RequestResult) {
        match result.result {
            Ok(text) => match result.action {
                RequestAction::SendMessage => {
                    self.messages.push(ChatMessage {
                        role: "assistant",
                        text,
                    });
                    self.connection_status = Some((true, "Respons AI berhasil diterima.".into()));
                }
                RequestAction::TestConnection => {
                    self.connection_status = Some((
                        true,
                        format!(
                            "Terhubung ke {} dengan model {}.",
                            self.provider.name(),
                            self.model
                        ),
                    ));
                }
            },
            Err(error) => {
                self.connection_status = Some((false, error.clone()));
                if matches!(result.action, RequestAction::SendMessage) {
                    self.messages.push(ChatMessage {
                        role: "error",
                        text: error,
                    });
                }
            }
        }
    }
}

fn credential_entry(provider: Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new("ione-ai-chat", provider.credential_id()).map_err(|error| error.to_string())
}

fn load_credential(provider: Provider) -> Result<Option<String>, String> {
    match credential_entry(provider)?.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn store_credential(provider: Provider, api_key: &str) -> Result<(), String> {
    let entry = credential_entry(provider)?;
    if api_key.trim().is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    } else {
        entry
            .set_password(api_key.trim())
            .map_err(|error| error.to_string())
    }
}

fn validate_settings(endpoint: &str, model: &str, api_key: &str) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("API key belum diisi.".into());
    }
    if model.trim().is_empty() {
        return Err("Nama model belum diisi.".into());
    }
    let endpoint = endpoint.trim();
    let endpoint_lower = endpoint.to_ascii_lowercase();
    let secure = endpoint_lower.starts_with("https://");
    let local_http = endpoint_lower
        .strip_prefix("http://")
        .and_then(|authority_and_path| authority_and_path.split(['/', '?', '#']).next())
        .is_some_and(is_loopback_authority);
    if !(secure || local_http) {
        return Err(
            "Endpoint harus menggunakan HTTPS. HTTP hanya diizinkan untuk layanan lokal.".into(),
        );
    }
    Ok(())
}

fn is_loopback_authority(authority: &str) -> bool {
    ["localhost", "127.0.0.1", "[::1]"].iter().any(|host| {
        authority == *host
            || authority
                .strip_prefix(host)
                .and_then(|suffix| suffix.strip_prefix(':'))
                .is_some_and(|port| !port.is_empty() && port.parse::<u16>().is_ok())
    })
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
        if egui_ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::A))
        {
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

#[cfg(test)]
mod tests {
    use super::validate_settings;

    #[test]
    fn valid_https_provider_settings_are_accepted() {
        assert!(validate_settings("https://api.example.com/chat", "model", "key").is_ok());
    }

    #[test]
    fn local_http_endpoints_are_supported_for_local_models() {
        assert!(validate_settings("http://localhost:1234/v1", "model", "key").is_ok());
        assert!(validate_settings("http://127.0.0.1:1234/v1", "model", "key").is_ok());
        assert!(validate_settings("http://[::1]:1234/v1", "model", "key").is_ok());
    }

    #[test]
    fn non_local_http_endpoints_are_rejected_before_sending_secrets() {
        assert!(
            validate_settings("http://example.com/v1", "model", "key")
                .expect_err("remote HTTP is insecure")
                .contains("HTTPS")
        );
        assert!(validate_settings("http://localhost.attacker.test/v1", "model", "key").is_err());
    }

    #[test]
    fn missing_configuration_is_reported_before_a_request_starts() {
        assert!(validate_settings("https://api.example.com", "model", "").is_err());
        assert!(validate_settings("https://api.example.com", "", "key").is_err());
    }
}
