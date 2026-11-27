//! Shotbolt: upload to a user-defined HTTP API.
//!
//! The file goes out as one `multipart/form-data` POST to
//! `custom_upload_url`, in the field `custom_upload_field`. An optional token
//! rides in `custom_upload_auth_header`: the `Authorization` header gets
//! `Bearer <token>`, any other header the token as-is. The share link is read
//! from the JSON response at the dotted path `custom_upload_url_key`
//! (`url`, `data.link`, `files.0.url`); a plain-text response that is itself a
//! link is accepted too.

use std::path::Path;

use crate::config::AppConfig;

use super::upload::{
    build_multipart, guess_content_type, multipart_boundary, ProgressReader, UploadError,
    UploadProgress, UploadResult,
};

const MAX_ERROR_BODY_CHARS: usize = 300;

pub(crate) fn is_configured(config: &AppConfig) -> bool {
    let url = config.custom_upload_url.trim();
    url.starts_with("http://") || url.starts_with("https://")
}

pub(crate) fn not_configured_notification(_config: &AppConfig) -> (String, String) {
    (
        crate::i18n::t("Upload API not configured"),
        crate::i18n::t("Set the upload URL of your API in Settings → Cloud."),
    )
}

/// The header name and value that carry the token, or `None` without a token.
fn auth_header(config: &AppConfig) -> Option<(String, String)> {
    let token = config.custom_upload_token.trim();
    if token.is_empty() {
        return None;
    }
    let header = match config.custom_upload_auth_header.trim() {
        "" => "Authorization",
        header => header,
    };
    let value = if header.eq_ignore_ascii_case("authorization") && !token.contains(' ') {
        format!("Bearer {token}")
    } else {
        token.to_string()
    };
    Some((header.to_string(), value))
}

fn field_name(config: &AppConfig) -> &str {
    match config.custom_upload_field.trim() {
        "" => "file",
        field => field,
    }
}

fn request(config: &AppConfig, boundary: &str) -> ureq::Request {
    let mut req = ureq::post(config.custom_upload_url.trim())
        .set("Accept", "application/json, text/plain;q=0.9, */*;q=0.5")
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        );
    if let Some((name, value)) = auth_header(config) {
        req = req.set(&name, &value);
    }
    req
}

pub(crate) fn upload(
    config: &AppConfig,
    path: &Path,
    progress: UploadProgress<'_>,
) -> Result<UploadResult, UploadError> {
    if !is_configured(config) {
        return Err(UploadError::NotConfigured(
            "Upload API not configured. Set the upload URL in Settings.".to_string(),
        ));
    }

    let file_bytes = std::fs::read(path).map_err(|e| UploadError::FileRead(e.to_string()))?;
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("upload")
        .to_string();
    let content_type = guess_content_type(&filename);

    let boundary = multipart_boundary();
    let body = build_multipart(
        &boundary,
        field_name(config),
        &filename,
        &content_type,
        &file_bytes,
    );

    let response = request(config, &boundary)
        .set("Content-Length", &body.len().to_string())
        .send(ProgressReader::new(&body, progress))
        .map_err(map_http_error)?;

    let text = response
        .into_string()
        .map_err(|e| UploadError::Server(format!("Could not read the response: {e}")))?;

    let share_url = extract_share_url(&text, &config.custom_upload_url_key).ok_or_else(|| {
        UploadError::Server(format!(
            "The API response did not contain a link at \"{}\": {}",
            config.custom_upload_url_key.trim(),
            shorten(&text)
        ))
    })?;

    Ok(UploadResult { share_url })
}

/// Sends a tiny text file the same way a real upload goes out, so the user
/// sees at once whether URL, token and field name are accepted.
pub fn test_connection(config: &AppConfig) -> Result<String, String> {
    if !is_configured(config) {
        return Err("Enter an upload URL starting with http:// or https://.".into());
    }
    let boundary = multipart_boundary();
    let body = build_multipart(
        &boundary,
        field_name(config),
        "shotbolt-test.txt",
        "text/plain",
        b"Shotbolt connection test",
    );
    match request(config, &boundary).send_bytes(&body) {
        Ok(response) => {
            let text = response.into_string().unwrap_or_default();
            match extract_share_url(&text, &config.custom_upload_url_key) {
                Some(url) => Ok(url),
                None => Err(format!(
                    "Connected, but the response has no link at \"{}\": {}",
                    config.custom_upload_url_key.trim(),
                    shorten(&text)
                )),
            }
        }
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            Err("Token rejected. Check the token and header name.".into())
        }
        Err(ureq::Error::Status(404, _)) => Err("Upload URL not found (HTTP 404).".into()),
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(format!("HTTP {code}: {}", shorten(&body)))
        }
        Err(e) => Err(format!("Could not reach the API: {e}")),
    }
}

