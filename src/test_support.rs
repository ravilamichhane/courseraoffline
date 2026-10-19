use actix_web::{App, HttpRequest, HttpResponse, HttpServer, dev::ServerHandle, web};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde_json::json;

pub fn pool() -> Pool<SqliteConnectionManager> {
    let manager =
        SqliteConnectionManager::memory().with_init(|c| crate::database::configure_connection(c));
    let pool = Pool::builder().max_size(1).build(manager).unwrap();
    crate::database::migrations()
        .unwrap()
        .to_latest(&mut pool.get().unwrap())
        .unwrap();
    pool
}

pub async fn provider() -> (String, ServerHandle) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test provider listener");
    let address = listener.local_addr().unwrap();
    let server = HttpServer::new(|| App::new().route("/{tail:.*}", web::route().to(response)))
        .workers(1)
        .listen(listener)
        .unwrap()
        .run();
    let handle = server.handle();
    actix_web::rt::spawn(server);
    (format!("http://{address}"), handle)
}

async fn response(request: HttpRequest) -> HttpResponse {
    let path = request.path();
    if request.query_string().contains("slug=denied") {
        return HttpResponse::Forbidden().finish();
    }
    if request.query_string().contains("slug=missing") {
        return HttpResponse::Ok().json(json!({"elements":[]}));
    }
    let a = json!({"id":"c1","slug":"rust","name":"Rust","description":"<p>Learn Rust.</p>","estimatedWorkload":"4 weeks"});
    let b = json!({"id":"c2","slug":"async-rust","name":"Async Rust","description":"Learn async Rust."});
    let c = json!({"id":"c3","slug":"distributed-rust","name":"Distributed Rust","description":"Learn distributed Rust."});
    let data = match path {
        "/api/adminUserPermissions.v1" => {
            json!({"elements":[{"id":"42","permissions":{}}],"paging":{},"linked":{}})
        }
        "/api/openCourseMemberships.v1" => {
            json!({"elements":[{"id":"42~c1","userId":42,"courseId":"c1","courseRole":"LEARNER","timestamp":1000}],"paging":{},"linked":{}})
        }
        "/api/onDemandSpecializationMemberships.v1" => {
            json!({"elements":[{"id":"42~s1","userId":42,"s12nId":"s1","role":"LEARNER"}]})
        }
        "/api/onDemandCourses.v1" => {
            let course = if request.query_string().contains("slug=async-rust") {
                b
            } else if request.query_string().contains("slug=distributed-rust") {
                c
            } else {
                a
            };
            json!({"elements":[course]})
        }
        "/api/onDemandSpecializations.v1" => {
            if request.query_string().contains("slug=distributed-systems") {
                json!({"elements":[{"id":"s2","slug":"distributed-systems","name":"Distributed Systems","description":"Distributed systems programming","courseIds":["c1","c3"]}],"linked":{"courses.v1":[a,c]}})
            } else {
                json!({"elements":[{"id":"s1","slug":"systems","name":"Systems","description":"Systems programming","courseIds":["c2","c1"]}],"linked":{"courses.v1":[a,b]}})
            }
        }
        "/api/onDemandCourseMaterials.v2/" => json!({"elements":[{"moduleIds":["m1"]}],"linked":{
            "onDemandCourseMaterialModules.v1":[{"id":"m1","name":"Module","lessonIds":["l1"]}],
            "onDemandCourseMaterialLessons.v1":[{"id":"l1","name":"Lesson","itemIds":["v1","r1"],"elementIds":["v1","r1"]}],
            "onDemandCourseMaterialItems.v2":[{"id":"v1","name":"Video","contentSummary":{"typeName":"lecture"}},{"id":"r1","name":"Reading","contentSummary":{"typeName":"supplement"}}]
        }}),
        "/video.mp4" => return HttpResponse::Ok().body(vec![7u8; 32768]),
        "/slow-video.mp4" => {
            let size = 256 * 1024usize;
            let offset = request
                .headers()
                .get("Range")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bytes="))
                .and_then(|v| v.strip_suffix('-'))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            if offset >= size {
                return HttpResponse::RangeNotSatisfiable().finish();
            }
            let mut response = if offset > 0 {
                HttpResponse::PartialContent()
            } else {
                HttpResponse::Ok()
            };
            if offset > 0 {
                response.insert_header((
                    "Content-Range",
                    format!("bytes {offset}-{}/{size}", size - 1),
                ));
            }
            return response.no_chunking((size - offset) as u64).streaming(
                futures_util::stream::unfold(offset, move |offset| async move {
                    if offset >= size {
                        return None;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(8)).await;
                    let end = (offset + 4096).min(size);
                    let bytes: Vec<u8> = (offset..end).map(|n| (n % 251) as u8).collect();
                    Some((Ok::<_, std::io::Error>(web::Bytes::from(bytes)), end))
                }),
            );
        }
        "/caption.vtt" => {
            return HttpResponse::Ok().body("WEBVTT\n\n00:00.000 --> 00:01.000\nRust\n");
        }
        _ if path.starts_with("/api/onDemandLectureVideos.v1/") => {
            let video = if path.contains("slow") {
                "/slow-video.mp4"
            } else {
                "/video.mp4"
            };
            json!({"linked":{"onDemandVideos.v1":[{"sources":{"byResolution":{"360p":{"mp4VideoUrl":video},"1080p":{"mp4VideoUrl":video}}},"subtitlesVtt":{"en":"/caption.vtt"}}]}})
        }
        _ if path.starts_with("/api/onDemandSupplements.v1/") => {
            json!({"linked":{"openCourseAssets.v1":[{"typeName":"cml","definition":{"renderableHtmlWithMetadata":{"renderableHtml":"<p>Reading content.</p>"}}}]}})
        }
        _ => return HttpResponse::NotFound().finish(),
    };
    HttpResponse::Ok().json(data)
}
