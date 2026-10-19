//! Typed ConnectRPC admin service backed by the existing SQLite catalog.
use std::sync::Arc;

use actix_web::{HttpRequest, HttpResponse, web};
use connectrpc::{
    ConnectError, ConnectRpcService, RequestContext, Response, Router, ServiceRequest,
    ServiceResult,
};
use http_body_util::{BodyExt, Full};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, params};
use tower::ServiceExt;

use crate::proto::courseviewer::admin::v1::*;
use crate::services::courseviewer::admin::v1::AdminService;

type DbPool = Pool<SqliteConnectionManager>;

pub struct Admin {
    pub pool: DbPool,
    pub downloads: Arc<crate::downloads::Downloads>,
}

fn sql_error(error: rusqlite::Error) -> ConnectError {
    if let rusqlite::Error::SqliteFailure(ref failure, _) = error {
        return match failure.extended_code {
            2067 | 1555 => ConnectError::already_exists("The slug or remote ID is already in use."),
            787 => ConnectError::failed_precondition(
                "This record is still referenced by another record, or a selected course no longer exists.",
            ),
            275 | 1811 => ConnectError::invalid_argument(
                "One or more values do not satisfy the application constraints.",
            ),
            _ => {
                eprintln!("Admin database error: {error}");
                ConnectError::internal("Could not save or load application data.")
            }
        };
    }
    eprintln!("Admin database error: {error}");
    ConnectError::internal("Could not save or load application data.")
}

impl Admin {
    async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, ConnectError> + Send + 'static,
    ) -> Result<T, ConnectError> {
        let pool = self.pool.clone();
        web::block(move || {
            let mut conn = pool.get().map_err(|_| {
                ConnectError::unavailable("The database is busy. Please try again.")
            })?;
            f(&mut conn)
        })
        .await
        .map_err(|_| ConnectError::internal("The database operation could not finish."))?
    }
}

fn found(changed: usize) -> Result<(), ConnectError> {
    if changed == 0 {
        return Err(ConnectError::not_found(
            "This record no longer exists. Refresh and try again.",
        ));
    }
    Ok(())
}