fn map_http_error(e: ureq::Error) -> UploadError {
    match e {
        ureq::Error::Status(401, _) | ureq::Error::Status(403, _) => UploadError::AuthExpired(
            "Your upload API rejected the token. Update it in Settings \u{2192} Cloud.".to_string(),
        ),
        ureq::Error::Status(413, _) => {
            UploadError::Server("The file is too large for your upload API.".to_string())
        }
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let trimmed = body.trim();
            if trimmed.is_empty() {
                UploadError::Server(format!("HTTP {code}"))
            } else {
                UploadError::Server(format!("HTTP {code}: {}", shorten(trimmed)))
            }
        }
        e => UploadError::HttpRequest(e.to_string()),
    }
}

/// Finds the share link in a response body: the JSON value at `key_path`
/// (dot-separated, numeric parts index arrays), else the whole body when it
/// is a bare link.
pub(crate) fn extract_share_url(body: &str, key_path: &str) -> Option<String> {
    let body = body.trim();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let key_path = match key_path.trim() {
            "" => "url",
            key => key,
        };
        let mut node = &json;
        for part in key_path.split('.').filter(|p| !p.is_empty()) {
            node = match node {
                serde_json::Value::Object(map) => map.get(part)?,
                serde_json::Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        return node
            .as_str()
            .map(str::trim)
            .filter(|url| looks_like_link(url))
            .map(str::to_string);
    }
    let first_line = body.lines().next().unwrap_or("").trim();
    (looks_like_link(first_line) && !first_line.contains(char::is_whitespace))
        .then(|| first_line.to_string())
}

fn looks_like_link(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn shorten(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return "(empty response)".to_string();
    }
    let mut short: String = text.chars().take(MAX_ERROR_BODY_CHARS).collect();
    if text.chars().count() > MAX_ERROR_BODY_CHARS {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(url: &str, token: &str, header: &str) -> AppConfig {
        AppConfig {
            cloud_destination: "custom".into(),
            custom_upload_url: url.into(),
            custom_upload_token: token.into(),
            custom_upload_auth_header: header.into(),
            ..AppConfig::default()
        }
    }

    #[test]
    fn is_configured_needs_an_http_url() {
        assert!(!is_configured(&config("", "", "Authorization")));
        assert!(!is_configured(&config("ftp://x", "", "Authorization")));
        assert!(is_configured(&config("https://api.example/upload", "", "")));
        assert!(is_configured(&config("http://localhost:8080/up", "", "")));
    }

    #[test]
    fn authorization_header_gets_a_bearer_prefix() {
        let cfg = config("https://a", "secret", "Authorization");
        assert_eq!(
            auth_header(&cfg),
            Some(("Authorization".into(), "Bearer secret".into()))
        );
    }

    #[test]
    fn authorization_value_with_a_scheme_is_sent_unchanged() {
        let cfg = config("https://a", "Basic dXNlcjpwYXNz", "Authorization");
        assert_eq!(
            auth_header(&cfg),
            Some(("Authorization".into(), "Basic dXNlcjpwYXNz".into()))
        );
    }

    #[test]
    fn other_headers_get_the_raw_token() {
        let cfg = config("https://a", "k123", "X-API-Key");
        assert_eq!(auth_header(&cfg), Some(("X-API-Key".into(), "k123".into())));
    }

    #[test]
    fn no_token_means_no_auth_header() {
        assert_eq!(
            auth_header(&config("https://a", "  ", "Authorization")),
            None
        );
    }

    #[test]
    fn extracts_top_level_url() {
        let body = r#"{"url":"https://cdn.example/a.png"}"#;
        assert_eq!(
            extract_share_url(body, "url").as_deref(),
            Some("https://cdn.example/a.png")
        );
    }

    #[test]
    fn extracts_nested_and_array_paths() {
        let body =
            r#"{"data":{"link":"https://i.example/x"},"files":[{"url":"https://f.example/1"}]}"#;
        assert_eq!(
            extract_share_url(body, "data.link").as_deref(),
            Some("https://i.example/x")
        );
        assert_eq!(
            extract_share_url(body, "files.0.url").as_deref(),
            Some("https://f.example/1")
        );
    }

    #[test]
    fn missing_or_non_link_json_values_are_rejected() {
        assert_eq!(extract_share_url(r#"{"ok":true}"#, "url"), None);
        assert_eq!(extract_share_url(r#"{"url":"not a link"}"#, "url"), None);
        assert_eq!(extract_share_url(r#"{"url":42}"#, "url"), None);
    }

    #[test]
    fn plain_text_link_is_accepted() {
        assert_eq!(
            extract_share_url("https://paste.example/abc\n", "url").as_deref(),
            Some("https://paste.example/abc")
        );
        assert_eq!(extract_share_url("Upload failed", "url"), None);
    }

    #[test]
    fn long_error_bodies_are_shortened() {
        let long = "x".repeat(1000);
        let short = shorten(&long);
        assert_eq!(short.chars().count(), MAX_ERROR_BODY_CHARS + 1);
        assert_eq!(shorten("   "), "(empty response)");
    }
}
