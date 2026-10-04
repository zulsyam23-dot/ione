//! Blocking HTTP calls to LLM backends. Always run these off the UI thread
//! (the plugin spawns a worker per request and delivers over an `mpsc`).

use serde_json::json;

pub fn call_openai_compatible(
    endpoint: &str,
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> Result<String, String> {
    let body = json!({
        "model": model,
        "messages": messages.iter().map(|(role, content)| {
            json!({ "role": role, "content": content })
        }).collect::<Vec<_>>(),
    });
    let mut resp = ureq::post(endpoint)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send_json(&body)
        .map_err(|e| format!("Request gagal: {e}"))?;
    let json: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("Respons tidak valid: {e}"))?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Format respons tak dikenal: {json}"))
}

pub fn call_claude(
    endpoint: &str,
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> Result<String, String> {
    let (system, msgs): (Vec<_>, Vec<_>) = messages
        .iter()
        .partition(|(role, _)| role == "system");
    let system_text = system
        .iter()
        .map(|(_, c)| c.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut body = json!({
        "model": model,
        "max_tokens": 4096,
        "messages": msgs.iter().map(|(role, content)| {
            json!({ "role": role, "content": content })
        }).collect::<Vec<_>>(),
    });
    if !system_text.is_empty() {
        body["system"] = serde_json::Value::String(system_text);
    }
    let mut resp = ureq::post(endpoint)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| format!("Request gagal: {e}"))?;
    let json: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("Respons tidak valid: {e}"))?;
    json["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Format respons tak dikenal: {json}"))
}
