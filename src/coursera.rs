//! Coursera metadata normalization and transactional catalog imports.
use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::courseeraclient::CourseEraClient;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCourse {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub photo_url: String,
    #[serde(default)]
    pub promo_photo: String,
    #[serde(default)]
    pub estimated_workload: String,
    #[serde(default)]
    pub course_status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSpecialization {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub course_ids: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tagline: String,
    #[serde(default)]
    pub logo: String,
    #[serde(default)]
    pub photo_url: String,
}

pub fn slug(input: &str) -> Result<String, String> {
    let value = input.trim();
    if value.is_empty()
        || value.len() > 200
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("Enter a Coursera slug, for example machine-learning.".into());
    }
    Ok(value.to_owned())
}

fn first<T: serde::de::DeserializeOwned>(data: &Value) -> Result<T, String> {
    let element = data
        .get("elements")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
        .ok_or("No matching Coursera course or specialisation found.")?;
    serde_json::from_value(element.clone())
        .map_err(|_| "Coursera returned incomplete metadata.".into())
}

pub async fn fetch_course(client: &CourseEraClient, input: &str) -> Result<RemoteCourse, String> {
    let slug = slug(input)?;
    let data = client
        .api_json(
            "/api/onDemandCourses.v1",
            &[
                ("q", "slug"),
                ("slug", &slug),
                (
                    "fields",
                    "name,slug,description,photoUrl,promoPhoto,estimatedWorkload,courseStatus",
                ),
            ],
        )
        .await?;
    first(&data)
}

pub async fn fetch_specialization(
    client: &CourseEraClient,
    input: &str,
) -> Result<(RemoteSpecialization, Vec<RemoteCourse>), String> {
    let slug = slug(input)?;
    let data = client.api_json("/api/onDemandSpecializations.v1", &[
        ("q", "slug"), ("slug", &slug), ("includes", "courseIds"),
        ("fields", "courseIds,name,slug,description,tagline,logo,photoUrl,courses.v1(name,slug,description,photoUrl,promoPhoto,estimatedWorkload,courseStatus)"),
    ]).await?;
    let spec: RemoteSpecialization = first(&data)?;
    let linked = data
        .pointer("/linked/courses.v1")
        .cloned()
        .unwrap_or(json!([]));
    let courses: Vec<RemoteCourse> = serde_json::from_value(linked)
        .map_err(|_| "Coursera returned incomplete course details.")?;
    // Never replace a complete path with an incomplete linked response.
    if spec
        .course_ids
        .iter()
        .any(|id| !courses.iter().any(|c| &c.id == id))
    {
        return Err(
            "Coursera did not return all courses in this specialisation. Try again.".into(),
        );
    }
    Ok((spec, courses))
}

pub async fn fetch_outline_with_enrollment(
    client: &CourseEraClient,
    course_id: &str,
    course_slug: &str,
) -> Result<Outline, String> {
    client.ensure_course_enrolled(course_id, course_slug).await;
    let mut outline = fetch_outline(client, course_slug).await?;
    if outline.items.values().any(|i| i.is_locked) {
        client.force_enroll_course(course_id).await;
        if let Ok(refreshed) = fetch_outline(client, course_slug).await {
            outline = refreshed;
        }
    }
    Ok(outline)
}

pub async fn fetch_course_bundle(
    client: &CourseEraClient,
    input: &str,
) -> Result<(RemoteCourse, Result<Outline, String>), String> {
    let course = fetch_course(client, input).await?;
    client
        .ensure_course_enrolled(&course.id, &course.slug)
        .await;
    let outline = fetch_outline_with_enrollment(client, &course.id, &course.slug).await;
    Ok((course, outline))
}

pub async fn fetch_specialization_bundle(
    client: &CourseEraClient,
    input: &str,
) -> Result<
    (
        RemoteSpecialization,
        Vec<RemoteCourse>,
        HashMap<String, Result<Outline, String>>,
    ),
    String,
> {
    let (spec, mut courses) = fetch_specialization(client, input).await?;
    client.ensure_specialization_enrolled(&spec.id).await;
    for remote_id in &spec.course_ids {
        if let Some(course) = courses.iter().find(|c| &c.id == remote_id) {
            client
                .ensure_course_enrolled(&course.id, &course.slug)
                .await;
        } else {
            client.force_enroll_course(remote_id).await;
        }
    }
    let mut outlines = HashMap::new();
    for remote_id in &spec.course_ids {
        if let Some(course) = courses.iter_mut().find(|c| &c.id == remote_id) {
            if let Ok(mut full_course) = fetch_course(client, &course.slug).await {
                full_course.id = remote_id.clone();
                *course = full_course;
            }
            let outline = fetch_outline_with_enrollment(client, &course.id, &course.slug).await;
            outlines.insert(remote_id.clone(), outline);
        }
    }
    Ok((spec, courses, outlines))
}