fn ids(conn: &Connection, query: &str, id: i64) -> Result<Vec<i64>, ConnectError> {
    conn.prepare(query)
        .map_err(sql_error)?
        .query_map([id], |r| r.get(0))
        .map_err(sql_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(sql_error)
}

fn build_course_downloads(
    tx: &Connection,
    courses: &[Course],
    downloads: &[DownloadJob],
    download_supplements: bool,
) -> Result<Vec<CourseDownloadStatus>, ConnectError> {
    use std::collections::{HashMap, HashSet};

    let active_courses: HashSet<i64> = downloads
        .iter()
        .filter(|j| ["planning", "queued", "running", "paused"].contains(&j.state.as_str()))
        .flat_map(|j| j.course_ids.iter().copied())
        .collect();

    let mut assets_by_item: HashMap<i64, Vec<AssetDownloadStatus>> = HashMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT
                    a.id,
                    a.item_id,
                    a.kind,
                    a.variant_key,
                    a.format,
                    coalesce(a.height, 0),
                    coalesce(a.language_code, ''),
                    coalesce(f.state, ''),
                    coalesce(f.bytes_present, 0),
                    coalesce(f.size_bytes, 0),
                    coalesce(f.relative_path, ''),
                    coalesce(t.state, ''),
                    coalesce(t.downloaded_bytes, 0),
                    coalesce(t.total_bytes, 0),
                    coalesce(t.last_error_message, '')
                 FROM assets a
                 LEFT JOIN local_files f ON f.asset_id = a.id
                 LEFT JOIN download_tasks t ON t.id = (
                    SELECT t2.id FROM download_tasks t2 WHERE t2.asset_id = a.id ORDER BY t2.id DESC LIMIT 1
                 )
                 WHERE a.removed_at IS NULL
                 ORDER BY a.item_id, CASE a.kind WHEN 'video' THEN 0 WHEN 'subtitle' THEN 1 ELSE 2 END, a.id",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map([], |r| {
                let id: i64 = r.get(0)?;
                let item_id: i64 = r.get(1)?;
                let kind: String = r.get(2)?;
                let variant_key: String = r.get(3)?;
                let format: String = r.get(4)?;
                let height: i32 = r.get(5)?;
                let language_code: String = r.get(6)?;
                let file_state: String = r.get(7)?;
                let bytes_present: i64 = r.get(8)?;
                let size_bytes: i64 = r.get(9)?;
                let relative_path: String = r.get(10)?;
                let task_state: String = r.get(11)?;
                let task_downloaded: i64 = r.get(12)?;
                let task_total: i64 = r.get(13)?;
                let last_error: String = r.get(14)?;

                let (state, downloaded_bytes, total_bytes) = if file_state == "ready" {
                    ("downloaded".to_string(), bytes_present, size_bytes)
                } else if task_state == "running" {
                    (
                        "downloading".to_string(),
                        task_downloaded.max(bytes_present),
                        task_total.max(size_bytes),
                    )
                } else if task_state == "queued" || task_state == "paused" {
                    (
                        task_state,
                        task_downloaded.max(bytes_present),
                        task_total.max(size_bytes),
                    )
                } else if task_state == "failed" {
                    (
                        "failed".to_string(),
                        task_downloaded.max(bytes_present),
                        task_total.max(size_bytes),
                    )
                } else if task_state == "cancelled" {
                    (
                        "cancelled".to_string(),
                        task_downloaded.max(bytes_present),
                        task_total.max(size_bytes),
                    )
                } else if file_state == "partial" {
                    ("partial".to_string(), bytes_present, size_bytes)
                } else {
                    ("remaining".to_string(), 0, size_bytes)
                };

                Ok((
                    item_id,
                    AssetDownloadStatus {
                        id,
                        kind,
                        variant_key,
                        format,
                        height,
                        language_code,
                        state,
                        downloaded_bytes,
                        total_bytes,
                        relative_path,
                        last_error,
                        ..Default::default()
                    },
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (item_id, asset) = row.map_err(sql_error)?;
            assets_by_item.entry(item_id).or_default().push(asset);
        }
    }

    let mut items_by_lesson: HashMap<i64, Vec<ItemDownloadStatus>> = HashMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT i.id, i.course_id, i.lesson_id, i.remote_id, coalesce(i.slug,''), i.name, i.content_type, i.position, i.is_locked != 0, coalesce(p.is_completed, 0) != 0
                 FROM items i
                 LEFT JOIN playback_progress p ON p.item_id = i.id
                 WHERE i.removed_at IS NULL
                 ORDER BY i.course_id, i.lesson_id, i.position, i.id",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, i32>(7)?,
                    r.get::<_, bool>(8)?,
                    r.get::<_, bool>(9)?,
                ))
            })
            .map_err(sql_error)?;
        let raw_rows: Vec<_> = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        let mut unlocked_courses = std::collections::HashSet::new();
        for (_, course_id, _, _, _, _, content_type, _, is_locked, _) in &raw_rows {
            let base_downloadable = content_type == "lecture"
                || (download_supplements
                    && (content_type == "supplement" || content_type == "reading"));
            if base_downloadable && !*is_locked {
                unlocked_courses.insert(*course_id);
            }
        }
        for (
            item_id,
            course_id,
            lesson_id,
            remote_id,
            slug,
            name,
            content_type,
            position,
            is_locked,
            completed,
        ) in raw_rows
        {
            let assets = assets_by_item.remove(&item_id).unwrap_or_default();
            let base_downloadable = content_type == "lecture"
                || (download_supplements
                    && (content_type == "supplement" || content_type == "reading"));
            let downloadable = (base_downloadable
                && (!is_locked || !unlocked_courses.contains(&course_id)))
                || !assets.is_empty();
            let downloaded_bytes: i64 = assets.iter().map(|a| a.downloaded_bytes).sum();
            let total_bytes: i64 = assets.iter().map(|a| a.total_bytes).sum();
            let last_error = assets
                .iter()
                .find(|a| !a.last_error.is_empty())
                .map(|a| a.last_error.clone())
                .unwrap_or_default();
            let state = if !downloadable {
                "skipped".to_string()
            } else if assets.is_empty() {
                if active_courses.contains(&course_id) {
                    "queued".to_string()
                } else {
                    "remaining".to_string()
                }
            } else if assets.iter().all(|a| a.state == "downloaded") {
                "downloaded".to_string()
            } else if assets.iter().any(|a| a.state == "downloading") {
                "downloading".to_string()
            } else if assets
                .iter()
                .any(|a| a.state == "queued" || a.state == "paused")
            {
                "queued".to_string()
            } else if assets.iter().any(|a| a.state == "failed") {
                if assets.iter().any(|a| a.state == "downloaded") {
                    "partial".to_string()
                } else {
                    "failed".to_string()
                }
            } else if assets.iter().any(|a| a.state == "downloaded") || downloaded_bytes > 0 {
                "partial".to_string()
            } else {
                "remaining".to_string()
            };
            items_by_lesson
                .entry(lesson_id)
                .or_default()
                .push(ItemDownloadStatus {
                    id: item_id,
                    remote_id,
                    slug,
                    name,
                    content_type,
                    position,
                    is_locked,
                    downloadable,
                    state,
                    downloaded_bytes,
                    total_bytes,
                    last_error,
                    assets,
                    completed,
                    ..Default::default()
                });
        }
    }

    let mut lessons_by_module: HashMap<i64, Vec<LessonDownloadStatus>> = HashMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT id, module_id, remote_id, coalesce(slug,''), name, position
                 FROM lessons
                 WHERE removed_at IS NULL
                 ORDER BY course_id, module_id, position, id",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i32>(5)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (lesson_id, module_id, remote_id, slug, name, position) = row.map_err(sql_error)?;
            let items = items_by_lesson.remove(&lesson_id).unwrap_or_default();
            let total_items = items.iter().filter(|i| i.downloadable).count() as i32;
            let downloaded_items = items
                .iter()
                .filter(|i| i.downloadable && i.state == "downloaded")
                .count() as i32;
            let remaining_items = (total_items - downloaded_items).max(0);
            let downloaded_bytes: i64 = items.iter().map(|i| i.downloaded_bytes).sum();
            let total_bytes: i64 = items.iter().map(|i| i.total_bytes).sum();
            let state = if total_items > 0 && downloaded_items == total_items {
                "downloaded".to_string()
            } else if items
                .iter()
                .any(|i| i.downloadable && (i.state == "downloading" || i.state == "queued"))
            {
                "downloading".to_string()
            } else if items.iter().any(|i| i.downloadable && i.state == "failed") {
                if downloaded_items > 0 {
                    "partial".to_string()
                } else {
                    "failed".to_string()
                }
            } else if downloaded_items > 0
                || items.iter().any(|i| i.downloadable && i.state == "partial")
            {
                "partial".to_string()
            } else {
                "remaining".to_string()
            };
            lessons_by_module
                .entry(module_id)
                .or_default()
                .push(LessonDownloadStatus {
                    id: lesson_id,
                    remote_id,
                    slug,
                    name,
                    position,
                    total_items,
                    downloaded_items,
                    remaining_items,
                    downloaded_bytes,
                    total_bytes,
                    state,
                    items,
                    ..Default::default()
                });
        }
    }

    let mut modules_by_course: HashMap<i64, Vec<ModuleDownloadStatus>> = HashMap::new();
    {
        let mut stmt = tx
            .prepare(
                "SELECT id, course_id, remote_id, coalesce(slug,''), name, position
                 FROM modules
                 WHERE removed_at IS NULL
                 ORDER BY course_id, position, id",
            )
            .map_err(sql_error)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i32>(5)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (module_id, course_id, remote_id, slug, name, position) = row.map_err(sql_error)?;
            let lessons = lessons_by_module.remove(&module_id).unwrap_or_default();
            let total_items: i32 = lessons.iter().map(|l| l.total_items).sum();
            let downloaded_items: i32 = lessons.iter().map(|l| l.downloaded_items).sum();
            let remaining_items = (total_items - downloaded_items).max(0);
            let downloaded_bytes: i64 = lessons.iter().map(|l| l.downloaded_bytes).sum();
            let total_bytes: i64 = lessons.iter().map(|l| l.total_bytes).sum();
            let state = if total_items > 0 && downloaded_items == total_items {
                "downloaded".to_string()
            } else if lessons.iter().any(|l| l.state == "downloading") {
                "downloading".to_string()
            } else if lessons.iter().any(|l| l.state == "failed") {
                if downloaded_items > 0 {
                    "partial".to_string()
                } else {
                    "failed".to_string()
                }
            } else if downloaded_items > 0 || lessons.iter().any(|l| l.state == "partial") {
                "partial".to_string()
            } else {
                "remaining".to_string()
            };
            modules_by_course
                .entry(course_id)
                .or_default()
                .push(ModuleDownloadStatus {
                    id: module_id,
                    remote_id,
                    slug,
                    name,
                    position,
                    total_items,
                    downloaded_items,
                    remaining_items,
                    downloaded_bytes,
                    total_bytes,
                    state,
                    lessons,
                    ..Default::default()
                });
        }
    }

    let mut course_errors: HashMap<i64, String> = HashMap::new();
    {
        let mut stmt = tx
            .prepare("SELECT id, coalesce(last_sync_error, '') FROM courses")
            .map_err(sql_error)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql_error)?;
        for row in rows {
            let (id, err) = row.map_err(sql_error)?;
            if !err.is_empty() {
                course_errors.insert(id, err);
            }
        }
    }

    let mut result = Vec::with_capacity(courses.len());
    for c in courses {
        let modules = modules_by_course.remove(&c.id).unwrap_or_default();
        let module_count = modules.len() as i32;
        let lesson_count: i32 = modules.iter().map(|m| m.lessons.len() as i32).sum();
        let total_items: i32 = modules.iter().map(|m| m.total_items).sum();
        let downloaded_items: i32 = modules.iter().map(|m| m.downloaded_items).sum();
        let remaining_items = (total_items - downloaded_items).max(0);
        let downloaded_bytes: i64 = modules.iter().map(|m| m.downloaded_bytes).sum();
        let total_bytes: i64 = modules.iter().map(|m| m.total_bytes).sum();
        let state = if modules.is_empty() && c.outline_state != "ready" {
            if active_courses.contains(&c.id) {
                "downloading".to_string()
            } else {
                c.outline_state.clone()
            }
        } else if total_items > 0 && downloaded_items == total_items {
            "downloaded".to_string()
        } else if active_courses.contains(&c.id) || modules.iter().any(|m| m.state == "downloading")
        {
            "downloading".to_string()
        } else if modules.iter().any(|m| m.state == "failed") {
            if downloaded_items > 0 {
                "partial".to_string()
            } else {
                "failed".to_string()
            }
        } else if downloaded_items > 0 || modules.iter().any(|m| m.state == "partial") {
            "partial".to_string()
        } else {
            "remaining".to_string()
        };
        let last_error = course_errors.remove(&c.id).unwrap_or_default();
        result.push(CourseDownloadStatus {
            course_id: c.id,
            slug: c.slug.clone(),
            name: c.name.clone(),
            outline_state: c.outline_state.clone(),
            specialization_ids: c.specialization_ids.clone(),
            module_count,
            lesson_count,
            total_items,
            downloaded_items,
            remaining_items,
            downloaded_bytes,
            total_bytes,
            state,
            last_error,
            modules,
            ..Default::default()
        });
    }
    Ok(result)
}

