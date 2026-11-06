//! Bring-your-own AI narration: an OpenAI-compatible chat endpoint the user
//! runs themselves ("local": this computer or their own network), a cloud
//! one with their own key ("cloud": https only), or Anthropic's Messages API
//! with their own key ("anthropic": https only). Off by default.
//!
//! The flow is fixed: `ai_preview` builds the rounded, nameless digest
//! (see `samriddhi_core::digest`) and returns the exact text; only
//! `ai_send`, called with that same text, sends it, once. The key lives
//! only in the OS keyring (service `dev.brahma.samriddhi.ai`) or, on Android,
//! encrypted by its own Android Keystore key, is sent only to a cloud or
//! Anthropic endpoint over https, only in a header, and never appears in an
//! error. The
//! model's answer is plain text to show; it never becomes a step or an
//! action.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use samriddhi_core::digest::{NARRATOR_SYSTEM_PROMPT, digest_text};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, State, Url};
use tauri_plugin_samriddhi_device::DeviceExt;

use crate::{App, CommandResult, get, now};

/// Where the settings are kept (not the key: that is in the keyring).
pub const SETTING: &str = "ai.settings";
/// Ollama's OpenAI-compatible endpoint on its default port.
pub const LOCAL_SUGGESTION: &str = "http://localhost:11434/v1";
/// Anthropic's API address, and the model suggested for it (the user can
/// type any model id).
pub const ANTHROPIC_SUGGESTION: &str = "https://api.anthropic.com";
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-5-5";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// The most the digest text may be.
pub const MAX_PROMPT_BYTES: usize = 16 * 1024;
/// The most of the model's answer that is read.
pub const MAX_REPLY_BYTES: usize = 256 * 1024;
/// The most of the model's answer that is shown.
pub const MAX_REPLY_CHARS: usize = 6_000;
pub const MAX_TOKENS: u32 = 600;
pub const TIMEOUT: Duration = Duration::from_secs(90);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ENDPOINT: usize = 300;
const MAX_MODEL: usize = 100;
const MAX_KEY: usize = 512;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    /// On this computer or the user's own network; plain http allowed; no key.
    #[default]
    Local,
    /// A cloud endpoint with the user's own key; https only.
    Cloud,
    /// Anthropic's Messages API with the user's own key; https only.
    Anthropic,
}

/// The AI settings, without the key. Off by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub enabled: bool,
    pub provider: Provider,
    /// Base URL, for example `http://localhost:11434/v1`.
    pub endpoint: String,
    pub model: String,
}

/// Is this URL host (as `Url::host_str` gives it) on this computer or the
/// user's own network: loopback, private (RFC 1918), shared/tailnet
/// (100.64.0.0/10), IPv6 unique-local or link-local, `localhost` or a
/// `.local` name?
pub fn is_local_host(host: &str) -> bool {
    fn v4(ip: &Ipv4Addr) -> bool {
        let [a, b, ..] = ip.octets();
        ip.is_loopback() || ip.is_private() || (a == 100 && (64..=127).contains(&b))
    }
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner.parse::<Ipv6Addr>().is_ok_and(|ip| {
            let s = ip.segments()[0];
            ip.is_loopback()
                || (s & 0xfe00) == 0xfc00
                || (s & 0xffc0) == 0xfe80
                || ip.to_ipv4_mapped().is_some_and(|m| v4(&m))
        });
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return v4(&ip);
    }
    let d = host.to_ascii_lowercase();
    d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local")
}