fn validate_course(c: &RemoteCourse) -> Result<(), String> {
    slug(&c.slug)?;
    if c.id.trim().is_empty() || c.name.trim().is_empty() {
        return Err("Coursera returned an unnamed course or a missing ID.".into());
    }
    Ok(())
}

pub fn upsert_course(conn: &Connection, c: &RemoteCourse, full: bool) -> Result<i64, String> {
    validate_course(c)?;
    let image = if c.photo_url.is_empty() {
        &c.promo_photo
    } else {
        &c.photo_url
    };
    let state = if full { "ready" } else { "summary" };
    let existing_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM courses WHERE remote_id = ?1 OR slug = ?2 ORDER BY (remote_id = ?1) DESC LIMIT 1",
            params![c.id, c.slug],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing_id {
        conn.execute(
            "UPDATE courses SET
                remote_id = ?1,
                slug = ?2,
                name = ?3,
                description = ?4,
                image_url = ?5,
                estimated_workload = CASE WHEN ?9 OR ?6 != '' THEN ?6 ELSE courses.estimated_workload END,
                remote_status = CASE WHEN ?9 OR ?7 != '' THEN ?7 ELSE courses.remote_status END,
                metadata_state = CASE WHEN ?9 THEN 'ready' ELSE courses.metadata_state END,
                metadata_synced_at = unixepoch() * 1000,
                last_sync_error = NULL,
                updated_at = unixepoch() * 1000
             WHERE id = ?8",
            params![
                c.id,
                c.slug,
                c.name,
                c.description,
                image,
                c.estimated_workload,
                c.course_status,
                id,
                full
            ],
        )
        .map_err(|e| format!("Could not save Coursera course: {e}"))?;
        return Ok(id);
    }
    conn.query_row(
        "INSERT INTO courses(remote_id,slug,name,description,image_url,estimated_workload,remote_status,metadata_state,metadata_synced_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,unixepoch()*1000)
         RETURNING id",
        params![c.id, c.slug, c.name, c.description, image, c.estimated_workload, c.course_status, state],
        |r| r.get(0),
    )
    .map_err(|e| format!("Could not save Coursera course: {e}"))
}

pub fn import_specialization(
    conn: &mut Connection,
    spec: &RemoteSpecialization,
    courses: &[RemoteCourse],
) -> Result<i64, String> {
    import_specialization_with_outlines(conn, spec, courses, &HashMap::new())
}

