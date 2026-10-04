//! Blocking HTTP calls to LLM backends. Always run these off the UI thread.

use std::time::Duration;

use serde_json::{Value, json};

use super::provider::Provider;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_ERROR_DETAIL_CHARS: usize = 400;

pub fn send_message(
    provider: Provider,
    endpoint: &str,
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> Result<String, String> {
    let body = match provider {
        Provider::Claude => claude_body(model, messages, 4096),
        _ => openai_body(model, messages, None),
    };
    send_request(provider, endpoint, api_key, body)
}

pub fn test_connection(
    provider: Provider,
    endpoint: &str,
    api_key: &str,
    model: &str,
) -> Result<(), String> {
    let messages = vec![("user".to_string(), "Reply with OK.".to_string())];
    let body = match provider {
        Provider::Claude => claude_body(model, &messages, 16),
        _ => openai_body(model, &messages, Some(16)),
    };
    send_request(provider, endpoint, api_key, body).map(|_| ())
}

fn openai_body(model: &str, messages: &[(String, String)], max_tokens: Option<u32>) -> Value {
    let mut body = json!({
        "model": model,
        "messages": messages.iter().map(|(role, content)| {
            json!({ "role": role, "content": content })
        }).collect::<Vec<_>>(),
    });
    if let Some(max_tokens) = max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    body
}

fn claude_body(model: &str, messages: &[(String, String)], max_tokens: u32) -> Value {
    let (system, msgs): (Vec<_>, Vec<_>) = messages.iter().partition(|(role, _)| role == "system");
    let system_text = system
        .iter()
        .map(|(_, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": msgs.iter().map(|(role, content)| {
            json!({ "role": role, "content": content })
        }).collect::<Vec<_>>(),
    });
    if !system_text.is_empty() {
        body["system"] = Value::String(system_text);
    }
    body
}

fn send_request(
    provider: Provider,
    endpoint: &str,
    api_key: &str,
    body: Value,
) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut request = agent
        .post(endpoint)
        .header("Content-Type", "application/json");
    request = match provider {
        Provider::Claude => request
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
        _ => request.header("Authorization", format!("Bearer {api_key}")),
    };

    let mut response = request
        .send_json(&body)
        .map_err(|error| explain_request_error(&error))?;
    let status = response.status().as_u16();
    let response_text = response
        .body_mut()
        .read_to_string()
        .map_err(|_| "Respons dari provider tidak dapat dibaca.".to_string())?;
    let response_json = serde_json::from_str::<Value>(&response_text);

    if !(200..300).contains(&status) {
        return Err(explain_provider_error(
            status,
            response_json.as_ref().ok(),
            &response_text,
            api_key,
        ));
    }

    let response_json = response_json
        .map_err(|_| "Provider mengirim respons JSON yang tidak valid.".to_string())?;
    response_text_for(provider, &response_json)
}

fn response_text_for(provider: Provider, response: &Value) -> Result<String, String> {
    let content = match provider {
        Provider::Claude => &response["content"],
        _ => &response["choices"][0]["message"]["content"],
    };
    extract_text(content).ok_or_else(|| {
        "Respons provider tidak berisi teks. Periksa model dan format endpoint.".to_string()
    })
}

fn extract_text(content: &Value) -> Option<String> {
    let text = content.as_str().map(str::to_owned).or_else(|| {
        content.as_array().map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
    })?;
    (!text.trim().is_empty()).then_some(text)
}

fn explain_request_error(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(_) => {
            "Permintaan ke provider melewati batas waktu 45 detik.".to_string()
        }
        ureq::Error::HostNotFound => {
            "Host provider tidak ditemukan. Periksa endpoint dan koneksi internet.".to_string()
        }
        ureq::Error::BadUri(_) | ureq::Error::RequireHttpsOnly(_) => {
            "Endpoint tidak valid. Gunakan URL HTTPS yang lengkap.".to_string()
        }
        ureq::Error::Tls(_) => {
            "Koneksi TLS gagal. Periksa sertifikat, tanggal sistem, atau jaringan.".to_string()
        }
        ureq::Error::RedirectFailed | ureq::Error::TooManyRedirects => {
            "Endpoint mengalihkan permintaan. Gunakan URL HTTPS tujuan secara langsung.".to_string()
        }
        _ => "Tidak dapat terhubung ke provider. Periksa jaringan dan endpoint.".to_string(),
    }
}