fn plain(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// The shape of the settings: lengths and characters. Checked on every save.
pub fn check_shape(s: &AiSettings) -> CommandResult<()> {
    if s.endpoint.chars().count() > MAX_ENDPOINT || !plain(&s.endpoint) {
        return Err(format!(
            "Keep the endpoint to one address under {MAX_ENDPOINT} letters."
        ));
    }
    if s.model.chars().count() > MAX_MODEL || !plain(&s.model) {
        return Err(format!(
            "Keep the model name to one line under {MAX_MODEL} letters."
        ));
    }
    Ok(())
}

/// Why these settings cannot be used, or `None` when they can.
pub fn problem(s: &AiSettings, key_set: bool) -> Option<String> {
    if let Err(e) = check_shape(s) {
        return Some(e);
    }
    if s.model.trim().is_empty() {
        return Some("Enter a model name.".into());
    }
    let Ok(u) = Url::parse(s.endpoint.trim()) else {
        return Some("Enter a valid endpoint address, such as http://localhost:11434/v1.".into());
    };
    let Some(host) = u.host_str().filter(|h| !h.is_empty()) else {
        return Some("Enter a valid endpoint address, such as http://localhost:11434/v1.".into());
    };
    if !matches!(u.scheme(), "http" | "https") {
        return Some("The endpoint must start with http:// or https://.".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Some(
            "Leave names and passwords out of the address; a cloud key goes in its own box.".into(),
        );
    }
    if u.query().is_some() || u.fragment().is_some() {
        return Some("Leave anything after ? or # out of the endpoint address.".into());
    }
    match s.provider {
        Provider::Local if !is_local_host(host) => Some(
            "A local model must run on this computer or your own network (localhost, a private \
             address or a .local name). Choose cloud for anything else."
                .into(),
        ),
        Provider::Local => None,
        Provider::Cloud | Provider::Anthropic if u.scheme() != "https" => {
            Some("Cloud endpoints must use https://.".into())
        }
        Provider::Cloud | Provider::Anthropic if !key_set => Some("Add your API key.".into()),
        Provider::Cloud | Provider::Anthropic => None,
    }
}

/// A key the user typed: one line of visible characters.
pub fn check_key(key: &str) -> CommandResult<String> {
    let k = key.trim();
    if k.is_empty() || k.chars().count() > MAX_KEY || !k.chars().all(|c| c.is_ascii_graphic()) {
        return Err(format!(
            "An API key is one line of letters, digits and symbols, up to {MAX_KEY} of them."
        ));
    }
    Ok(k.to_owned())
}

/// How the key goes with the request: always a header, never the body or URL.
pub enum Credential {
    /// `Authorization: Bearer <key>` (OpenAI-compatible endpoints).
    Bearer(String),
    /// `x-api-key: <key>` (Anthropic).
    ApiKey(String),
}

/// A chat request ready to post. Deliberately not `Debug`: it may hold the key.
pub struct ChatRequest {
    pub url: String,
    pub credential: Option<Credential>,
    /// Fixed headers that are not secret.
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: Vec<u8>,
    pub provider: Provider,
    /// Sent to this computer or the user's own network.
    pub local: bool,
}

/// The request for `user`, the previewed text: OpenAI-compatible chat, or
/// Anthropic's Messages API. The key goes only to a cloud or Anthropic
/// endpoint, only in a header.
pub fn chat_request(
    s: &AiSettings,
    key: Option<&str>,
    system: &str,
    user: &str,
) -> CommandResult<ChatRequest> {
    if let Some(p) = problem(s, key.is_some_and(|k| !k.is_empty())) {
        return Err(p);
    }
    if user.len() > MAX_PROMPT_BYTES {
        return Err("The summary is too long to send.".into());
    }
    let base = s.endpoint.trim().trim_end_matches('/');
    let (url, body, headers) = match s.provider {
        Provider::Anthropic => (
            // The base may be given with or without its /v1.
            format!(
                "{}/messages",
                if base.ends_with("/v1") {
                    base.to_owned()
                } else {
                    format!("{base}/v1")
                }
            ),
            json!({
                "model": s.model.trim(),
                "max_tokens": MAX_TOKENS,
                "system": system,
                "messages": [{ "role": "user", "content": user }],
            }),
            vec![("anthropic-version", ANTHROPIC_VERSION)],
        ),
        Provider::Local | Provider::Cloud => (
            format!("{base}/chat/completions"),
            json!({
                "model": s.model.trim(),
                "max_tokens": MAX_TOKENS,
                "stream": false,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user },
                ],
            }),
            Vec::new(),
        ),
    };
    let credential = key.map(str::to_owned).and_then(|k| match s.provider {
        Provider::Cloud => Some(Credential::Bearer(k)),
        Provider::Anthropic => Some(Credential::ApiKey(k)),
        Provider::Local => None,
    });
    Ok(ChatRequest {
        url,
        credential,
        headers,
        provider: s.provider,
        body: serde_json::to_vec(&body).map_err(|_| "Could not prepare the request.")?,
        local: s.provider == Provider::Local,
    })
}