pub fn import_specialization_with_outlines(
    conn: &mut Connection,
    spec: &RemoteSpecialization,
    courses: &[RemoteCourse],
    outlines: &HashMap<String, Result<Outline, String>>,
) -> Result<i64, String> {
    slug(&spec.slug)?;
    if spec.id.is_empty() || spec.name.trim().is_empty() {
        return Err("Coursera returned incomplete specialisation details.".into());
    }
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let id = tx.query_row(
        "INSERT INTO specializations(remote_id,slug,name,description,tagline,image_url,synced_at) VALUES (?1,?2,?3,?4,?5,?6,unixepoch()*1000)
         ON CONFLICT(remote_id) DO UPDATE SET slug=excluded.slug,name=excluded.name,description=excluded.description,
         tagline=excluded.tagline,image_url=excluded.image_url,synced_at=unixepoch()*1000,last_sync_error=NULL,updated_at=unixepoch()*1000 RETURNING id",
        params![spec.id,spec.slug,spec.name,spec.description,spec.tagline,if spec.photo_url.is_empty() { &spec.logo } else { &spec.photo_url }],
        |r| r.get::<_,i64>(0),
    ).map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM specialization_courses WHERE specialization_id=?",
        [id],
    )
    .map_err(|e| e.to_string())?;
    let mut saved_courses = Vec::new();
    for (position, remote_id) in spec.course_ids.iter().enumerate() {
        let course = courses
            .iter()
            .find(|c| &c.id == remote_id)
            .ok_or("Missing linked course")?;
        let course_id = upsert_course(&tx, course, true)?;
        tx.execute(
            "INSERT INTO specialization_courses(specialization_id,course_id,position)
             VALUES (?1,?2,?3)
             ON CONFLICT(specialization_id,course_id) DO UPDATE SET position=excluded.position",
            params![id, course_id, position as i64],
        )
        .map_err(|e| e.to_string())?;
        saved_courses.push((course_id, remote_id.clone()));
    }
    tx.commit().map_err(|e| e.to_string())?;
    for (course_id, remote_id) in saved_courses {
        if let Some(outline_res) = outlines.get(&remote_id) {
            match outline_res {
                Ok(outline) => {
                    store_outline(conn, course_id, outline)?;
                }
                Err(err) => {
                    conn.execute(
                        "UPDATE courses SET
                            outline_state = CASE WHEN outline_state = 'ready' THEN 'ready' ELSE 'failed' END,
                            last_sync_error = ?1,
                            updated_at = unixepoch() * 1000
                         WHERE id = ?2",
                        params![err, course_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(id)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteModule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub time_commitment: Option<i64>,
    pub lesson_ids: Vec<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteLesson {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub element_ids: Vec<String>,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub time_commitment: Option<i64>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub is_locked: bool,
    #[serde(default)]
    pub time_commitment: Option<i64>,
    pub content_summary: Value,
}

pub struct Outline {
    pub module_ids: Vec<String>,
    pub modules: HashMap<String, RemoteModule>,
    pub lessons: HashMap<String, RemoteLesson>,
    pub items: HashMap<String, RemoteItem>,
}

fn linked_map<T: serde::de::DeserializeOwned>(
    data: &Value,
    key: &str,
) -> Result<HashMap<String, T>, String> {
    let rows = data
        .get("linked")
        .and_then(|v| v.get(key))
        .and_then(Value::as_array)
        .ok_or("Coursera returned an incomplete course outline.")?;
    rows.iter()
        .map(|row| {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .ok_or("Missing outline ID")?
                .to_owned();
            let value =
                serde_json::from_value(row.clone()).map_err(|_| "Invalid course outline entry")?;
            Ok((id, value))
        })
        .collect()
}

pub async fn fetch_outline(client: &CourseEraClient, course_slug: &str) -> Result<Outline, String> {
    let data=client.api_json("/api/onDemandCourseMaterials.v2/", &[
        ("q","slug"),("slug",course_slug),("includes","modules,lessons,items"),
        ("fields","moduleIds,onDemandCourseMaterialModules.v1(name,slug,description,timeCommitment,lessonIds),onDemandCourseMaterialLessons.v1(name,slug,timeCommitment,itemIds,elementIds),onDemandCourseMaterialItems.v2(name,slug,timeCommitment,contentSummary,isLocked)"),
        ("showLockedItems","true"),
    ]).await?;
    let module_ids = serde_json::from_value(
        data.pointer("/elements/0/moduleIds")
            .cloned()
            .ok_or("Course outline is unavailable. Check your Coursera access.")?,
    )
    .map_err(|_| "Invalid module IDs")?;
    Ok(Outline {
        module_ids,
        modules: linked_map(&data, "onDemandCourseMaterialModules.v1")?,
        lessons: linked_map(&data, "onDemandCourseMaterialLessons.v1")?,
        items: linked_map(&data, "onDemandCourseMaterialItems.v2")?,
    })
}

#[derive(Clone)]
pub struct DownloadItem {
    pub id: i64,
    pub remote_id: String,
    pub kind: String,
    pub locked: bool,
}

pub fn store_outline(
    conn: &mut Connection,
    course_id: i64,
    outline: &Outline,
) -> Result<Vec<DownloadItem>, String> {
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for table in ["modules", "lessons", "items"] {
        tx.execute(
            &format!("UPDATE {table} SET removed_at=unixepoch()*1000 WHERE course_id=?"),
            [course_id],
        )
        .map_err(|e| e.to_string())?;
    }
    let mut items = Vec::new();
    for (mpos, mid) in outline.module_ids.iter().enumerate() {
        let m = outline
            .modules
            .get(mid)
            .ok_or("Missing module in Coursera outline")?;
        let module_tc = m.time_commitment.filter(|&v| v >= 0);
        let module_id:i64=tx.query_row("INSERT INTO modules(course_id,remote_id,name,slug,description,position,time_commitment_raw) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(course_id,remote_id) DO UPDATE SET name=excluded.name,slug=excluded.slug,description=excluded.description,position=excluded.position,time_commitment_raw=excluded.time_commitment_raw,removed_at=NULL,updated_at=unixepoch()*1000 RETURNING id",params![course_id,m.id,m.name,m.slug,m.description,mpos as i64,module_tc],|r|r.get(0)).map_err(|e|e.to_string())?;
        for (lpos, lid) in m.lesson_ids.iter().enumerate() {
            let l = outline
                .lessons
                .get(lid)
                .ok_or("Missing lesson in Coursera outline")?;
            let lesson_tc = l.time_commitment.filter(|&v| v >= 0);
            let lesson_id:i64=tx.query_row("INSERT INTO lessons(course_id,module_id,remote_id,name,slug,position,time_commitment_raw) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(course_id,remote_id) DO UPDATE SET module_id=excluded.module_id,name=excluded.name,slug=excluded.slug,position=excluded.position,time_commitment_raw=excluded.time_commitment_raw,removed_at=NULL,updated_at=unixepoch()*1000 RETURNING id",params![course_id,module_id,l.id,l.name,l.slug,lpos as i64,lesson_tc],|r|r.get(0)).map_err(|e|e.to_string())?;
            let lesson_items = if l.item_ids.is_empty() {
                &l.element_ids
            } else {
                &l.item_ids
            };
            for (ipos, iid) in lesson_items.iter().enumerate() {
                let i = outline
                    .items
                    .get(iid)
                    .ok_or("Missing item in Coursera outline")?;
                let kind = i
                    .content_summary
                    .get("typeName")
                    .and_then(Value::as_str)
                    .ok_or("Missing item content type")?;
                let item_tc = i.time_commitment.filter(|&v| v >= 0);
                let duration_ms = i
                    .content_summary
                    .pointer("/definition/duration")
                    .and_then(Value::as_i64)
                    .filter(|&v| v >= 0);
                let item_id:i64=tx.query_row("INSERT INTO items(course_id,lesson_id,remote_id,name,slug,content_type,position,is_locked,duration_ms,time_commitment_raw,content_summary_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(course_id,remote_id) DO UPDATE SET lesson_id=excluded.lesson_id,name=excluded.name,slug=excluded.slug,content_type=excluded.content_type,position=excluded.position,is_locked=excluded.is_locked,duration_ms=excluded.duration_ms,time_commitment_raw=excluded.time_commitment_raw,content_summary_json=excluded.content_summary_json,removed_at=NULL,updated_at=unixepoch()*1000 RETURNING id",params![course_id,lesson_id,i.id,i.name,i.slug,kind,ipos as i64,i.is_locked,duration_ms,item_tc,i.content_summary.to_string()],|r|r.get(0)).map_err(|e|e.to_string())?;
                items.push(DownloadItem {
                    id: item_id,
                    remote_id: i.id.clone(),
                    kind: kind.into(),
                    locked: i.is_locked,
                });
            }
        }
    }
    tx.execute("UPDATE courses SET outline_state='ready',outline_synced_at=unixepoch()*1000,last_sync_error=NULL,updated_at=unixepoch()*1000 WHERE id=?",[course_id]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(items)
}

pub fn load_outline_items(conn: &Connection, course_id: i64) -> Result<Vec<DownloadItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT i.id, i.remote_id, i.content_type, i.is_locked != 0
             FROM items i
             JOIN lessons l ON l.id = i.lesson_id
             JOIN modules m ON m.id = l.module_id
             WHERE i.course_id = ?1
               AND i.removed_at IS NULL
               AND l.removed_at IS NULL
               AND m.removed_at IS NULL
             ORDER BY m.position, l.position, i.position, i.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([course_id], |r| {
            Ok(DownloadItem {
                id: r.get(0)?,
                remote_id: r.get(1)?,
                kind: r.get(2)?,
                locked: r.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn incomplete_reimport_preserves_existing_path() {
        let pool = crate::test_support::pool();
        let mut conn = pool.get().unwrap();
        let course: RemoteCourse =
            serde_json::from_value(json!({"id":"c1","slug":"rust","name":"Rust"})).unwrap();
        let mut spec: RemoteSpecialization = serde_json::from_value(
            json!({"id":"s1","slug":"systems","name":"Systems","courseIds":["c1"]}),
        )
        .unwrap();
        let id = import_specialization(&mut conn, &spec, &[course.clone()]).unwrap();
        spec.name = "Changed".into();
        spec.course_ids = vec!["c1".into(), "missing".into()];
        assert!(import_specialization(&mut conn, &spec, &[course]).is_err());
        assert_eq!(
            conn.query_row("SELECT name FROM specializations WHERE id=?", [id], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "Systems"
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM specialization_courses WHERE specialization_id=?",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert!(slug("https://coursera.org/learn/rust").is_err());
        assert_eq!(slug(" rust ").unwrap(), "rust");
    }
}
