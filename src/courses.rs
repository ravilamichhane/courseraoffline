use actix_web::{HttpRequest, HttpResponse, get, web};
use serde::Deserialize;

use crate::appstate::AppState;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_course_info).service(get_lecture_video);
}

#[derive(Debug, Clone, Deserialize)]
struct LectureParams {
    course_id: String,
    item_id: String,
}

#[get("/courses/{course_id}/info")]
async fn get_course_info(
    req: HttpRequest,
    data: web::Data<AppState>,
    course_id: web::Path<String>,
) -> HttpResponse {
    if !courseviewer::auth::is_request_authorized(&req) {
        return HttpResponse::Unauthorized().body("Unauthorized");
    }
    let result = data.client.enroll_in_specialization(&course_id).await;
    match result {
        Ok(response) => HttpResponse::Ok().body(response.text().await.unwrap()),
        Err(err) => {
            println!("Error: {}", err);
            HttpResponse::InternalServerError().body(err.to_string())
        }
    }
}

#[get("/courses/lecture/{course_id}/{item_id}")]
async fn get_lecture_video(
    req: HttpRequest,
    data: web::Data<AppState>,
    params: web::Path<LectureParams>,
) -> HttpResponse {
    if !courseviewer::auth::is_request_authorized(&req) {
        return HttpResponse::Unauthorized().body("Unauthorized");
    }
    let result = data
        .client
        .get_supplement(&params.course_id, &params.item_id)
        .await;
    match result {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(err) => {
            println!("Error: {}", err);
            HttpResponse::InternalServerError().body(err.to_string())
        }
    }
}