fn explain_provider_error(
    status: u16,
    body: Option<&Value>,
    raw_body: &str,
    api_key: &str,
) -> String {
    let summary = match status {
        401 => "API key ditolak. Periksa key dan provider yang dipilih.",
        403 => "Akses ditolak. Periksa izin akun, saldo, atau batasan provider.",
        404 => "Endpoint atau model tidak ditemukan. Periksa pengaturan provider.",
        408 | 504 => "Provider kehabisan waktu untuk memproses permintaan.",
        429 => "Batas penggunaan tercapai. Periksa kuota, saldo, atau rate limit.",
        500..=599 => "Provider sedang mengalami gangguan. Coba lagi nanti.",
        _ => "Provider menolak permintaan.",
    };
    let detail = body
        .and_then(provider_error_detail)
        .or_else(|| (!raw_body.trim().is_empty()).then_some(raw_body.trim()));
    let Some(detail) = detail else {
        return format!("{summary} (HTTP {status})");
    };
    let detail = if api_key.is_empty() {
        detail.to_string()
    } else {
        detail.replace(api_key, "[API key disamarkan]")
    };
    let detail = truncate_chars(&detail, MAX_ERROR_DETAIL_CHARS);
    format!("{summary} (HTTP {status}): {detail}")
}

fn provider_error_detail(body: &Value) -> Option<&str> {
    body.pointer("/error/message")
        .and_then(Value::as_str)
        .or_else(|| body.pointer("/error").and_then(Value::as_str))
        .or_else(|| body.pointer("/message").and_then(Value::as_str))
        .filter(|message| !message.trim().is_empty())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    #[test]
    fn openai_compatible_request_sends_bearer_key_and_parses_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().expect("local address");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut headers = String::new();
            let mut content_length = 0;
            {
                let mut reader = BufReader::new(&mut stream);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("read request header");
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        content_length = value.trim().parse().expect("content length");
                    }
                    headers.push_str(&line);
                }
                let mut body = vec![0; content_length];
                reader.read_exact(&mut body).expect("read request body");
            }
            let response = r#"{"choices":[{"message":{"content":"Connected"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .expect("write response");
            headers
        });

        let messages = vec![("user".to_string(), "hello".to_string())];
        let response = send_message(
            Provider::OpenRouter,
            &format!("http://{address}/chat/completions"),
            "test-secret",
            "test-model",
            &messages,
        )
        .expect("successful provider response");
        let headers = worker.join().expect("test server thread");

        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-secret")
        );
        assert_eq!(response, "Connected");
    }

    #[test]
    fn connection_test_checks_the_selected_model_with_a_small_prompt() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().expect("local address");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut reader = BufReader::new(&mut stream);
            let mut headers = String::new();
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request header");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("content length");
                }
                headers.push_str(&line);
            }
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).expect("read request body");
            drop(reader);
            let response = r#"{"choices":[{"message":{"content":"OK"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .expect("write response");
            (
                headers,
                String::from_utf8(body).expect("UTF-8 request body"),
            )
        });

        test_connection(
            Provider::OpenRouter,
            &format!("http://{address}/chat/completions"),
            "test-secret",
            "test-model",
        )
        .expect("test request should validate provider credentials");
        let (headers, body) = worker.join().expect("test server thread");
        let body: Value = serde_json::from_str(&body).expect("valid request JSON");

        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-secret")
        );
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["max_tokens"], 16);
        assert_eq!(body["messages"][0]["content"], "Reply with OK.");
    }

    #[test]
    fn provider_errors_explain_auth_failures_without_exposing_key() {
        let error = explain_provider_error(
            401,
            Some(&json!({"error": {"message": "rejected test-secret"}})),
            "",
            "test-secret",
        );

        assert!(error.contains("API key ditolak"));
        assert!(error.contains("[API key disamarkan]"));
        assert!(!error.contains("test-secret"));
    }

    #[test]
    fn http_error_responses_are_reported_with_provider_detail() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test server");
        let address = listener.local_addr().expect("local address");
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut reader = BufReader::new(&mut stream);
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request header");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = value.trim().parse().expect("content length");
                }
            }
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body).expect("read request body");
            drop(reader);
            let response = r#"{"error":{"message":"invalid credentials"}}"#;
            write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .expect("write response");
        });

        let messages = vec![("user".to_string(), "hello".to_string())];
        let error = send_message(
            Provider::OpenRouter,
            &format!("http://{address}/chat/completions"),
            "test-secret",
            "test-model",
            &messages,
        )
        .expect_err("unauthorized provider response");
        worker.join().expect("test server thread");

        assert!(error.contains("API key ditolak"));
        assert!(error.contains("invalid credentials"));
        assert!(!error.contains("test-secret"));
    }

    #[test]
    fn response_extraction_rejects_missing_text() {
        let error = response_text_for(Provider::OpenRouter, &json!({"choices": []}))
            .expect_err("empty provider response must not be accepted");

        assert!(error.contains("tidak berisi teks"));
    }

    #[test]
    fn response_extraction_collects_text_blocks() {
        let response = json!({
            "content": [
                {"type": "text", "text": "Hello"},
                {"type": "text", "text": "World"}
            ]
        });

        assert_eq!(
            response_text_for(Provider::Claude, &response).expect("text blocks"),
            "Hello\nWorld"
        );
    }
}
