use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::json;

#[derive(Clone, Debug, Default)]
pub struct ApiKeyConfig(pub Option<String>);

impl ApiKeyConfig {
    pub fn required(key: impl Into<String>) -> Self {
        let trimmed = key.into().trim().to_string();
        if trimmed.is_empty() {
            Self(None)
        } else {
            Self(Some(trimmed))
        }
    }

    pub fn disabled() -> Self {
        Self(None)
    }

    pub fn expected(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn extract_api_key(req: &HttpRequest) -> Option<String> {
    if let Some(val) = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Some(val.to_string());
    }

    if let Some(auth) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let token = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
            .unwrap_or(auth)
            .trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }

    if let Some(cookie_header) = req.headers().get("cookie").and_then(|v| v.to_str().ok()) {
        for part in cookie_header.split(';') {
            if let Some((k, v)) = part.trim().split_once('=') {
                let key = k.trim();
                if key == "courseviewer_api_key" || key == "api_key" {
                    let decoded = percent_decode(v.trim());
                    if !decoded.trim().is_empty() {
                        return Some(decoded.trim().to_string());
                    }
                }
            }
        }
    }

    for part in req.query_string().split('&') {
        if let Some((k, v)) = part.split_once('=') {
            if k == "api_key" {
                let decoded = percent_decode(v);
                if !decoded.trim().is_empty() {
                    return Some(decoded.trim().to_string());
                }
            }
        }
    }

    None
}

pub fn is_authorized(req: &HttpRequest, expected: Option<&str>) -> bool {
    let Some(expected) = expected.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let Some(supplied) = extract_api_key(req) else {
        return false;
    };
    constant_time_eq(supplied.as_bytes(), expected.as_bytes())
}

pub fn is_request_authorized(req: &HttpRequest) -> bool {
    let expected = req
        .app_data::<web::Data<ApiKeyConfig>>()
        .and_then(|cfg| cfg.expected());
    is_authorized(req, expected)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/auth/verify", web::get().to(verify_auth))
        .route("/api/auth/verify", web::post().to(verify_auth));
}

async fn verify_auth(req: HttpRequest) -> HttpResponse {
    if is_request_authorized(&req) {
        HttpResponse::Ok().json(json!({ "authenticated": true }))
    } else {
        HttpResponse::Unauthorized().json(json!({
            "authenticated": false,
            "error": "Invalid or missing API key."
        }))
    }
}