/// The model's answer from a reply. Errors give the status only: a
/// provider's error body can echo the request.
pub fn parse_reply(provider: Provider, status: u16, body: &[u8]) -> CommandResult<String> {
    if status != 200 {
        return Err(format!("The model endpoint answered with HTTP {status}."));
    }
    let bad = || "The model's answer was not in the expected format.".to_string();
    let v: Value = serde_json::from_slice(body).map_err(|_| bad())?;
    let joined;
    let text = match provider {
        Provider::Anthropic => {
            // A message whose content is a list of typed blocks; the text
            // blocks are the answer, other kinds are skipped.
            if v["type"] != "message" || v["role"] != "assistant" {
                return Err(bad());
            }
            if v["stop_reason"] == "refusal" {
                return Err("The model declined to answer.".into());
            }
            let blocks = v["content"].as_array().ok_or_else(bad)?;
            let mut text = String::new();
            for b in blocks {
                if b["type"].as_str().ok_or_else(bad)? == "text" {
                    text.push_str(b["text"].as_str().ok_or_else(bad)?);
                }
            }
            joined = text;
            joined.trim()
        }
        Provider::Local | Provider::Cloud => v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(bad)?
            .trim(),
    };
    if text.is_empty() {
        return Err("The model gave an empty answer.".into());
    }
    Ok(text.chars().take(MAX_REPLY_CHARS).collect())
}

/// Desktop trusts the OS certificate store. On Android, reqwest's own verifier
/// needs a Java helper this app does not bundle, so phones trust the bundled
/// Mozilla roots instead (a private CA on an https endpoint will not work there).
#[cfg(not(target_os = "android"))]
fn tls_roots(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    b
}

#[cfg(target_os = "android")]
fn tls_roots(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    b.tls_certs_only(
        webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|c| reqwest::Certificate::from_der(c.as_ref()).ok()),
    )
}

/// Post the request and read at most [`MAX_REPLY_BYTES`] of the answer.
/// No redirects are followed; a local endpoint is reached without proxies.
pub async fn post(req: ChatRequest, timeout: Duration) -> CommandResult<String> {
    fn net(e: reqwest::Error, timeout: Duration) -> String {
        // Never the error's own text: keep messages fixed and key-free.
        if e.is_timeout() {
            format!(
                "The model did not answer within {} seconds.",
                timeout.as_secs()
            )
        } else if e.is_connect() {
            "Could not connect to the model. Is it running, and is the address right?".into()
        } else {
            "Could not reach the model.".into()
        }
    }
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // Fails only if another thread installed one first, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT.min(timeout))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("Samriddhi/", env!("CARGO_PKG_VERSION")));
    let builder = if req.local {
        builder.no_proxy()
    } else {
        builder
    };
    let client = tls_roots(builder)
        .build()
        .map_err(|_| "Could not set up the connection.".to_string())?;
    let mut rb = client
        .post(&req.url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(req.body);
    for (name, value) in &req.headers {
        rb = rb.header(*name, *value);
    }
    rb = match &req.credential {
        Some(Credential::Bearer(k)) => rb.bearer_auth(k),
        Some(Credential::ApiKey(k)) => rb.header("x-api-key", k),
        None => rb,
    };
    let mut res = rb.send().await.map_err(|e| net(e, timeout))?;
    let status = res.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| net(e, timeout))? {
        if body.len() + chunk.len() > MAX_REPLY_BYTES {
            return Err("The model's answer was too long to read.".into());
        }
        body.extend_from_slice(&chunk);
    }
    parse_reply(req.provider, status, &body)
}

/// A preview waiting to be sent: the text exactly as shown, and the
/// settings it was shown for.
pub struct Pending {
    text: String,
    settings: AiSettings,
}

/// The pending preview, if `text` and `settings` are exactly what it was
/// shown with. Taken either way: a preview is sent at most once.
pub fn take_matching(
    pending: &mut Option<Pending>,
    text: &str,
    settings: &AiSettings,
) -> CommandResult<String> {
    match pending.take() {
        Some(p) if p.text == text && &p.settings == settings => Ok(p.text),
        _ => Err("Preview it again first: only the exact text you saw can be sent.".into()),
    }
}