fn dashboard(conn: &mut Connection) -> Result<GetDashboardResponse, ConnectError> {
    // A read transaction keeps catalog relationships and totals consistent.
    let tx = conn.transaction().map_err(sql_error)?;
    let mut courses = tx.prepare("SELECT c.id,c.remote_id,c.slug,c.name,coalesce(c.description,''),coalesce(c.image_url,''),coalesce(c.estimated_workload,''),c.metadata_state,c.outline_state,c.updated_at,c.archived_at IS NOT NULL,(SELECT count(*) FROM modules m WHERE m.course_id=c.id AND m.removed_at IS NULL) FROM courses c ORDER BY c.updated_at DESC,c.id DESC").map_err(sql_error)?
        .query_map([], |r| Ok(Course { id:r.get(0)?,remote_id:r.get(1)?,slug:r.get(2)?,name:r.get(3)?,description:r.get(4)?,image_url:r.get(5)?,estimated_workload:r.get(6)?,metadata_state:r.get(7)?,outline_state:r.get(8)?,updated_at:r.get(9)?,archived:r.get(10)?,module_count:r.get(11)?,..Default::default() })).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    for c in &mut courses {
        c.specialization_ids = ids(
            &tx,
            "SELECT specialization_id FROM specialization_courses WHERE course_id=? ORDER BY specialization_id",
            c.id,
        )?;
    }
    let mut specializations = tx.prepare("SELECT id,remote_id,slug,name,coalesce(description,''),coalesce(tagline,''),coalesce(image_url,''),updated_at FROM specializations ORDER BY updated_at DESC,id DESC").map_err(sql_error)?
        .query_map([], |r| Ok(Specialization { id:r.get(0)?,remote_id:r.get(1)?,slug:r.get(2)?,name:r.get(3)?,description:r.get(4)?,tagline:r.get(5)?,image_url:r.get(6)?,updated_at:r.get(7)?,..Default::default() })).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    for s in &mut specializations {
        s.course_ids = ids(
            &tx,
            "SELECT course_id FROM specialization_courses WHERE specialization_id=? ORDER BY position,course_id",
            s.id,
        )?;
    }
    let downloads = tx.prepare("SELECT j.id,coalesce(c.name,s.name,'Unknown'),CASE WHEN j.course_id IS NULL THEN 'specialization' ELSE 'course' END,j.state,count(t.id),coalesce(sum(t.state='completed'),0),coalesce(sum(t.downloaded_bytes),0),coalesce(sum(t.total_bytes),0),coalesce(j.last_error,''),j.created_at,coalesce(j.course_id,0),coalesce(j.specialization_id,0),j.selection_json FROM download_jobs j LEFT JOIN courses c ON c.id=j.course_id LEFT JOIN specializations s ON s.id=j.specialization_id LEFT JOIN download_tasks t ON t.job_id=j.id GROUP BY j.id ORDER BY j.created_at DESC,j.id DESC").map_err(sql_error)?
        .query_map([], |r| {
            let course_id: i64 = r.get(10)?;
            let specialization_id: i64 = r.get(11)?;
            let selection_json: String = r.get(12)?;
            let mut course_ids = serde_json::from_str::<serde_json::Value>(&selection_json)
                .ok()
                .and_then(|v| v.get("course_ids").cloned())
                .and_then(|v| serde_json::from_value::<Vec<i64>>(v).ok())
                .unwrap_or_default();
            if course_ids.is_empty() && course_id > 0 {
                course_ids.push(course_id);
            }
            Ok(DownloadJob {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: r.get(2)?,
                state: r.get(3)?,
                task_count: r.get(4)?,
                completed_tasks: r.get(5)?,
                downloaded_bytes: r.get(6)?,
                total_bytes: r.get(7)?,
                last_error: r.get(8)?,
                created_at: r.get(9)?,
                course_ids,
                course_id,
                specialization_id,
                ..Default::default()
            })
        }).map_err(sql_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(sql_error)?;
    let settings = tx.query_row("SELECT preferred_video_height,preferred_video_format,quality_fallback_policy,subtitle_languages_json,download_subtitles,download_supplements,max_concurrent_downloads FROM application_settings WHERE id=1",[],|r| {
        let langs: String = r.get(3)?;
        Ok(ApplicationSettings { preferred_video_height:r.get(0)?,preferred_video_format:r.get(1)?,quality_fallback_policy:r.get(2)?,subtitle_languages:serde_json::from_str(&langs).map_err(|e| rusqlite::Error::FromSqlConversionFailure(3,rusqlite::types::Type::Text,Box::new(e)))?,download_subtitles:r.get(4)?,download_supplements:r.get(5)?,max_concurrent_downloads:r.get(6)?,..Default::default() })
    }).map_err(sql_error)?;
    let course_downloads =
        build_course_downloads(&tx, &courses, &downloads, settings.download_supplements)?;
    let storage_bytes = tx
        .query_row(
            "SELECT coalesce(sum(bytes_present),0) FROM local_files",
            [],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    tx.commit().map_err(sql_error)?;
    Ok(GetDashboardResponse {
        courses,
        specializations,
        downloads,
        settings: buffa::MessageField::some(settings),
        storage_bytes,
        course_downloads,
        ..Default::default()
    })
}

fn delete_course(conn: &mut Connection, id: i64) -> Result<(), ConnectError> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    let retained: i64 = tx.query_row("SELECT (SELECT count(*) FROM download_jobs WHERE course_id=?1)+(SELECT count(*) FROM local_files f JOIN assets a ON a.id=f.asset_id JOIN items i ON i.id=a.item_id WHERE i.course_id=?1)+(SELECT count(*) FROM download_tasks t JOIN assets a ON a.id=t.asset_id JOIN items i ON i.id=a.item_id WHERE i.course_id=?1)",[id],|r|r.get(0)).map_err(sql_error)?;
    if retained > 0 {
        return Err(ConnectError::failed_precondition(
            "This course has download history or local files. Archive it to keep those records intact.",
        ));
    }
    tx.execute("DELETE FROM specialization_courses WHERE course_id=?", [id])
        .map_err(sql_error)?;
    found(
        tx.execute("DELETE FROM courses WHERE id=?", [id])
            .map_err(sql_error)?,
    )?;
    tx.commit().map_err(sql_error)?;
    Ok(())
}

fn save_settings(conn: &Connection, s: ApplicationSettings) -> Result<(), ConnectError> {
    if s.subtitle_languages.iter().any(|l| l.trim().is_empty()) {
        return Err(ConnectError::invalid_argument(
            "Subtitle language codes cannot be blank.",
        ));
    }
    let languages = serde_json::to_string(&s.subtitle_languages)
        .map_err(|_| ConnectError::invalid_argument("Invalid subtitle languages."))?;
    conn.execute("UPDATE application_settings SET preferred_video_height=?1,preferred_video_format=?2,quality_fallback_policy=?3,subtitle_languages_json=?4,download_subtitles=?5,download_supplements=?6,max_concurrent_downloads=?7,updated_at=unixepoch()*1000 WHERE id=1",params![s.preferred_video_height,s.preferred_video_format,s.quality_fallback_policy,languages,s.download_subtitles,s.download_supplements,s.max_concurrent_downloads]).map_err(sql_error)?;
    Ok(())
}

#[allow(refining_impl_trait)]
impl AdminService for Admin {
    async fn import_course(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, ImportCourseRequest>,
    ) -> ServiceResult<ImportCourseResponse> {
        let slug = crate::coursera::slug(request.slug).map_err(ConnectError::invalid_argument)?;
        let (course, outline) = crate::coursera::fetch_course_bundle(&self.downloads.client, &slug)
            .await
            .map_err(ConnectError::invalid_argument)?;
        let (id, spec_remote_ids) = self
            .db(move |conn| {
                let id = crate::coursera::upsert_course(conn, &course, true)
                    .map_err(ConnectError::failed_precondition)?;
                match outline {
                    Ok(outline) => {
                        crate::coursera::store_outline(conn, id, &outline)
                            .map_err(ConnectError::failed_precondition)?;
                    }
                    Err(err) => {
                        conn.execute(
                            "UPDATE courses SET outline_state='failed',last_sync_error=?1,updated_at=unixepoch()*1000 WHERE id=?2",
                            params![err, id],
                        )
                        .map_err(sql_error)?;
                    }
                }
                let mut stmt = conn
                    .prepare(
                        "SELECT s.remote_id FROM specializations s
                         JOIN specialization_courses sc ON sc.specialization_id = s.id
                         WHERE sc.course_id = ?",
                    )
                    .map_err(sql_error)?;
                let specs = stmt
                    .query_map([id], |r| r.get::<_, String>(0))
                    .map_err(sql_error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(sql_error)?;
                Ok((id, specs))
            })
            .await?;
        for spec_remote_id in spec_remote_ids {
            self.downloads
                .client
                .ensure_specialization_enrolled(&spec_remote_id)
                .await;
        }
        Response::ok(ImportCourseResponse {
            id,
            ..Default::default()
        })
    }

    async fn import_specialization(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, ImportSpecializationRequest>,
    ) -> ServiceResult<ImportSpecializationResponse> {
        let slug = crate::coursera::slug(request.slug).map_err(ConnectError::invalid_argument)?;
        let (spec, courses, outlines) =
            crate::coursera::fetch_specialization_bundle(&self.downloads.client, &slug)
                .await
                .map_err(ConnectError::invalid_argument)?;
        let id = self
            .db(move |conn| {
                crate::coursera::import_specialization_with_outlines(
                    conn, &spec, &courses, &outlines,
                )
                .map_err(ConnectError::failed_precondition)
            })
            .await?;
        Response::ok(ImportSpecializationResponse {
            id,
            ..Default::default()
        })
    }

    async fn start_download(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, StartDownloadRequest>,
    ) -> ServiceResult<StartDownloadResponse> {
        let (course, specialization) = match request.to_owned_message().target {
            Some(start_download_request::Target::CourseId(id)) if id > 0 => (Some(id), None),
            Some(start_download_request::Target::SpecializationId(id)) if id > 0 => {
                (None, Some(id))
            }
            _ => {
                return Err(ConnectError::invalid_argument(
                    "Choose a course or specialisation.",
                ));
            }
        };
        let id = self
            .downloads
            .enqueue(course, specialization)
            .await
            .map_err(ConnectError::failed_precondition)?;
        Response::ok(StartDownloadResponse {
            id,
            ..Default::default()
        })
    }
    async fn get_dashboard(
        &self,
        _: RequestContext,
        _: ServiceRequest<'_, GetDashboardRequest>,
    ) -> ServiceResult<GetDashboardResponse> {
        let cookie = self.downloads.client.get_cookie();
        let mut response = self.db(dashboard).await?;
        if let Some(mut settings) = std::mem::take(&mut response.settings).into_option() {
            settings.coursera_cookie = cookie;
            response.settings = buffa::MessageField::some(settings);
        }
        Response::ok(response)
    }
    async fn set_course_archived(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, SetCourseArchivedRequest>,
    ) -> ServiceResult<SetCourseArchivedResponse> {
        let id = request.id;
        let archived = request.archived;
        self.db(move |c| found(c.execute("UPDATE courses SET archived_at=CASE WHEN ?1 THEN coalesce(archived_at,unixepoch()*1000) END,updated_at=unixepoch()*1000 WHERE id=?2",params![archived,id]).map_err(sql_error)?)).await?;
        Response::ok(SetCourseArchivedResponse::default())
    }
    async fn delete_course(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, DeleteCourseRequest>,
    ) -> ServiceResult<DeleteCourseResponse> {
        let id = request.id;
        self.db(move |c| delete_course(c, id)).await?;
        Response::ok(DeleteCourseResponse::default())
    }
    async fn delete_specialization(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, DeleteSpecializationRequest>,
    ) -> ServiceResult<DeleteSpecializationResponse> {
        let id = request.id;
        self.db(move |c| {
            let retained: i64 = c
                .query_row(
                    "SELECT count(*) FROM download_jobs WHERE specialization_id=?",
                    [id],
                    |r| r.get(0),
                )
                .map_err(sql_error)?;
            if retained > 0 {
                return Err(ConnectError::failed_precondition(
                    "This specialization has download history and cannot be deleted.",
                ));
            }
            found(
                c.execute("DELETE FROM specializations WHERE id=?", [id])
                    .map_err(sql_error)?,
            )
        })
        .await?;
        Response::ok(DeleteSpecializationResponse::default())
    }
    async fn save_settings(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, SaveSettingsRequest>,
    ) -> ServiceResult<SaveSettingsResponse> {
        let settings = request
            .to_owned_message()
            .settings
            .into_option()
            .ok_or_else(|| ConnectError::invalid_argument("Settings are required."))?;
        let cookie = settings.coursera_cookie.clone();
        self.db(move |c| save_settings(c, settings)).await?;
        self.downloads
            .client
            .update_cookie(&cookie)
            .map_err(ConnectError::internal)?;
        Response::ok(SaveSettingsResponse::default())
    }
    async fn cancel_download(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, CancelDownloadRequest>,
    ) -> ServiceResult<CancelDownloadResponse> {
        let id = request.id;
        self.downloads
            .cancel(id)
            .await
            .map_err(ConnectError::failed_precondition)?;
        Response::ok(CancelDownloadResponse::default())
    }
    async fn set_item_completed(
        &self,
        _: RequestContext,
        request: ServiceRequest<'_, SetItemCompletedRequest>,
    ) -> ServiceResult<SetItemCompletedResponse> {
        let item_id = request.item_id;
        let completed = request.completed;
        self.db(move |c| {
            let exists: i64 = c
                .query_row("SELECT count(*) FROM items WHERE id = ?1", [item_id], |r| {
                    r.get(0)
                })
                .map_err(sql_error)?;
            found(exists as usize)?;
            c.execute(
                "INSERT INTO playback_progress (item_id, is_completed, completed_at, updated_at)
                 VALUES (?1, ?2, CASE WHEN ?2 THEN unixepoch() * 1000 ELSE NULL END, unixepoch() * 1000)
                 ON CONFLICT(item_id) DO UPDATE SET
                     is_completed = excluded.is_completed,
                     completed_at = CASE WHEN excluded.is_completed THEN coalesce(playback_progress.completed_at, excluded.completed_at) ELSE NULL END,
                     updated_at = excluded.updated_at",
                params![item_id, if completed { 1 } else { 0 }],
            )
            .map_err(sql_error)?;
            Ok(())
        })
        .await?;
        Response::ok(SetItemCompletedResponse::default())
    }
}

/// Adapt the ConnectRPC tower service to Actix for the unary admin RPCs.
/// Protocol negotiation, Protobuf/JSON codecs, and errors belong to connectrpc.
pub fn routes(pool: DbPool) -> actix_web::Scope {
    let downloads = crate::downloads::Downloads::new(
        pool.clone(),
        Arc::new(crate::courseeraclient::CourseEraClient::new()),
    );
    routes_with_downloads(pool, downloads)
}

pub fn routes_with_downloads(
    pool: DbPool,
    downloads: Arc<crate::downloads::Downloads>,
) -> actix_web::Scope {
    let router = Router::new().add_service(Arc::new(Admin { pool, downloads }));
    web::scope("/courseviewer.admin.v1.AdminService")
        .app_data(web::Data::new(ConnectRpcService::new(router)))
        .app_data(web::PayloadConfig::new(4 * 1024 * 1024))
        .route("/{method}", web::post().to(rpc))
}

async fn rpc(
    req: HttpRequest,
    body: web::Bytes,
    service: web::Data<ConnectRpcService>,
) -> Result<HttpResponse, actix_web::Error> {
    if !crate::auth::is_request_authorized(&req) {
        return Ok(HttpResponse::Unauthorized()
            .content_type("application/json")
            .body(r#"{"code":"unauthenticated","message":"Invalid or missing API key."}"#));
    }
    let mut request = http::Request::builder()
        .method(req.method().as_str())
        .uri(req.uri().to_string());
    for (key, value) in req.headers() {
        request = request.header(key.as_str(), value.as_bytes());
    }
    let request = request
        .body(Full::new(buffa::bytes::Bytes::copy_from_slice(&body)))
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let response = service
        .get_ref()
        .clone()
        .oneshot(request)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let (parts, body) = response.into_parts();
    let mut response = HttpResponse::build(
        actix_web::http::StatusCode::from_u16(parts.status.as_u16())
            .map_err(actix_web::error::ErrorInternalServerError)?,
    );
    for (key, value) in &parts.headers {
        response.append_header((key.as_str(), value.as_bytes()));
    }
    Ok(response.body(
        body.collect()
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?
            .to_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, test};
    use buffa::Message;
    use serde_json::{Value, json};

    fn request(method: &str, body: Value) -> test::TestRequest {
        test::TestRequest::post()
            .uri(&format!("/courseviewer.admin.v1.AdminService/{method}"))
            .insert_header(("connect-protocol-version", "1"))
            .set_json(body)
    }

    #[actix_web::test]
    async fn slug_imports_and_background_queue_use_connectrpc() {
        let pool = crate::test_support::pool();
        let (base, provider) = crate::test_support::provider().await;
        let downloads = crate::downloads::Downloads::new(
            pool.clone(),
            Arc::new(crate::courseeraclient::CourseEraClient::for_test(base)),
        );
        let app =
            test::init_service(App::new().service(routes_with_downloads(pool.clone(), downloads)))
                .await;
        let saved: Value = test::call_and_read_body_json(
            &app,
            request("ImportCourse", json!({"slug":"rust"})).to_request(),
        )
        .await;
        let id = saved["id"].as_str().unwrap();
        let repeated: Value = test::call_and_read_body_json(
            &app,
            request("ImportCourse", json!({"slug":"rust"})).to_request(),
        )
        .await;
        assert_eq!(saved, repeated);
        let spec: Value = test::call_and_read_body_json(
            &app,
            request("ImportSpecialization", json!({"slug":"systems"})).to_request(),
        )
        .await;
        let spec_id = spec["id"].as_str().unwrap();
        let spec2: Value = test::call_and_read_body_json(
            &app,
            request(
                "ImportSpecialization",
                json!({"slug":"distributed-systems"}),
            )
            .to_request(),
        )
        .await;
        let spec2_id = spec2["id"].as_str().unwrap();
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/courseviewer.admin.v1.AdminService/GetDashboard")
                .insert_header(("content-type", "application/proto"))
                .insert_header(("connect-protocol-version", "1"))
                .set_payload(GetDashboardRequest::default().encode_to_vec())
                .to_request(),
        )
        .await;
        assert!(response.status().is_success());
        let dashboard =
            GetDashboardResponse::decode_from_slice(&test::read_body(response).await).unwrap();
        assert_eq!(dashboard.courses.len(), 3);
        assert_eq!(dashboard.specializations.len(), 2);
        assert_eq!(dashboard.course_downloads.len(), 3);
        let rust = dashboard.courses.iter().find(|c| c.slug == "rust").unwrap();
        let async_rust = dashboard
            .courses
            .iter()
            .find(|c| c.slug == "async-rust")
            .unwrap();
        assert_eq!(rust.metadata_state, "ready");
        assert_eq!(rust.outline_state, "ready");
        assert_eq!(rust.module_count, 1);
        assert_eq!(rust.estimated_workload, "4 weeks");
        assert_eq!(rust.specialization_ids.len(), 2);
        assert_eq!(async_rust.metadata_state, "ready");
        assert_eq!(async_rust.outline_state, "ready");
        assert_eq!(async_rust.module_count, 1);
        for cd in &dashboard.course_downloads {
            assert_eq!(cd.module_count, 1);
            assert_eq!(cd.lesson_count, 1);
            assert_eq!(cd.total_items, 2);
            assert_eq!(cd.downloaded_items, 0);
            assert_eq!(cd.remaining_items, 2);
            assert_eq!(cd.modules[0].lessons[0].items.len(), 2);
        }
        for slug in ["missing", "denied", "Bad Slug"] {
            let response = test::call_service(
                &app,
                request("ImportCourse", json!({"slug":slug})).to_request(),
            )
            .await;
            assert_eq!(response.status(), 400);
        }
        let started: Value = test::call_and_read_body_json(
            &app,
            request("StartDownload", json!({"specializationId":spec_id})).to_request(),
        )
        .await;
        let duplicate_spec = test::call_service(
            &app,
            request("StartDownload", json!({"specializationId":spec_id})).to_request(),
        )
        .await;
        assert_eq!(
            duplicate_spec.status(),
            400,
            "duplicate specialization downloads must be rejected"
        );
        let started2: Value = test::call_and_read_body_json(
            &app,
            request("StartDownload", json!({"specializationId":spec2_id})).to_request(),
        )
        .await;
        assert!(started2["id"].as_str().is_some());
        for job_id in [&started["id"], &started2["id"]] {
            let response = test::call_service(
                &app,
                request("CancelDownload", json!({"id":job_id})).to_request(),
            )
            .await;
            assert!(response.status().is_success());
        }
        let response = test::call_service(
            &app,
            request("SetCourseArchived", json!({"id":id,"archived":true})).to_request(),
        )
        .await;
        assert!(response.status().is_success());
        let response = test::call_service(
            &app,
            request("StartDownload", json!({"courseId":id})).to_request(),
        )
        .await;
        assert_eq!(
            response.status(),
            400,
            "archived courses cannot be downloaded"
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row("SELECT count(*) FROM download_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        provider.stop(true).await;
    }

    #[actix_web::test]
    async fn settings_validate_and_persist() {
        let pool = crate::test_support::pool();
        let app = test::init_service(App::new().service(routes(pool.clone()))).await;
        let mut settings = json!({"preferredVideoHeight":1080,"preferredVideoFormat":"webm","qualityFallbackPolicy":"exact","subtitleLanguages":["en","fr"],"downloadSubtitles":true,"downloadSupplements":false,"maxConcurrentDownloads":5,"courseraCookie":"CAUTH=test-session-token; CSRF3-Token=csrf-val"});
        let response = test::call_service(
            &app,
            request("SaveSettings", json!({"settings":settings})).to_request(),
        )
        .await;
        assert!(response.status().is_success());
        settings["maxConcurrentDownloads"] = json!(0);
        settings["courseraCookie"] = json!("CAUTH=should-not-save");
        let response = test::call_service(
            &app,
            request("SaveSettings", json!({"settings":settings})).to_request(),
        )
        .await;
        assert_eq!(response.status(), 400);
        let dash: Value =
            test::call_and_read_body_json(&app, request("GetDashboard", json!({})).to_request())
                .await;
        assert_eq!(
            dash["settings"]["courseraCookie"],
            "CAUTH=test-session-token; CSRF3-Token=csrf-val"
        );
        let mut conn = pool.get().unwrap();
        let settings = dashboard(&mut conn).unwrap().settings;
        assert_eq!(settings.preferred_video_height, 1080);
        assert_eq!(settings.max_concurrent_downloads, 5);
        assert_eq!(settings.subtitle_languages, vec!["en", "fr"]);
    }

    #[actix_web::test]
    async fn api_key_authentication_protects_admin_service() {
        let pool = crate::test_support::pool();
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(crate::auth::ApiKeyConfig::required(
                    "super-secret-key",
                )))
                .service(routes(pool.clone())),
        )
        .await;

        let unauth =
            test::call_service(&app, request("GetDashboard", json!({})).to_request()).await;
        assert_eq!(unauth.status(), 401);

        let wrong_key = test::call_service(
            &app,
            request("GetDashboard", json!({}))
                .insert_header(("x-api-key", "wrong"))
                .to_request(),
        )
        .await;
        assert_eq!(wrong_key.status(), 401);

        let valid_header = test::call_service(
            &app,
            request("GetDashboard", json!({}))
                .insert_header(("x-api-key", "super-secret-key"))
                .to_request(),
        )
        .await;
        assert!(valid_header.status().is_success());

        let valid_cookie = test::call_service(
            &app,
            request("GetDashboard", json!({}))
                .insert_header(("cookie", "courseviewer_api_key=super-secret-key"))
                .to_request(),
        )
        .await;
        assert!(valid_cookie.status().is_success());
    }
}
