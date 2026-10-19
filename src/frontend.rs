use std::io::SeekFrom;
use std::path::{Component, Path, PathBuf};

use actix_web::{HttpRequest, HttpResponse, web};
use futures_util::stream;
use include_dir::{Dir, include_dir};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::auth::is_request_authorized;

static EMBEDDED_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/frontend/dist");

#[derive(Clone, Debug)]
pub enum FrontendMode {
    Development { proxy_url: String },
    Embedded { dist_dir: Option<PathBuf> },
}

#[derive(Clone)]
pub struct FrontendConfig {
    pub mode: FrontendMode,
    pub downloads_dir: PathBuf,
    pub proxy_client: reqwest::Client,
}

impl FrontendConfig {
    pub fn new(is_dev: bool, downloads_dir: PathBuf) -> Self {
        let mode = if is_dev {
            let proxy_url = crate::env_var("FRONTEND_DEV_URL")
                .unwrap_or_else(|| "http://localhost:3000".to_string());
            FrontendMode::Development { proxy_url }
        } else {
            let dist_path = PathBuf::from("frontend/dist");
            let dist_dir = if dist_path.join("index.html").is_file() {
                Some(dist_path)
            } else {
                None
            };
            FrontendMode::Embedded { dist_dir }
        };
        Self {
            mode,
            downloads_dir,
            proxy_client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("proxy HTTP client"),
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/local-media/{tail:.*}", web::get().to(serve_local_media))
        .route("/local-media/{tail:.*}", web::head().to(serve_local_media))
        .default_service(web::route().to(serve_frontend));
}

fn mime_type_for_path(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "vtt" => "text/vtt; charset=utf-8",
        "srt" | "txt" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn sanitize_relative_path(raw: &str) -> Option<PathBuf> {
    let mut clean = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if clean.as_os_str().is_empty() {
        None
    } else {
        Some(clean)
    }
}

async fn serve_local_media(
    req: HttpRequest,
    tail: web::Path<String>,
    config: web::Data<FrontendConfig>,
) -> HttpResponse {
    if !is_request_authorized(&req) {
        return HttpResponse::Unauthorized().body("Unauthorized");
    }

    let Some(rel_path) = sanitize_relative_path(&tail.into_inner()) else {
        return HttpResponse::NotFound().body("Not found");
    };

    let full_path = config.downloads_dir.join(&rel_path);
    let Ok(metadata) = tokio::fs::metadata(&full_path).await else {
        return HttpResponse::NotFound().body("Not found");
    };
    if !metadata.is_file() {
        return HttpResponse::NotFound().body("Not found");
    }

    let file_size = metadata.len();
    let content_type = mime_type_for_path(&full_path.to_string_lossy());
    let is_head = req.method() == actix_web::http::Method::HEAD;

    if let Some(range_header) = req.headers().get("range").and_then(|h| h.to_str().ok()) {
        if let Some(range_spec) = range_header.strip_prefix("bytes=") {
            let Some((start_str, end_str)) = range_spec.split_once('-') else {
                return HttpResponse::RangeNotSatisfiable()
                    .insert_header(("Content-Range", format!("bytes */{file_size}")))
                    .finish();
            };
            let start = if start_str.is_empty() {
                0
            } else {
                match start_str.parse::<u64>() {
                    Ok(v) => v,
                    Err(_) => {
                        return HttpResponse::RangeNotSatisfiable()
                            .insert_header(("Content-Range", format!("bytes */{file_size}")))
                            .finish();
                    }
                }
            };
            let end = if end_str.is_empty() {
                file_size.saturating_sub(1)
            } else {
                match end_str.parse::<u64>() {
                    Ok(v) => v.min(file_size.saturating_sub(1)),
                    Err(_) => {
                        return HttpResponse::RangeNotSatisfiable()
                            .insert_header(("Content-Range", format!("bytes */{file_size}")))
                            .finish();
                    }
                }
            };

            if file_size == 0 || start >= file_size || start > end {
                return HttpResponse::RangeNotSatisfiable()
                    .insert_header(("Content-Range", format!("bytes */{file_size}")))
                    .finish();
            }

            let length = end - start + 1;
            let mut file = match tokio::fs::File::open(&full_path).await {
                Ok(f) => f,
                Err(_) => return HttpResponse::InternalServerError().finish(),
            };
            if file.seek(SeekFrom::Start(start)).await.is_err() {
                return HttpResponse::InternalServerError().finish();
            }

            let mut builder = HttpResponse::PartialContent();
            builder
                .insert_header(("Accept-Ranges", "bytes"))
                .insert_header(("Content-Type", content_type))
                .insert_header(("Content-Range", format!("bytes {start}-{end}/{file_size}")))
                .insert_header(("Content-Length", length.to_string()));

            if is_head {
                return builder.finish();
            }

            return builder.streaming(file_range_stream(file, length));
        }
    }

    let file = match tokio::fs::File::open(&full_path).await {
        Ok(f) => f,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let mut builder = HttpResponse::Ok();
    builder
        .insert_header(("Accept-Ranges", "bytes"))
        .insert_header(("Content-Type", content_type))
        .insert_header(("Content-Length", file_size.to_string()));

    if is_head {
        return builder.finish();
    }

    builder.streaming(file_range_stream(file, file_size))
}

fn file_range_stream(
    file: tokio::fs::File,
    total_bytes: u64,
) -> impl futures_util::Stream<Item = Result<actix_web::web::Bytes, std::io::Error>> {
    stream::unfold((file, total_bytes), |(mut file, remaining)| async move {
        if remaining == 0 {
            return None;
        }
        let to_read = remaining.min(64 * 1024) as usize;
        let mut buf = vec![0u8; to_read];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((
                    Ok(actix_web::web::Bytes::from(buf)),
                    (file, remaining - n as u64),
                ))
            }
            Err(e) => Some((Err(e), (file, 0))),
        }
    })
}

async fn serve_frontend(
    req: HttpRequest,
    body: web::Bytes,
    config: web::Data<FrontendConfig>,
) -> HttpResponse {
    match &config.mode {
        FrontendMode::Development { proxy_url } => {
            proxy_to_dev_server(&req, body, proxy_url, &config.proxy_client).await
        }
        FrontendMode::Embedded { dist_dir } => {
            serve_embedded_or_dist(&req, dist_dir.as_deref()).await
        }
    }
}

async fn proxy_to_dev_server(
    req: &HttpRequest,
    body: web::Bytes,
    proxy_url: &str,
    client: &reqwest::Client,
) -> HttpResponse {
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let target_url = format!("{}{}", proxy_url.trim_end_matches('/'), path_and_query);

    let Ok(method) = reqwest::Method::from_bytes(req.method().as_str().as_bytes()) else {
        return HttpResponse::MethodNotAllowed().finish();
    };

    let mut builder = client.request(method, &target_url);
    for (name, value) in req.headers() {
        let key = name.as_str();
        if !key.eq_ignore_ascii_case("host") && !key.eq_ignore_ascii_case("connection") {
            builder = builder.header(key, value.as_bytes());
        }
    }
    if !body.is_empty() {
        builder = builder.body(body.to_vec());
    }

    match builder.send().await {
        Ok(resp) => {
            let status = actix_web::http::StatusCode::from_u16(resp.status().as_u16())
                .unwrap_or(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR);
            let mut response = HttpResponse::build(status);
            for (name, value) in resp.headers() {
                let key = name.as_str();
                if !key.eq_ignore_ascii_case("transfer-encoding")
                    && !key.eq_ignore_ascii_case("connection")
                {
                    response.append_header((key, value.as_bytes()));
                }
            }
            match resp.bytes().await {
                Ok(bytes) => response.body(bytes),
                Err(_) => HttpResponse::BadGateway().body("Error reading from frontend dev server"),
            }
        }
        Err(err) => HttpResponse::BadGateway().body(format!(
            "Could not reach frontend dev server at {proxy_url}: {err}"
        )),
    }
}

async fn serve_embedded_or_dist(req: &HttpRequest, dist_dir: Option<&Path>) -> HttpResponse {
    let raw_path = req.path().trim_start_matches('/');
    let lookup_path = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

    if let Some(clean_rel) = sanitize_relative_path(lookup_path) {
        if let Some(dir) = dist_dir {
            let candidate = dir.join(&clean_rel);
            if let Ok(bytes) = tokio::fs::read(&candidate).await {
                let mime = mime_type_for_path(&clean_rel.to_string_lossy());
                return HttpResponse::Ok().content_type(mime).body(bytes);
            }
        }

        if let Some(file) = EMBEDDED_DIST.get_file(&clean_rel) {
            let mime = mime_type_for_path(&clean_rel.to_string_lossy());
            return HttpResponse::Ok()
                .content_type(mime)
                .body(file.contents().to_vec());
        }
    }

    // SPA fallback to index.html for navigation routes
    if let Some(dir) = dist_dir {
        let index_path = dir.join("index.html");
        if let Ok(bytes) = tokio::fs::read(&index_path).await {
            return HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .body(bytes);
        }
    }

    if let Some(index) = EMBEDDED_DIST.get_file("index.html") {
        return HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(index.contents().to_vec());
    }

    HttpResponse::NotFound().body("Frontend build (dist) not found.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};

    #[actix_web::test]
    async fn embedded_frontend_and_authenticated_local_media_work() {
        let temp_downloads = std::env::temp_dir().join(format!(
            "courseviewer-media-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let item_dir = temp_downloads.join("rust/item-1");
        tokio::fs::create_dir_all(&item_dir).await.unwrap();
        tokio::fs::write(
            item_dir.join("10.vtt"),
            b"WEBVTT\n\n00:00.000 --> 00:01.000\nHi\n",
        )
        .await
        .unwrap();

        let frontend_cfg = FrontendConfig {
            mode: FrontendMode::Embedded { dist_dir: None },
            downloads_dir: temp_downloads.clone(),
            proxy_client: reqwest::Client::new(),
        };

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(crate::auth::ApiKeyConfig::required(
                    "my-key",
                )))
                .app_data(web::Data::new(frontend_cfg))
                .configure(configure),
        )
        .await;

        // Public SPA routes serve embedded index.html
        for route in ["/", "/admin", "/courses/rust"] {
            let resp =
                test::call_service(&app, test::TestRequest::get().uri(route).to_request()).await;
            assert!(resp.status().is_success(), "route {route} failed");
            let body = test::read_body(resp).await;
            assert!(
                String::from_utf8_lossy(&body)
                    .to_ascii_lowercase()
                    .contains("<!doctype html>")
            );
        }

        // Unauthenticated media request is rejected
        let unauth = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/local-media/rust/item-1/10.vtt")
                .to_request(),
        )
        .await;
        assert_eq!(unauth.status(), 401);

        // Authenticated media request via query parameter or cookie succeeds and supports Range
        let authed = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/local-media/rust/item-1/10.vtt?api_key=my-key")
                .insert_header(("Range", "bytes=0-5"))
                .to_request(),
        )
        .await;
        assert_eq!(authed.status(), 206);
        let partial_body = test::read_body(authed).await;
        assert_eq!(&partial_body[..], b"WEBVTT");

        tokio::fs::remove_dir_all(temp_downloads).await.unwrap();
    }
}