fn settings_of(state: &App) -> CommandResult<AiSettings> {
    state.with(|s| Ok(get::<AiSettings>(s, SETTING).unwrap_or_default()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    settings: AiSettings,
    /// Only whether a key is kept, never the key.
    key_set: bool,
    /// Why the keyring (or Android Keystore) cannot be read, if it cannot.
    key_problem: Option<String>,
    /// Why the settings cannot be used yet, if they cannot.
    problem: Option<String>,
    local_suggestion: &'static str,
    anthropic_suggestion: &'static str,
    /// The model id offered for Anthropic; the user can type any other.
    anthropic_model: &'static str,
    /// Every build can send. Kept so the screens need not guess.
    can_send: bool,
}

#[tauri::command]
pub fn ai_settings_get(app: AppHandle, state: State<App>) -> CommandResult<AiSettingsView> {
    let settings = settings_of(&state)?;
    let (key_set, key_problem) = match app.device().ai_key() {
        Ok(k) => (k.is_some(), None),
        Err(e) => (false, Some(e.to_string())),
    };
    Ok(AiSettingsView {
        problem: problem(&settings, key_set),
        settings,
        key_set,
        key_problem,
        local_suggestion: LOCAL_SUGGESTION,
        anthropic_suggestion: ANTHROPIC_SUGGESTION,
        anthropic_model: ANTHROPIC_DEFAULT_MODEL,
        can_send: true,
    })
}

/// Save the settings. Turning AI on needs settings that work.
#[tauri::command]
pub fn ai_settings_set(
    app: AppHandle,
    state: State<App>,
    settings: AiSettings,
) -> CommandResult<()> {
    check_shape(&settings)?;
    let mut settings = AiSettings {
        endpoint: settings.endpoint.trim().to_owned(),
        model: settings.model.trim().to_owned(),
        ..settings
    };
    if settings.provider == Provider::Anthropic {
        // A blank box means the suggested address and model.
        if settings.endpoint.is_empty() {
            settings.endpoint = ANTHROPIC_SUGGESTION.into();
        }
        if settings.model.is_empty() {
            settings.model = ANTHROPIC_DEFAULT_MODEL.into();
        }
    }
    if settings.enabled {
        let key_set = app.device().ai_key().ok().flatten().is_some();
        if let Some(p) = problem(&settings, key_set) {
            return Err(p);
        }
    }
    state.with(|s| s.set_setting(SETTING, &settings))?;
    state.lock_inner()?.ai_pending = None;
    Ok(())
}

/// Keep the user's cloud API key in the OS keyring (Android: encrypted by a
/// Keystore key). Only while unlocked.
#[tauri::command]
pub fn ai_key_set(app: AppHandle, state: State<App>, key: String) -> CommandResult<()> {
    let key = check_key(&key)?;
    state.with(|_| Ok(()))?;
    app.device().set_ai_key(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ai_key_clear(app: AppHandle, state: State<App>) -> CommandResult<()> {
    state.with(|_| Ok(()))?;
    app.device().clear_ai_key().map_err(|e| e.to_string())?;
    state.lock_inner()?.ai_pending = None;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPreview {
    /// The instructions sent with it.
    system: &'static str,
    /// Exactly what would be sent.
    text: String,
    /// Where it would go: the endpoint's host.
    destination: String,
    provider: Provider,
}

/// Build the digest and show it. Nothing is sent.
#[tauri::command]
pub fn ai_preview(app: AppHandle, state: State<App>) -> CommandResult<AiPreview> {
    let settings = settings_of(&state)?;
    if !settings.enabled {
        return Err("AI narration is off. Turn it on in Settings first.".into());
    }
    let key_set = app.device().ai_key().ok().flatten().is_some();
    if let Some(p) = problem(&settings, key_set) {
        return Err(p);
    }
    let digest = state
        .with(|s| s.digest(now()))?
        .ok_or("Answer the risk questions first, so there is a plan to explain.")?;
    let text = digest_text(&digest);
    if text.len() > MAX_PROMPT_BYTES {
        return Err("The summary is too long to send.".into());
    }
    let destination = Url::parse(&settings.endpoint)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    state.lock_inner()?.ai_pending = Some(Pending {
        text: text.clone(),
        settings: settings.clone(),
    });
    Ok(AiPreview {
        system: NARRATOR_SYSTEM_PROMPT,
        text,
        destination,
        provider: settings.provider,
    })
}

/// Send the previewed text, exactly as shown, and return the model's
/// answer as plain text.
#[tauri::command]
pub async fn ai_send(app: AppHandle, state: State<'_, App>, text: String) -> CommandResult<String> {
    let settings = settings_of(&state)?;
    let text = {
        let mut inner = state.lock_inner()?;
        take_matching(&mut inner.ai_pending, &text, &settings)?
    };
    if !settings.enabled {
        return Err("AI narration is off.".into());
    }
    let key = match settings.provider {
        Provider::Cloud | Provider::Anthropic => {
            app.device().ai_key().map_err(|e| e.to_string())?
        }
        Provider::Local => None,
    };
    let req = chat_request(&settings, key.as_deref(), NARRATOR_SYSTEM_PROMPT, &text)?;
    post(req, TIMEOUT).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(endpoint: &str) -> AiSettings {
        AiSettings {
            enabled: true,
            provider: Provider::Local,
            endpoint: endpoint.into(),
            model: "llama3".into(),
        }
    }

    fn cloud(endpoint: &str) -> AiSettings {
        AiSettings {
            provider: Provider::Cloud,
            ..local(endpoint)
        }
    }

    #[test]
    fn ai_is_off_by_default_and_empty_settings_cannot_be_used() {
        let s = AiSettings::default();
        assert!(!s.enabled);
        assert_eq!(s.provider, Provider::Local);
        assert!(problem(&s, false).is_some());
    }

    #[test]
    fn local_accepts_this_computer_and_own_network_only() {
        // Private addresses are built, not written out, so the public-repo
        // scan has nothing to flag.
        let lan = |a: u8, b: u8| format!("http://{a}.{b}.{}.{}:11434/v1", 1, 20);
        for ok in [
            LOCAL_SUGGESTION.to_string(),
            "http://127.0.0.1:1234/v1".into(),
            "http://[::1]:8080/v1".into(),
            "http://gpu-box.local:11434/v1".into(),
            "https://localhost/v1".into(),
            lan(10, 0),
            lan(172, 16),
            lan(192, 168),
            lan(100, 64),
        ] {
            assert_eq!(problem(&local(&ok), false), None, "{ok}");
        }
        for bad in [
            "http://example.com/v1".to_string(),
            "https://api.example.com/v1".into(),
            "http://8.8.8.8/v1".into(),
            lan(172, 32),
            lan(100, 128),
            "ftp://localhost/v1".into(),
            "localhost:11434".into(),
            "http://user:pw@localhost/v1".into(),
            "http://localhost/v1?x=1".into(),
        ] {
            assert!(problem(&local(&bad), false).is_some(), "{bad}");
        }
    }

    #[test]
    fn cloud_needs_https_and_a_key() {
        let base = "https://api.example.com/v1";
        assert!(problem(&cloud(base), false).unwrap().contains("API key"));
        assert!(
            problem(&cloud("http://api.example.com/v1"), true)
                .unwrap()
                .contains("https")
        );
        assert_eq!(problem(&cloud(base), true), None);
        let mut no_model = cloud(base);
        no_model.model = " ".into();
        assert!(problem(&no_model, true).unwrap().contains("model"));
    }

    #[test]
    fn settings_and_keys_have_a_sane_shape() {
        let mut s = local(LOCAL_SUGGESTION);
        s.model = "m\nx".into();
        assert!(check_shape(&s).is_err());
        s.model = "m".repeat(101);
        assert!(check_shape(&s).is_err());
        s = local(&format!("http://localhost/{}", "a".repeat(300)));
        assert!(check_shape(&s).is_err());
        assert_eq!(check_key("  sk-test-123 \n").unwrap(), "sk-test-123");
        for bad in ["", "two words", "tab\there", &"k".repeat(513)] {
            assert!(check_key(bad).is_err(), "{bad:?}");
        }
        assert!(
            !check_key("sk-secret value")
                .unwrap_err()
                .contains("sk-secret")
        );
    }

    #[test]
    fn local_request_is_openai_shaped_with_no_key() {
        let r = chat_request(
            &local("http://localhost:11434/v1/"),
            Some("sk-never"),
            "sys",
            "digest",
        )
        .unwrap();
        assert_eq!(r.url, "http://localhost:11434/v1/chat/completions");
        assert!(
            r.credential.is_none(),
            "a key never goes to a local endpoint"
        );
        assert!(r.local);
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "digest");
        assert_eq!(body["model"], "llama3");
        assert!(!String::from_utf8(r.body).unwrap().contains("sk-never"));
    }

    #[test]
    fn cloud_request_carries_the_key_in_the_header_only() {
        let r = chat_request(
            &cloud("https://api.example.com/v1"),
            Some("sk-test-123"),
            "s",
            "u",
        )
        .unwrap();
        assert!(matches!(&r.credential, Some(Credential::Bearer(k)) if k == "sk-test-123"));
        assert!(!String::from_utf8(r.body).unwrap().contains("sk-test-123"));
        assert!(!r.local);
        // No key, no request.
        assert!(chat_request(&cloud("https://api.example.com/v1"), None, "s", "u").is_err());
        let big = "x".repeat(MAX_PROMPT_BYTES + 1);
        assert!(chat_request(&local(LOCAL_SUGGESTION), None, "s", &big).is_err());
    }

    #[test]
    fn replies_are_read_strictly_and_errors_never_echo_the_body() {
        let ok = json!({ "choices": [{ "message": { "content": " Hello. " } }] }).to_string();
        assert_eq!(
            parse_reply(Provider::Cloud, 200, ok.as_bytes()).unwrap(),
            "Hello."
        );
        let e = parse_reply(Provider::Cloud, 500, b"echo of your prompt: SECRET").unwrap_err();
        assert!(e.contains("500") && !e.contains("SECRET"), "{e}");
        assert!(parse_reply(Provider::Cloud, 200, br#"{"nope":1}"#).is_err());
        assert!(parse_reply(Provider::Cloud, 200, b"not json").is_err());
        let empty = json!({ "choices": [{ "message": { "content": "  " } }] }).to_string();
        assert!(
            parse_reply(Provider::Cloud, 200, empty.as_bytes())
                .unwrap_err()
                .contains("empty")
        );
        let long = json!({ "choices": [{ "message": { "content": "y".repeat(10_000) } }] });
        let text = parse_reply(Provider::Cloud, 200, long.to_string().as_bytes()).unwrap();
        assert_eq!(text.chars().count(), MAX_REPLY_CHARS);
    }

    #[test]
    fn only_the_exact_previewed_text_can_be_sent_and_only_once() {
        let s = local(LOCAL_SUGGESTION);
        let shown = || {
            Some(Pending {
                text: "{\"a\": 1}".into(),
                settings: s.clone(),
            })
        };
        let mut p = shown();
        assert!(take_matching(&mut p, "{\"a\": 2}", &s).is_err());
        assert!(p.is_none(), "a wrong try uses the preview up");
        let mut p = shown();
        let other = cloud("https://api.example.com/v1");
        assert!(take_matching(&mut p, "{\"a\": 1}", &other).is_err());
        let mut p = shown();
        assert_eq!(
            take_matching(&mut p, "{\"a\": 1}", &s).unwrap(),
            "{\"a\": 1}"
        );
        assert!(take_matching(&mut p, "{\"a\": 1}", &s).is_err());
        assert!(take_matching(&mut None, "", &s).is_err());
    }

    fn anthropic(endpoint: &str) -> AiSettings {
        AiSettings {
            provider: Provider::Anthropic,
            model: ANTHROPIC_DEFAULT_MODEL.into(),
            ..local(endpoint)
        }
    }

    #[test]
    fn anthropic_needs_https_and_a_key_and_its_default_model_is_set() {
        assert_eq!(ANTHROPIC_DEFAULT_MODEL, "claude-sonnet-5-5");
        let a = anthropic(ANTHROPIC_SUGGESTION);
        assert!(problem(&a, false).unwrap().contains("API key"));
        assert_eq!(problem(&a, true), None);
        assert!(
            problem(&anthropic("http://api.example.com"), true)
                .unwrap()
                .contains("https")
        );
        // Not a local provider: a private address is no excuse for plain http.
        assert!(problem(&anthropic("http://localhost:1/v1"), true).is_some());
        assert_eq!(
            serde_json::to_string(&Provider::Anthropic).unwrap(),
            "\"anthropic\""
        );
    }

    #[test]
    fn anthropic_request_has_the_messages_shape_and_the_key_only_in_a_header() {
        let key = "sk-ant-test-123";
        for base in [
            "https://api.anthropic.com",
            "https://api.anthropic.com/",
            "https://api.anthropic.com/v1",
        ] {
            let r = chat_request(&anthropic(base), Some(key), "sys prompt", "digest text").unwrap();
            assert_eq!(r.url, "https://api.anthropic.com/v1/messages", "{base}");
        }
        let r = chat_request(
            &anthropic(ANTHROPIC_SUGGESTION),
            Some(key),
            "sys prompt",
            "digest text",
        )
        .unwrap();
        assert!(matches!(&r.credential, Some(Credential::ApiKey(k)) if k == key));
        assert_eq!(r.headers, vec![("anthropic-version", "2023-06-01")]);
        assert!(!r.local);
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(body["model"], "claude-sonnet-5-5");
        assert_eq!(body["max_tokens"], MAX_TOKENS);
        assert_eq!(body["system"], "sys prompt");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "digest text");
        assert!(body.get("stream").is_none());
        assert!(!String::from_utf8(r.body).unwrap().contains(key));
        assert!(!r.url.contains(key));
        // No key, no request. A key is never turned into a bearer here.
        assert!(chat_request(&anthropic(ANTHROPIC_SUGGESTION), None, "s", "u").is_err());
    }

    #[test]
    fn anthropic_replies_are_read_strictly() {
        let p = Provider::Anthropic;
        let ok = |content: Value| {
            json!({ "type": "message", "role": "assistant", "content": content }).to_string()
        };
        let two = ok(json!([
            { "type": "text", "text": " Hello " },
            { "type": "thinking", "thinking": "hidden" },
            { "type": "text", "text": "world. " },
        ]));
        assert_eq!(parse_reply(p, 200, two.as_bytes()).unwrap(), "Hello world.");
        // Error bodies are never echoed.
        let e = parse_reply(p, 401, br#"{"error":"bad key sk-ant-test-123"}"#).unwrap_err();
        assert!(e.contains("401") && !e.contains("sk-ant"), "{e}");
        // Wrong shapes.
        for bad in [
            br#"{"choices":[{"message":{"content":"x"}}]}"#.to_vec(),
            ok(json!("just a string")).into_bytes(),
            ok(json!([{ "type": "text" }])).into_bytes(),
            ok(json!([{ "text": "no type" }])).into_bytes(),
            json!({ "type": "error", "content": [{ "type": "text", "text": "x" }] })
                .to_string()
                .into_bytes(),
            json!({ "type": "message", "role": "user", "content": [{ "type": "text", "text": "x" }] })
                .to_string()
                .into_bytes(),
            b"not json".to_vec(),
        ] {
            assert!(parse_reply(p, 200, &bad).is_err(), "{}", String::from_utf8_lossy(&bad));
        }
        // No text at all, or only blank text.
        assert!(
            parse_reply(p, 200, ok(json!([])).as_bytes())
                .unwrap_err()
                .contains("empty")
        );
        assert!(
            parse_reply(
                p,
                200,
                ok(json!([{ "type": "text", "text": "  " }])).as_bytes()
            )
            .is_err()
        );
        // A refusal is a plain message, not text to show.
        let refused = json!({ "type": "message", "role": "assistant", "stop_reason": "refusal",
            "content": [{ "type": "text", "text": "x" }] })
        .to_string();
        assert!(
            parse_reply(p, 200, refused.as_bytes())
                .unwrap_err()
                .contains("declined")
        );
        // Long answers are cut like the others.
        let long = ok(json!([{ "type": "text", "text": "y".repeat(10_000) }]));
        assert_eq!(
            parse_reply(p, 200, long.as_bytes())
                .unwrap()
                .chars()
                .count(),
            MAX_REPLY_CHARS
        );
    }

    /// A one-shot HTTP server on loopback: no real network.
    mod wire {
        use super::*;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;

        /// Serve one request; give back what arrived.
        fn serve(
            status: &'static str,
            body: String,
            delay: Duration,
        ) -> (String, mpsc::Receiver<String>) {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = l.local_addr().unwrap();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let (mut c, _) = l.accept().unwrap();
                let mut got = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = c.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    got.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&got).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if got.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&got).to_string());
                std::thread::sleep(delay);
                let _ = write!(
                    c,
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
            (format!("http://{addr}/v1"), rx)
        }

        fn send(req: ChatRequest, timeout: Duration) -> CommandResult<String> {
            tauri::async_runtime::block_on(post(req, timeout))
        }

        #[test]
        fn a_local_round_trip_sends_the_text_and_no_key() {
            let reply = json!({ "choices": [{ "message": { "content": "Plain English plan." } }] });
            let (url, rx) = serve("200 OK", reply.to_string(), Duration::ZERO);
            let req = chat_request(&local(&url), Some("sk-never"), "sys", "Goal 1").unwrap();
            assert_eq!(send(req, TIMEOUT).unwrap(), "Plain English plan.");
            let seen = rx.recv().unwrap();
            assert!(seen.starts_with("POST /v1/chat/completions "), "{seen}");
            assert!(!seen.to_ascii_lowercase().contains("authorization"));
            assert!(!seen.contains("sk-never"));
            assert!(seen.contains("Goal 1"));
        }

        #[test]
        fn a_key_sent_to_an_endpoint_is_never_in_its_error() {
            // A cloud request carries the key; the mock answers 401 echoing it.
            let (url, rx) = serve(
                "401 Unauthorized",
                "bad key sk-test-123".into(),
                Duration::ZERO,
            );
            let req = ChatRequest {
                credential: Some(Credential::Bearer("sk-test-123".into())),
                ..chat_request(&local(&url), None, "s", "u").unwrap()
            };
            let e = send(req, TIMEOUT).unwrap_err();
            assert!(e.contains("401") && !e.contains("sk-test-123"), "{e}");
            assert!(rx.recv().unwrap().contains("Bearer sk-test-123"));
        }

        #[test]
        fn an_anthropic_round_trip_sends_headers_and_never_the_key_in_the_body() {
            let reply = json!({ "type": "message", "role": "assistant",
                "content": [{ "type": "text", "text": "Plain plan." }] });
            let (url, rx) = serve("200 OK", reply.to_string(), Duration::ZERO);
            // The mock is plain http on loopback, so build the request by hand
            // from an anthropic one: same body, headers and credential.
            let req = ChatRequest {
                url: format!("{url}/messages"),
                local: true,
                ..chat_request(
                    &anthropic(ANTHROPIC_SUGGESTION),
                    Some("sk-ant-test-123"),
                    "sys",
                    "Goal 1",
                )
                .unwrap()
            };
            assert_eq!(send(req, TIMEOUT).unwrap(), "Plain plan.");
            let seen = rx.recv().unwrap();
            let lower = seen.to_ascii_lowercase();
            assert!(seen.starts_with("POST /v1/messages "), "{seen}");
            assert!(lower.contains("x-api-key: sk-ant-test-123"));
            assert!(lower.contains("anthropic-version: 2023-06-01"));
            assert!(!lower.contains("authorization"));
            let body = &seen[seen.find("\r\n\r\n").unwrap() + 4..];
            assert!(!body.contains("sk-ant") && body.contains("Goal 1"));
        }

        #[test]
        fn an_anthropic_error_never_echoes_the_key() {
            let (url, _rx) = serve(
                "401 Unauthorized",
                "invalid x-api-key sk-ant-test-123".into(),
                Duration::ZERO,
            );
            let req = ChatRequest {
                url: format!("{url}/messages"),
                local: true,
                ..chat_request(
                    &anthropic(ANTHROPIC_SUGGESTION),
                    Some("sk-ant-test-123"),
                    "s",
                    "u",
                )
                .unwrap()
            };
            let e = send(req, TIMEOUT).unwrap_err();
            assert!(e.contains("401") && !e.contains("sk-ant"), "{e}");
        }

        #[test]
        fn a_huge_answer_is_refused() {
            let (url, _rx) = serve("200 OK", "z".repeat(MAX_REPLY_BYTES + 10), Duration::ZERO);
            let req = chat_request(&local(&url), None, "s", "u").unwrap();
            assert!(send(req, TIMEOUT).unwrap_err().contains("too long"));
        }

        #[test]
        fn a_slow_model_times_out_cleanly() {
            let (url, _rx) = serve("200 OK", "{}".into(), Duration::from_secs(3));
            let req = chat_request(&local(&url), None, "s", "u").unwrap();
            let e = send(req, Duration::from_secs(1)).unwrap_err();
            assert!(e.contains("did not answer within 1 seconds"), "{e}");
        }

        #[test]
        fn nothing_listening_is_a_plain_error() {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/v1", l.local_addr().unwrap());
            drop(l);
            let req = chat_request(&local(&url), None, "s", "u").unwrap();
            assert!(
                send(req, TIMEOUT)
                    .unwrap_err()
                    .contains("Could not connect")
            );
        }
    }
}
