//! Persistent background download queue. One job plans at a time; file transfers
//! within that job run concurrently up to its saved configuration limit.
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::{StreamExt, stream};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::{courseeraclient::CourseEraClient, coursera};

type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Settings {
    pub height: i64,
    pub format: String,
    pub fallback: String,
    pub languages: Vec<String>,
    pub subtitles: bool,
    pub supplements: bool,
    pub concurrency: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Selection {
    course_ids: Vec<i64>,
    settings: Settings,
    /// Items already downloaded or reserved by another job when this was queued.
    #[serde(default)]
    skip_item_ids: Vec<i64>,
}

pub struct Downloads {
    pool: DbPool,
    pub client: Arc<CourseEraClient>,
    pub downloads_dir: PathBuf,
    /// Serialises control actions with acknowledgement from the current worker.
    execution: std::sync::Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Clone, Debug)]
struct Asset {
    kind: String,
    variant: String,
    format: String,
    height: Option<i64>,
    language: Option<String>,
    source: Option<String>,
    inline: Option<String>,
}

#[derive(Clone)]
struct Task {
    id: i64,
    asset_id: i64,
    source: Option<String>,
    inline: Option<String>,
    relative: String,
}

impl Downloads {
    pub fn new(pool: DbPool, client: Arc<CourseEraClient>) -> Arc<Self> {
        Self::with_dir(pool, client, crate::downloads_dir())
    }

    pub fn with_dir(
        pool: DbPool,
        client: Arc<CourseEraClient>,
        downloads_dir: PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            pool,
            client,
            downloads_dir,
            execution: std::sync::Mutex::new(HashMap::new()),
        })
    }

    fn execution_lock(&self, id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.execution
            .lock()
            .unwrap()
            .entry(id)
            .or_default()
            .clone()
    }

    pub(crate) async fn db<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get().map_err(|_| "Database unavailable".to_string())?;
            f(&mut conn)
        })
        .await
        .map_err(|_| "Database operation interrupted".to_string())?
    }

    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        self.db(|c| {
            c.execute_batch("UPDATE download_tasks SET state='queued',updated_at=unixepoch()*1000 WHERE state='running';
                UPDATE download_jobs SET state='queued',revision=revision+1,updated_at=unixepoch()*1000 WHERE state IN ('running','planning');").map_err(|e|e.to_string())
        }).await?;
        let worker = self.clone();
        tokio::spawn(async move {
            loop {
                match worker.claim().await {
                    Ok(Some((id, selection))) => {
                        let execution = worker.execution_lock(id);
                        let _execution = execution.lock().await;
                        let result = tokio::select! {
                            result=worker.process(id,selection)=>result,
                            _=worker.wait_for_stop(id)=>Ok(()),
                        };
                        if let Err(error) = result {
                            let _=worker.db(move |c| {
                                c.execute("UPDATE download_jobs SET state='failed',last_error=?1,finished_at=unixepoch()*1000,updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?2 AND state NOT IN ('cancelled','paused')",params![error,id]).map_err(|e|e.to_string())?;
                                c.execute("UPDATE download_tasks SET state='failed',last_error_message=?1,updated_at=unixepoch()*1000 WHERE job_id=?2 AND state IN ('queued','running') AND (SELECT state FROM download_jobs WHERE id=?2)='failed'",params![error,id]).map_err(|e|e.to_string())?;
                                Ok(())
                            }).await;
                        }
                    }
                    Ok(None) => {
                        tokio::time::sleep(Duration::from_millis(750)).await;
                    }
                    Err(error) => {
                        eprintln!("Download queue: {error}");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        });
        Ok(())
    }

    pub async fn enqueue(
        &self,
        course_id: Option<i64>,
        specialization_id: Option<i64>,
    ) -> Result<i64, String> {
        let downloads_dir = self.downloads_dir.clone();
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            let course_ids=if let Some(id)=course_id {
                let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM courses WHERE id=? AND archived_at IS NULL)",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
                if !exists { return Err("Course not found or archived.".into()); }
                vec![id]
            } else if let Some(id)=specialization_id {
                let mut stmt=tx.prepare("SELECT sc.course_id FROM specialization_courses sc JOIN courses c ON c.id=sc.course_id WHERE sc.specialization_id=? AND c.archived_at IS NULL ORDER BY sc.position").map_err(|e|e.to_string())?;
                let ids=stmt.query_map([id],|r|r.get::<_,i64>(0)).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
                if ids.is_empty() { return Err("This specialisation has no active courses.".into()); }
                ids
            } else { return Err("Choose a course or specialisation.".into()); };
            let settings=tx.query_row("SELECT preferred_video_height,preferred_video_format,quality_fallback_policy,subtitle_languages_json,download_subtitles,download_supplements,max_concurrent_downloads FROM application_settings WHERE id=1",[],|r| {
                let languages:String=r.get(3)?;
                Ok(Settings { height:r.get(0)?,format:r.get(1)?,fallback:r.get(2)?,languages:serde_json::from_str(&languages).unwrap_or_default(),subtitles:r.get(4)?,supplements:r.get(5)?,concurrency:r.get::<_,i64>(6)? as usize })
            }).map_err(|e|e.to_string())?;
            let (course_ids, skip_item_ids, existing_job) = missing_selection(&tx, &course_ids, &settings, &downloads_dir, None)?;
            if course_ids.is_empty() {
                // Repeated clicks are successful no-ops when everything is reserved or ready.
                let id = existing_job.unwrap_or(0);
                tx.commit().map_err(|e|e.to_string())?;
                return Ok(id);
            }
            let selection=serde_json::to_string(&Selection { course_ids,settings,skip_item_ids }).map_err(|e|e.to_string())?;
            tx.execute("INSERT INTO download_jobs(course_id,specialization_id,state,selection_json) VALUES (?1,?2,'queued',?3)",params![course_id,specialization_id,selection]).map_err(|e|e.to_string())?;
            let id=tx.last_insert_rowid();
            tx.commit().map_err(|e|e.to_string())?;
            Ok(id)
        }).await
    }

    async fn claim(&self) -> Result<Option<(i64, String)>, String> {
        self.db(|c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            let job=tx.query_row("SELECT id,selection_json FROM download_jobs WHERE state='queued' ORDER BY created_at,id LIMIT 1",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?))).optional().map_err(|e|e.to_string())?;
            if let Some((id,_))=&job { tx.execute("UPDATE download_jobs SET state='planning',started_at=coalesce(started_at,unixepoch()*1000),updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?",[id]).map_err(|e|e.to_string())?; }
            tx.commit().map_err(|e|e.to_string())?;
            Ok(job)
        }).await
    }

    /// True when the job was cancelled or paused and the worker should stop.
    async fn stopped(&self, id: i64) -> Result<bool, String> {
        self.db(move |c| {
            c.query_row(
                "SELECT state IN ('cancelled','paused') FROM download_jobs WHERE id=?",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
        })
        .await
    }

    async fn wait_for_stop(&self, id: i64) {
        loop {
            if self.stopped(id).await.unwrap_or(false) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
    }

    pub(crate) async fn cancel(&self, id: i64) -> Result<(), String> {
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            let state=tx.query_row("SELECT state FROM download_jobs WHERE id=?",[id],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?.ok_or("Download not found.")?;
            if ["completed","cancelled"].contains(&state.as_str()) { return Err("This download has already finished.".into()); }
            tx.execute("UPDATE download_jobs SET state='cancelled',finished_at=unixepoch()*1000,updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?",[id]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state='cancelled',finished_at=unixepoch()*1000,updated_at=unixepoch()*1000 WHERE job_id=? AND state NOT IN ('completed','skipped')",[id]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?;
            Ok(())
        }).await
    }

    /// Pause an active job. The worker notices within a moment and stops; partially
    /// downloaded files are kept so the transfer can continue from where it left off.
    pub(crate) async fn pause(&self, id: i64) -> Result<(), String> {
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            let state=tx.query_row("SELECT state FROM download_jobs WHERE id=?",[id],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?.ok_or("Download not found.")?;
            if state == "paused" { return Err("This download is already paused.".into()); }
            if !["planning","queued","running"].contains(&state.as_str()) { return Err("Only active downloads can be paused.".into()); }
            tx.execute("UPDATE download_jobs SET state='paused',updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?",[id]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state='paused',updated_at=unixepoch()*1000 WHERE job_id=? AND state IN ('queued','running')",[id]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?;
            Ok(())
        }).await?;
        // Keep the paused state visible until the worker has dropped its transfers,
        // so a quick Resume cannot cause it to miss the pause.
        let execution = self.execution_lock(id);
        let _execution = execution.lock().await;
        Ok(())
    }

    /// Put a paused job back in the queue at its original position.
    pub(crate) async fn resume(&self, id: i64) -> Result<(), String> {
        let execution = self.execution_lock(id);
        let _execution = execution.lock().await;
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            let state=tx.query_row("SELECT state FROM download_jobs WHERE id=?",[id],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?.ok_or("Download not found.")?;
            if state != "paused" { return Err("Only paused downloads can be resumed.".into()); }
            tx.execute("UPDATE download_jobs SET state='queued',updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?",[id]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state='queued',updated_at=unixepoch()*1000 WHERE job_id=? AND state IN ('paused','running')",[id]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?;
            Ok(())
        }).await
    }

    /// Re-queue a failed job. Files that already finished are reused; only the
    /// missing or failed ones are fetched again.
    pub(crate) async fn retry(&self, id: i64) -> Result<(), String> {
        let downloads_dir = self.downloads_dir.clone();
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            let state=tx.query_row("SELECT state FROM download_jobs WHERE id=?",[id],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?.ok_or("Download not found.")?;
            if !["failed","partial_failure"].contains(&state.as_str()) { return Err("Only failed downloads can be retried.".into()); }
            let json: String = tx.query_row("SELECT selection_json FROM download_jobs WHERE id=?",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
            let mut selection: Selection = serde_json::from_str(&json).map_err(|e|e.to_string())?;
            let (courses, skipped, _) = missing_selection(&tx, &selection.course_ids, &selection.settings, &downloads_dir, Some(id))?;
            if courses.is_empty() { return Ok(()); }
            selection.course_ids = courses;
            selection.skip_item_ids.extend(skipped);
            let json = serde_json::to_string(&selection).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_jobs SET selection_json=?1 WHERE id=?2",params![json,id]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_jobs SET state='queued',last_error=NULL,finished_at=NULL,updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?",[id]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state='queued',last_error_message=NULL,updated_at=unixepoch()*1000 WHERE job_id=? AND state IN ('failed','cancelled','paused','running')",[id]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?;
            Ok(())
        }).await
    }

    async fn process(self: &Arc<Self>, job_id: i64, selection: String) -> Result<(), String> {
        let selection: Selection = serde_json::from_str(&selection).map_err(
            |_| "Download settings are invalid. Cancel this job and start a new download.",
        )?;
        self.db(move |c| {
            c.execute(
                "DELETE FROM download_tasks WHERE job_id=? AND state!='completed'",
                [job_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
        .await?;
        let mut tasks = Vec::new();
        let mut reused_tasks = 0usize;
        let mut saw_locked_item = false;
        for course_id in selection.course_ids {
            if self.stopped(job_id).await? {
                return Ok(());
            }
            let downloads_dir = self.downloads_dir.clone();
            let settings_clone = selection.settings.clone();
            let skipped = selection.skip_item_ids.clone();
            let (remote_id, slug, outline_state, spec_remote_ids, stored_items, course_reused) =
                self.db(move |c| {
                    let (remote_id, slug, outline_state) = c
                        .query_row(
                            "SELECT remote_id,slug,outline_state FROM courses WHERE id=?",
                            [course_id],
                            |r| {
                                Ok((
                                    r.get::<_, String>(0)?,
                                    r.get::<_, String>(1)?,
                                    r.get::<_, String>(2)?,
                                ))
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    let specs = {
                        let mut stmt = c
                            .prepare(
                                "SELECT s.remote_id FROM specializations s
                                 JOIN specialization_courses sc ON sc.specialization_id = s.id
                                 WHERE sc.course_id = ?",
                            )
                            .map_err(|e| e.to_string())?;
                        stmt.query_map([course_id], |r| r.get::<_, String>(0))
                            .map_err(|e| e.to_string())?
                            .collect::<rusqlite::Result<Vec<_>>>()
                            .map_err(|e| e.to_string())?
                    };
                    let mut stored_items = if outline_state == "ready" {
                        coursera::load_outline_items(c, course_id).unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    stored_items.retain(|item| !skipped.contains(&item.id));
                    let course_reused = if outline_state == "ready" && !stored_items.is_empty() {
                        try_reuse_ready_course(
                            c,
                            job_id,
                            &stored_items,
                            &downloads_dir,
                            &settings_clone,
                        )?
                    } else {
                        None
                    };
                    Ok((
                        remote_id,
                        slug,
                        outline_state,
                        specs,
                        stored_items,
                        course_reused,
                    ))
                })
                .await?;
            if let Some(reused) = course_reused {
                reused_tasks += reused;
                continue;
            }
            for spec_remote_id in spec_remote_ids {
                self.client
                    .ensure_specialization_enrolled(&spec_remote_id)
                    .await;
            }
            let items = match coursera::fetch_outline_with_enrollment(
                &self.client,
                &remote_id,
                &slug,
            )
            .await
            {
                Ok(outline) => {
                    self.db(move |c| coursera::store_outline(c, course_id, &outline))
                        .await?
                }
                Err(error) => {
                    if outline_state == "ready" && !stored_items.is_empty() {
                        stored_items
                    } else {
                        let message = error.clone();
                        self.db(move |c| {
                            c.execute(
                                "UPDATE courses SET outline_state='failed',last_sync_error=?1,updated_at=unixepoch()*1000 WHERE id=?2",
                                params![message, course_id],
                            )
                            .map_err(|e| e.to_string())?;
                            Ok(())
                        })
                        .await?;
                        return Err(error);
                    }
                }
            };
            for item in items {
                if selection.skip_item_ids.contains(&item.id) {
                    continue;
                }
                if self.stopped(job_id).await? {
                    return Ok(());
                }
                if !is_item_downloadable(&item.kind, &selection.settings) {
                    continue;
                }
                if item.locked {
                    saw_locked_item = true;
                    continue;
                }
                let downloads_dir = self.downloads_dir.clone();
                let settings_clone = selection.settings.clone();
                let item_clone = item.clone();
                let item_reused = self
                    .db(move |c| {
                        reuse_ready_item_assets(
                            c,
                            job_id,
                            &item_clone,
                            &downloads_dir,
                            &settings_clone,
                        )
                    })
                    .await?;
                if item_reused > 0 {
                    reused_tasks += item_reused;
                    continue;
                }
                let assets = self.resolve(&remote_id, &item, &selection.settings).await?;
                let course_slug = slug.clone();
                let downloads_dir = self.downloads_dir.clone();
                let (planned, reused) = self
                    .db(move |c| {
                        plan_assets(c, job_id, item.id, &course_slug, &downloads_dir, assets)
                    })
                    .await?;
                reused_tasks += reused;
                tasks.extend(planned);
            }
        }
        if self.stopped(job_id).await? {
            return Ok(());
        }
        if tasks.is_empty() && reused_tasks == 0 {
            if saw_locked_item {
                return Err(
                    "Course materials are locked. Check your Coursera enrollment and cookie.txt."
                        .into(),
                );
            }
            return Err("No downloadable videos or resources were found.".into());
        }
        self.db(move |c| { c.execute("UPDATE download_jobs SET state='running',updated_at=unixepoch()*1000,revision=revision+1 WHERE id=? AND state='planning'",[job_id]).map_err(|e|e.to_string())?; Ok(()) }).await?;
        // Keep transfer futures inside this process so a pause drops them before
        // acknowledging the pause or allowing a resume to open the same files.
        let mut active = stream::iter(tasks).map(|task| {
            let worker = self.clone();
            async move {
                let task_id=task.id;
                if let Err(error)=worker.transfer(job_id,task).await {
                    let _=worker.db(move |c| { c.execute("UPDATE download_tasks SET state='failed',last_error_message=?1,updated_at=unixepoch()*1000 WHERE id=?2 AND state NOT IN ('cancelled','paused')",params![error,task_id]).map_err(|e|e.to_string())?; Ok(()) }).await;
                }
            }
        }).buffer_unordered(selection.settings.concurrency.clamp(1, 32));
        while active.next().await.is_some() {}
        self.db(move |c| {
            let (total,done,failed):(i64,i64,i64)=c.query_row("SELECT count(*),coalesce(sum(state IN ('completed','skipped')),0),coalesce(sum(state='failed'),0) FROM download_tasks WHERE job_id=?",[job_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
            let state=if total>0 && done==total { "completed" } else if done>0 { "partial_failure" } else { "failed" };
            let error=if failed>0 { Some(format!("{failed} file(s) failed. Use Retry to try again.")) } else { None };
            c.execute("UPDATE download_jobs SET state=?1,last_error=?2,finished_at=unixepoch()*1000,updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?3 AND state NOT IN ('cancelled','paused')",params![state,error,job_id]).map_err(|e|e.to_string())?;
            Ok(())
        }).await
    }

    async fn resolve(
        &self,
        course: &str,
        item: &coursera::DownloadItem,
        settings: &Settings,
    ) -> Result<Vec<Asset>, String> {
        if item.kind == "lecture" {
            let data = self
                .client
                .api_json(
                    &format!("/api/onDemandLectureVideos.v1/{course}~{}", item.remote_id),
                    &[
                        ("includes", "video"),
                        (
                            "fields",
                            "onDemandVideos.v1(sources,subtitles,subtitlesVtt,subtitlesTxt)",
                        ),
                    ],
                )
                .await?;
            let video = data
                .pointer("/linked/onDemandVideos.v1/0")
                .ok_or("Lecture video is unavailable. Check Coursera access.")?;
            lecture_assets(video, settings)
        } else {
            let data = self
                .client
                .api_json(
                    &format!("/api/onDemandSupplements.v1/{course}~{}", item.remote_id),
                    &[
                        ("includes", "asset"),
                        ("fields", "openCourseAssets.v1(typeName,definition)"),
                    ],
                )
                .await?;
            supplement_assets(&data)
        }
    }

    async fn transfer(&self, job_id: i64, task: Task) -> Result<(), String> {
        if self.stopped(job_id).await? {
            return Ok(());
        }
        let asset_id = task.asset_id;
        let task_id = task.id;
        let existing=self.db(move |c| c.query_row("SELECT storage_root,relative_path,size_bytes FROM local_files WHERE asset_id=? AND state='ready'",[asset_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).optional().map_err(|e|e.to_string())).await?;
        if let Some((old_root, relative, size)) = existing {
            if tokio::fs::metadata(self.downloads_dir.join(&relative))
                .await
                .is_ok_and(|m| m.len() == size as u64)
                || tokio::fs::metadata(PathBuf::from(old_root).join(&relative))
                    .await
                    .is_ok_and(|m| m.len() == size as u64)
            {
                self.progress(task_id, asset_id, size, Some(size), true)
                    .await?;
                return Ok(());
            }
        }
        let root = &self.downloads_dir;
        let path = root.join(&task.relative);
        let temporary = path.with_extension(format!(
            "{}.part",
            path.extension().unwrap_or_default().to_string_lossy()
        ));
        tokio::fs::create_dir_all(path.parent().ok_or("Invalid file path")?)
            .await
            .map_err(|e| format!("Could not create download directory: {e}"))?;
        // A leftover `.part` file (from a pause or interruption) lets us continue
        // with a ranged request instead of starting over.
        let existing_partial = if task.source.is_some() {
            tokio::fs::metadata(&temporary)
                .await
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };
        let mut resume_from = 0u64;
        let mut response = if let Some(source) = &task.source {
            let ranged = if existing_partial > 0 {
                self.client
                    .asset_response(source, existing_partial)
                    .await
                    .ok()
            } else {
                None
            };
            match ranged {
                Some(r)
                    if r.status() == reqwest::StatusCode::PARTIAL_CONTENT
                        && r.headers()
                            .get(reqwest::header::CONTENT_RANGE)
                            .and_then(|value| value.to_str().ok())
                            .and_then(|value| value.strip_prefix("bytes "))
                            .and_then(|value| value.split_once('-'))
                            .and_then(|(start, _)| start.parse::<u64>().ok())
                            == Some(existing_partial) =>
                {
                    resume_from = existing_partial;
                    Some(r)
                }
                Some(r) if r.status() == reqwest::StatusCode::OK => Some(r),
                _ => Some(self.client.asset_response(source, 0).await?),
            }
        } else {
            None
        };
        if self.stopped(job_id).await? {
            return Ok(());
        }
        let total = task.inline.as_ref().map(|s| s.len() as i64).or_else(|| {
            response
                .as_ref()
                .and_then(|r| r.content_length())
                .map(|n| (n + resume_from) as i64)
        });
        let storage_root = root.to_string_lossy().into_owned();
        let relative = task.relative.clone();
        let temp_relative = format!("{}.part", task.relative);
        let start_bytes = resume_from as i64;
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            tx.execute("INSERT INTO local_files(asset_id,storage_root,relative_path,temporary_relative_path,state,bytes_present,size_bytes) VALUES (?1,?2,?3,?4,'partial',?6,?5) ON CONFLICT(asset_id) DO UPDATE SET storage_root=excluded.storage_root,relative_path=excluded.relative_path,temporary_relative_path=excluded.temporary_relative_path,state='partial',bytes_present=excluded.bytes_present,size_bytes=excluded.size_bytes,completed_at=NULL,updated_at=unixepoch()*1000",params![asset_id,storage_root,relative,temp_relative,total,start_bytes]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state='running',downloaded_bytes=?3,total_bytes=?1,attempt_count=attempt_count+1,started_at=unixepoch()*1000,updated_at=unixepoch()*1000 WHERE id=?2 AND state NOT IN ('cancelled','paused')",params![total,task_id,start_bytes]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?; Ok(())
        }).await?;
        let mut file = if resume_from > 0 {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&temporary)
                .await
                .map_err(|e| format!("Could not open course file: {e}"))?
        } else {
            tokio::fs::File::create(&temporary)
                .await
                .map_err(|e| format!("Could not create course file: {e}"))?
        };
        let mut downloaded = resume_from as i64;
        let mut last_update = Instant::now();
        if let Some(content) = task.inline {
            file.write_all(content.as_bytes())
                .await
                .map_err(|e| format!("Could not write course file: {e}"))?;
            downloaded = content.len() as i64;
        } else if let Some(response) = response.as_mut() {
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "File transfer interrupted. Use Retry to try again.")?
            {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Could not write course file: {e}"))?;
                downloaded += chunk.len() as i64;
                if total.is_some_and(|n| downloaded > n) {
                    return Err("File size exceeded its reported length.".into());
                }
                if last_update.elapsed() >= Duration::from_millis(1000) {
                    self.progress(task_id, asset_id, downloaded, total, false)
                        .await?;
                    last_update = Instant::now();
                }
            }
        } else {
            return Err("Course file has no source.".into());
        }
        if total.is_some_and(|n| downloaded != n) {
            return Err("Downloaded file size does not match its expected length.".into());
        }
        file.flush().await.map_err(|e| e.to_string())?;
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        if self.stopped(job_id).await? {
            return Ok(());
        }
        tokio::fs::rename(&temporary, &path)
            .await
            .map_err(|e| format!("Could not finish course file: {e}"))?;
        self.progress(task_id, asset_id, downloaded, Some(downloaded), true)
            .await
    }

    async fn progress(
        &self,
        task: i64,
        asset: i64,
        bytes: i64,
        total: Option<i64>,
        complete: bool,
    ) -> Result<(), String> {
        self.db(move |c| {
            let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state=CASE WHEN ?1 THEN 'completed' ELSE state END,downloaded_bytes=?2,total_bytes=?3,finished_at=CASE WHEN ?1 THEN unixepoch()*1000 ELSE finished_at END,updated_at=unixepoch()*1000 WHERE id=?4 AND state NOT IN ('cancelled','paused')",params![complete,bytes,total,task]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE local_files SET state=CASE WHEN ?1 THEN 'ready' ELSE 'partial' END,bytes_present=?2,size_bytes=?3,completed_at=CASE WHEN ?1 THEN unixepoch()*1000 END,temporary_relative_path=CASE WHEN ?1 THEN NULL ELSE temporary_relative_path END,updated_at=unixepoch()*1000 WHERE asset_id=?4",params![complete,bytes,total,asset]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?; Ok(())
        }).await
    }
}

fn lecture_assets(video: &Value, settings: &Settings) -> Result<Vec<Asset>, String> {
    let resolutions = video
        .pointer("/sources/byResolution")
        .and_then(Value::as_object)
        .ok_or("Video sources are unavailable.")?;
    let key = if settings.format == "webm" {
        "webMVideoUrl"
    } else {
        "mp4VideoUrl"
    };
    let selected = resolutions
        .iter()
        .filter_map(|(resolution, value)| {
            let height = resolution.trim_end_matches('p').parse::<i64>().ok()?;
            let url = value.get(key)?.as_str().filter(|v| !v.is_empty())?;
            ((settings.fallback == "exact" && height == settings.height)
                || (settings.fallback != "exact" && height <= settings.height))
                .then_some((height, url))
        })
        .max_by_key(|(height, _)| *height)
        .ok_or("The requested video quality or format is unavailable.")?;
    let mut assets = vec![Asset {
        kind: "video".into(),
        variant: format!("video:{}:{}p", settings.format, selected.0),
        format: settings.format.clone(),
        height: Some(selected.0),
        language: None,
        source: Some(selected.1.into()),
        inline: None,
    }];
    if settings.subtitles {
        for language in &settings.languages {
            for (field, format) in [
                ("subtitlesVtt", "vtt"),
                ("subtitles", "srt"),
                ("subtitlesTxt", "txt"),
            ] {
                if let Some(url) = video
                    .get(field)
                    .and_then(|v| v.get(language))
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
                {
                    assets.push(Asset {
                        kind: "subtitle".into(),
                        variant: format!("subtitle:{format}:{language}"),
                        format: format.into(),
                        height: None,
                        language: Some(language.clone()),
                        source: Some(url.into()),
                        inline: None,
                    });
                    break;
                }
            }
        }
    }
    Ok(assets)
}

fn supplement_assets(data: &Value) -> Result<Vec<Asset>, String> {
    let rows = data
        .pointer("/linked/openCourseAssets.v1")
        .and_then(Value::as_array)
        .ok_or("Supplementary resources are unavailable.")?;
    let mut assets = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let definition = row
            .get("definition")
            .ok_or("Invalid supplementary resource")?;
        if let Some(html) = definition
            .pointer("/renderableHtmlWithMetadata/renderableHtml")
            .or_else(|| definition.get("value"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            assets.push(Asset {
                kind: "reading".into(),
                variant: format!("reading:{index}"),
                format: "html".into(),
                height: None,
                language: None,
                source: None,
                inline: Some(html.into()),
            });
        } else if let Some(url) = definition
            .get("url")
            .or_else(|| definition.get("downloadUrl"))
            .and_then(Value::as_str)
        {
            assets.push(Asset {
                kind: "attachment".into(),
                variant: format!("attachment:{index}"),
                format: "bin".into(),
                height: None,
                language: None,
                source: Some(url.into()),
                inline: None,
            });
        } else {
            return Err("This supplementary resource has an unsupported format.".into());
        }
    }
    if assets.is_empty() {
        return Err("No supplementary resource was returned by Coursera.".into());
    }
    Ok(assets)
}

fn safe_component(value: &str) -> String {
    let clean: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(100)
        .collect();
    if clean.is_empty() {
        "course".into()
    } else {
        clean
    }
}

fn is_item_downloadable(kind: &str, settings: &Settings) -> bool {
    kind == "lecture" || (settings.supplements && ["supplement", "reading"].contains(&kind))
}

/// Reserve work across both course and specialisation selections, including jobs
/// that have not planned their files yet. Failed items in a running job can be
/// picked up by a follow-up job without touching its other tasks.
fn missing_selection(
    conn: &Connection,
    course_ids: &[i64],
    settings: &Settings,
    downloads_dir: &std::path::Path,
    exclude_job: Option<i64>,
) -> Result<(Vec<i64>, Vec<i64>, Option<i64>), String> {
    let mut stmt = conn.prepare("SELECT id,state,selection_json FROM download_jobs WHERE state IN ('queued','planning','running','paused') AND id!=coalesce(?1,0) ORDER BY id").map_err(|e|e.to_string())?;
    let active = stmt
        .query_map([exclude_job], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let active = active
        .into_iter()
        .map(|(id, state, json)| {
            serde_json::from_str::<Selection>(&json)
                .map(|selection| (id, state, selection))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut selected = Vec::new();
    let mut skipped = Vec::new();
    let mut existing = None;
    for &course in course_ids {
        let covering: Vec<_> = active
            .iter()
            .filter(|(_, _, s)| s.course_ids.contains(&course))
            .collect();
        existing = existing.or_else(|| covering.first().map(|j| j.0));
        let items = coursera::load_outline_items(conn, course)?;
        if items.is_empty() {
            if covering.is_empty() {
                selected.push(course);
            }
            continue;
        }
        let mut missing = false;
        for item in items {
            if !is_item_downloadable(&item.kind, settings) {
                continue;
            }
            let reserved = covering.iter().any(|(id,state,selection)| {
                if selection.skip_item_ids.contains(&item.id) { return false; }
                if state != "running" { return true; }
                // Planning is finished in a running job. Only explicitly failed
                // items are eligible while its other transfers continue.
                !conn.query_row("SELECT EXISTS(SELECT 1 FROM download_tasks t JOIN assets a ON a.id=t.asset_id WHERE t.job_id=?1 AND a.item_id=?2 AND t.state='failed')",params![id,item.id],|r|r.get::<_,bool>(0)).unwrap_or(false)
            });
            if reserved || item_ready_assets(conn, &item, downloads_dir, settings)?.is_some() {
                skipped.push(item.id);
            } else {
                missing = true;
            }
        }
        if missing {
            selected.push(course);
        }
    }
    if existing.is_none() {
        existing = conn
            .query_row(
                "SELECT id FROM download_jobs WHERE EXISTS(SELECT 1 FROM json_each(selection_json,'$.course_ids') WHERE value IN (SELECT value FROM json_each(?1))) ORDER BY id DESC LIMIT 1",
                [serde_json::to_string(course_ids).map_err(|e|e.to_string())?],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
    }
    Ok((selected, skipped, existing))
}

fn is_local_file_ready(
    downloads_dir: &std::path::Path,
    storage_root: &str,
    relative_path: &str,
    size_bytes: i64,
) -> bool {
    if relative_path.is_empty() || size_bytes < 0 {
        return false;
    }
    let expected = size_bytes as u64;
    std::fs::metadata(downloads_dir.join(relative_path)).is_ok_and(|m| m.len() == expected)
        || (!storage_root.is_empty()
            && std::fs::metadata(PathBuf::from(storage_root).join(relative_path))
                .is_ok_and(|m| m.len() == expected))
}

fn item_ready_assets(
    conn: &Connection,
    item: &coursera::DownloadItem,
    downloads_dir: &std::path::Path,
    settings: &Settings,
) -> Result<Option<Vec<(i64, i64)>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.kind, a.format, a.height,
                    coalesce(f.state, ''), coalesce(f.storage_root, ''), coalesce(f.relative_path, ''), coalesce(f.size_bytes, -1)
             FROM assets a
             LEFT JOIN local_files f ON f.asset_id = a.id
             WHERE a.item_id = ?1 AND a.removed_at IS NULL",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([item.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut has_matching_video = item.kind != "lecture";
    let mut ready = Vec::with_capacity(rows.len());
    for (asset_id, kind, format, height, state, storage_root, relative_path, size_bytes) in rows {
        if state != "ready"
            || !is_local_file_ready(downloads_dir, &storage_root, &relative_path, size_bytes)
        {
            return Ok(None);
        }
        if kind == "video" {
            let height_ok = settings.fallback != "exact" || height == Some(settings.height);
            if format == settings.format && height_ok {
                has_matching_video = true;
            } else {
                return Ok(None);
            }
        }
        ready.push((asset_id, size_bytes));
    }
    if !has_matching_video {
        return Ok(None);
    }
    Ok(Some(ready))
}

fn mark_assets_completed_in_tx(
    tx: &rusqlite::Transaction<'_>,
    job: i64,
    assets: &[(i64, i64)],
) -> Result<(), String> {
    for &(asset_id, size_bytes) in assets {
        tx.execute(
            "INSERT INTO download_tasks(job_id, asset_id, state, downloaded_bytes, total_bytes, started_at, finished_at, updated_at)
             VALUES (?1, ?2, 'completed', ?3, ?3, unixepoch()*1000, unixepoch()*1000, unixepoch()*1000)
             ON CONFLICT(job_id, asset_id) DO UPDATE SET
                 state = 'completed',
                 downloaded_bytes = excluded.downloaded_bytes,
                 total_bytes = excluded.total_bytes,
                 started_at = coalesce(download_tasks.started_at, excluded.started_at),
                 finished_at = coalesce(download_tasks.finished_at, excluded.finished_at),
                 updated_at = unixepoch()*1000",
            params![job, asset_id, size_bytes],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn try_reuse_ready_course(
    conn: &mut Connection,
    job: i64,
    items: &[coursera::DownloadItem],
    downloads_dir: &std::path::Path,
    settings: &Settings,
) -> Result<Option<usize>, String> {
    let mut downloadable_count = 0usize;
    let mut all_ready = Vec::new();
    for item in items {
        if !is_item_downloadable(&item.kind, settings) {
            continue;
        }
        if item.locked {
            return Ok(None);
        }
        downloadable_count += 1;
        match item_ready_assets(conn, item, downloads_dir, settings)? {
            Some(ready) => all_ready.extend(ready),
            None => return Ok(None),
        }
    }
    if downloadable_count == 0 || all_ready.is_empty() {
        return Ok(None);
    }
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let cancelled: bool = tx
        .query_row(
            "SELECT state IN ('cancelled','paused') FROM download_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if cancelled {
        return Ok(Some(0));
    }
    mark_assets_completed_in_tx(&tx, job, &all_ready)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(all_ready.len()))
}

fn reuse_ready_item_assets(
    conn: &mut Connection,
    job: i64,
    item: &coursera::DownloadItem,
    downloads_dir: &std::path::Path,
    settings: &Settings,
) -> Result<usize, String> {
    let Some(ready) = item_ready_assets(conn, item, downloads_dir, settings)? else {
        return Ok(0);
    };
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let cancelled: bool = tx
        .query_row(
            "SELECT state IN ('cancelled','paused') FROM download_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if cancelled {
        return Ok(0);
    }
    mark_assets_completed_in_tx(&tx, job, &ready)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ready.len())
}

fn plan_assets(
    conn: &mut Connection,
    job: i64,
    item: i64,
    course_slug: &str,
    downloads_dir: &std::path::Path,
    assets: Vec<Asset>,
) -> Result<(Vec<Task>, usize), String> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let cancelled: bool = tx
        .query_row(
            "SELECT state IN ('cancelled','paused') FROM download_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if cancelled {
        return Ok((Vec::new(), 0));
    }
    let mut tasks = Vec::new();
    let mut reused = 0usize;
    for asset in assets {
        // A follow-up may include an item with a failed subtitle while its video
        // is still queued or downloading. Leave those reserved assets alone.
        let protected: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM assets a JOIN download_tasks t ON t.asset_id=a.id JOIN download_jobs j ON j.id=t.job_id
             WHERE a.item_id=?1 AND (a.variant_key=?2 OR (?3='video' AND a.kind='video')) AND t.job_id!=?4
             AND t.state IN ('queued','running','paused') AND j.state IN ('queued','planning','running','paused'))",
            params![item,asset.variant,asset.kind,job], |r|r.get(0),
        ).map_err(|e|e.to_string())?;
        if protected {
            continue;
        }
        if asset.kind == "video" {
            tx.execute("UPDATE assets SET removed_at=unixepoch()*1000 WHERE item_id=?1 AND kind='video' AND variant_key!=?2",params![item,asset.variant]).map_err(|e|e.to_string())?;
        }
        let id:i64=tx.query_row("INSERT INTO assets(item_id,kind,variant_key,format,height,language_code,source_url,inline_content,source_resolved_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,unixepoch()*1000) ON CONFLICT(item_id,variant_key) DO UPDATE SET source_url=excluded.source_url,inline_content=excluded.inline_content,source_resolved_at=unixepoch()*1000,removed_at=NULL,updated_at=unixepoch()*1000 RETURNING id",params![item,asset.kind,asset.variant,asset.format,asset.height,asset.language,asset.source,asset.inline],|r|r.get(0)).map_err(|e|e.to_string())?;
        let existing_ready = tx
            .query_row(
                "SELECT storage_root, relative_path, size_bytes FROM local_files WHERE asset_id=? AND state='ready'",
                [id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some((storage_root, relative_path, size_bytes)) = existing_ready {
            if is_local_file_ready(downloads_dir, &storage_root, &relative_path, size_bytes) {
                mark_assets_completed_in_tx(&tx, job, &[(id, size_bytes)])?;
                reused += 1;
                continue;
            }
        }
        let task_id:i64=tx.query_row("INSERT INTO download_tasks(job_id,asset_id) VALUES (?1,?2) ON CONFLICT(job_id,asset_id) DO UPDATE SET state=CASE WHEN download_tasks.state='completed' THEN 'completed' ELSE 'queued' END RETURNING id",params![job,id],|r|r.get(0)).map_err(|e|e.to_string())?;
        tasks.push(Task {
            id: task_id,
            asset_id: id,
            source: asset.source,
            inline: asset.inline,
            relative: format!(
                "{}/item-{item}/{id}.{}",
                safe_component(course_slug),
                safe_component(&asset.format)
            ),
        });
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok((tasks, reused))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "courseviewer-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[actix_web::test]
    async fn repeated_download_only_selects_failed_and_missing_items() {
        let pool = crate::test_support::pool();
        let (base, provider) = crate::test_support::provider().await;
        let client = Arc::new(CourseEraClient::for_test(base));
        let (spec, courses, outlines) = coursera::fetch_specialization_bundle(&client, "systems")
            .await
            .unwrap();
        let spec = coursera::import_specialization_with_outlines(
            &mut pool.get().unwrap(),
            &spec,
            &courses,
            &outlines,
        )
        .unwrap();
        let root = test_dir("selection");
        let worker = Downloads::with_dir(pool.clone(), client, root.clone());
        let course: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT id FROM courses WHERE slug='rust'", [], |r| r.get(0))
            .unwrap();
        let original = worker.enqueue(Some(course), None).await.unwrap();
        // One running course contains a failed video and a queued reading.
        pool.get().unwrap().execute_batch(&format!("UPDATE download_jobs SET state='running' WHERE id={original};
            INSERT INTO assets(item_id,kind,variant_key,format,height) SELECT id,'video','video:mp4:1080p','mp4',1080 FROM items WHERE course_id={course} AND content_type='lecture';
            INSERT INTO assets(item_id,kind,variant_key,format) SELECT id,'reading','reading:0','html' FROM items WHERE course_id={course} AND content_type='supplement';
            INSERT INTO download_tasks(job_id,asset_id,state) SELECT {original},a.id,CASE WHEN a.kind='video' THEN 'failed' ELSE 'queued' END FROM assets a JOIN items i ON i.id=a.item_id WHERE i.course_id={course};")).unwrap();
        let failed_item: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT item_id FROM assets WHERE kind='video'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let queued_item: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT item_id FROM assets WHERE kind='reading'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let followup = worker.enqueue(None, Some(spec)).await.unwrap();
        assert_ne!(followup, original);
        let json: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT selection_json FROM download_jobs WHERE id=?",
                [followup],
                |r| r.get(0),
            )
            .unwrap();
        let selection: Selection = serde_json::from_str(&json).unwrap();
        assert_eq!(
            selection.course_ids.len(),
            2,
            "also includes the untouched course"
        );
        assert!(selection.skip_item_ids.contains(&queued_item));
        assert!(!selection.skip_item_ids.contains(&failed_item));
        worker.enqueue(None, Some(spec)).await.unwrap();
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row("SELECT count(*) FROM download_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2,
            "rapid repeated clicks do not duplicate follow-up jobs"
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT state FROM download_jobs WHERE id=?",
                    [original],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "running"
        );
        assert_eq!(pool.get().unwrap().query_row("SELECT state FROM download_tasks WHERE job_id=? AND asset_id=(SELECT id FROM assets WHERE item_id=?)",params![original,queued_item],|r|r.get::<_,String>(0)).unwrap(),"queued");
        // A retry cannot take files already reserved by the new queued job.
        pool.get()
            .unwrap()
            .execute(
                "UPDATE download_jobs SET state='partial_failure' WHERE id=?",
                [original],
            )
            .unwrap();
        worker.retry(original).await.unwrap();
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT state FROM download_jobs WHERE id=?",
                    [original],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "queued"
        );
        let retried: String = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT selection_json FROM download_jobs WHERE id=?",
                [original],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            serde_json::from_str::<Selection>(&retried)
                .unwrap()
                .skip_item_ids
                .contains(&failed_item)
        );
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row(
                    "SELECT selection_json FROM download_jobs WHERE id=?",
                    [followup],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            json
        );
        provider.stop(true).await;
    }

    #[actix_web::test]
    async fn paused_transfer_resumes_partial_file_and_keeps_completed_files() {
        let pool = crate::test_support::pool();
        let (base, provider) = crate::test_support::provider().await;
        let client = Arc::new(CourseEraClient::for_test(base));
        let (course, outline) = coursera::fetch_course_bundle(&client, "rust")
            .await
            .unwrap();
        let course_id = coursera::upsert_course(&pool.get().unwrap(), &course, true).unwrap();
        coursera::store_outline(&mut pool.get().unwrap(), course_id, &outline.unwrap()).unwrap();
        pool.get()
            .unwrap()
            .execute(
                "UPDATE courses SET remote_id='slow' WHERE id=?",
                [course_id],
            )
            .unwrap();
        let root = test_dir("resume");
        let worker = Downloads::with_dir(pool.clone(), client, root.clone());
        let job = worker.enqueue(Some(course_id), None).await.unwrap();
        worker.start().await.unwrap();
        let partial = tokio::time::timeout(Duration::from_secs(5),async {
            loop {
                let path = worker.db(move |c|c.query_row("SELECT f.temporary_relative_path FROM local_files f JOIN assets a ON a.id=f.asset_id JOIN download_tasks t ON t.asset_id=a.id WHERE t.job_id=? AND a.kind='video' AND t.state='running'",[job],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())).await.unwrap();
                if let Some(path) = path {
                    let path = root.join(path);
                    if tokio::fs::metadata(&path).await.is_ok_and(|m|m.len()>0 && m.len()<256*1024) { break path; }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("transfer starts");
        worker.pause(job).await.unwrap();
        let size = tokio::fs::metadata(&partial).await.unwrap().len();
        assert!(size > 0 && size < 256 * 1024);
        assert!(worker.stopped(job).await.unwrap());
        // The provider actually honours ranges, so appending yields exact bytes.
        let ranged = worker
            .client
            .asset_response("/slow-video.mp4", size)
            .await
            .unwrap();
        assert_eq!(ranged.status(), reqwest::StatusCode::PARTIAL_CONTENT);
        drop(ranged);
        worker.resume(job).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state = worker
                    .db(move |c| {
                        c.query_row("SELECT state FROM download_jobs WHERE id=?", [job], |r| {
                            r.get::<_, String>(0)
                        })
                        .map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap();
                if state == "completed" {
                    break;
                }
                assert!(
                    !["failed", "partial_failure"].contains(&state.as_str()),
                    "unexpected {state}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("resumed transfer completes");
        let file: String = pool.get().unwrap().query_row("SELECT f.relative_path FROM local_files f JOIN assets a ON a.id=f.asset_id WHERE a.kind='video'",[],|r|r.get(0)).unwrap();
        let expected: Vec<u8> = (0..256 * 1024).map(|n| (n % 251) as u8).collect();
        assert_eq!(tokio::fs::read(root.join(&file)).await.unwrap(), expected);
        let modified = tokio::fs::metadata(root.join(&file))
            .await
            .unwrap()
            .modified()
            .unwrap();
        worker.enqueue(Some(course_id), None).await.unwrap();
        assert_eq!(
            tokio::fs::metadata(root.join(file))
                .await
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        provider.stop(true).await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[actix_web::test]
    async fn background_worker_recovers_and_downloads_specialization() {
        let pool = crate::test_support::pool();
        let (base, provider) = crate::test_support::provider().await;
        let client = Arc::new(CourseEraClient::for_test(base));
        let (spec, courses) = coursera::fetch_specialization(&client, "systems")
            .await
            .unwrap();
        let id =
            coursera::import_specialization(&mut pool.get().unwrap(), &spec, &courses).unwrap();
        let (spec2, courses2) = coursera::fetch_specialization(&client, "distributed-systems")
            .await
            .unwrap();
        let id2 =
            coursera::import_specialization(&mut pool.get().unwrap(), &spec2, &courses2).unwrap();
        let root = std::env::temp_dir().join(format!(
            "courseviewer-download-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        pool.get()
            .unwrap()
            .execute(
                "UPDATE application_settings SET max_concurrent_downloads=2,preferred_video_height=720",
                [],
            )
            .unwrap();
        let worker = Downloads::with_dir(pool.clone(), client, root.clone());
        let job = worker.enqueue(None, Some(id)).await.unwrap();
        assert_eq!(worker.enqueue(None, Some(id)).await.unwrap(), job);
        // A second specialization sharing a common course ("rust") can be queued concurrently.
        let job2 = worker.enqueue(None, Some(id2)).await.unwrap();
        // Simulate a server restart while the first job was running. Its saved quality
        // remains 720 even if the user changes preferences after enqueueing it.
        pool.get().unwrap().execute_batch("UPDATE download_jobs SET state='running' WHERE id=1; UPDATE application_settings SET preferred_video_height=1080;").unwrap();
        worker.start().await.unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let (state1, state2) = worker
                    .db(move |c| {
                        let s1: String = c
                            .query_row("SELECT state FROM download_jobs WHERE id=?", [job], |r| {
                                r.get(0)
                            })
                            .map_err(|e| e.to_string())?;
                        let s2: String = c
                            .query_row("SELECT state FROM download_jobs WHERE id=?", [job2], |r| {
                                r.get(0)
                            })
                            .map_err(|e| e.to_string())?;
                        Ok((s1, s2))
                    })
                    .await
                    .unwrap();
                if state1 == "completed" && state2 == "completed" {
                    break;
                }
                assert!(
                    !["failed", "partial_failure"].contains(&state1.as_str())
                        && !["failed", "partial_failure"].contains(&state2.as_str()),
                    "job failed: {state1}, {state2}"
                );
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .expect("background jobs complete without an open UI");
        let files=worker.db(|c| {
            assert_eq!(c.query_row("SELECT count(*) FROM courses WHERE outline_state='ready'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
            assert_eq!(c.query_row("SELECT count(*) FROM assets WHERE kind='video' AND height=360",[],|r|r.get::<_,i64>(0)).unwrap(),3);
            assert_eq!(c.query_row("SELECT count(*) FROM download_tasks WHERE state='completed'",[],|r|r.get::<_,i64>(0)).unwrap(),9);
            let mut stmt=c.prepare("SELECT storage_root,relative_path,size_bytes FROM local_files WHERE state='ready'").unwrap();
            Ok(stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap())
        }).await.unwrap();
        assert_eq!(files.len(), 9);
        for (directory, relative, size) in files {
            assert_eq!(
                tokio::fs::metadata(PathBuf::from(directory).join(relative))
                    .await
                    .unwrap()
                    .len(),
                size as u64
            );
        }
        let before: i64 = pool
            .get()
            .unwrap()
            .query_row("SELECT count(*) FROM download_jobs", [], |r| r.get(0))
            .unwrap();
        assert!(worker.enqueue(None, Some(id)).await.unwrap() > 0);
        assert_eq!(
            pool.get()
                .unwrap()
                .query_row("SELECT count(*) FROM download_jobs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            before
        );
        provider.stop(true).await;
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[actix_web::test]
    async fn pause_resume_and_retry_transitions() {
        let pool = crate::test_support::pool();
        let (base, provider) = crate::test_support::provider().await;
        let client = Arc::new(CourseEraClient::for_test(base));
        let (spec, courses) = coursera::fetch_specialization(&client, "systems")
            .await
            .unwrap();
        let spec_id =
            coursera::import_specialization(&mut pool.get().unwrap(), &spec, &courses).unwrap();
        let worker = Downloads::with_dir(pool.clone(), client, std::env::temp_dir());
        let job = worker.enqueue(None, Some(spec_id)).await.unwrap();
        let state = |id: i64| {
            let pool = pool.clone();
            move || -> String {
                pool.get()
                    .unwrap()
                    .query_row("SELECT state FROM download_jobs WHERE id=?", [id], |r| {
                        r.get(0)
                    })
                    .unwrap()
            }
        };

        assert!(
            worker.resume(job).await.is_err(),
            "queued job cannot resume"
        );
        assert!(worker.retry(job).await.is_err(), "queued job cannot retry");
        worker.pause(job).await.unwrap();
        assert_eq!(state(job)(), "paused");
        assert!(worker.stopped(job).await.unwrap());
        assert!(worker.pause(job).await.is_err(), "already paused");
        // Paused jobs are not picked up by the worker.
        assert!(worker.claim().await.unwrap().is_none());
        // Repeated clicks leave paused jobs in place.
        assert_eq!(worker.enqueue(None, Some(spec_id)).await.unwrap(), job);

        worker.resume(job).await.unwrap();
        assert_eq!(state(job)(), "queued");
        assert!(!worker.stopped(job).await.unwrap());

        pool.get()
            .unwrap()
            .execute(
                "UPDATE download_jobs SET state='failed',last_error='boom',finished_at=1 WHERE id=?",
                [job],
            )
            .unwrap();
        worker.retry(job).await.unwrap();
        let (s, err, finished): (String, Option<String>, Option<i64>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state,last_error,finished_at FROM download_jobs WHERE id=?",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(s, "queued");
        assert!(err.is_none() && finished.is_none());
        worker.cancel(job).await.unwrap();
        assert!(
            worker.pause(job).await.is_err(),
            "cancelled job cannot pause"
        );
        provider.stop(true).await;
    }

    #[actix_web::test]
    async fn quality_selection_and_paths_are_constrained() {
        let video = json!({"sources":{"byResolution":{"360p":{"mp4VideoUrl":"360"},"1080p":{"mp4VideoUrl":"1080"}}},"subtitlesVtt":{"en":"english","fr":"french"}});
        let mut settings = Settings {
            height: 720,
            format: "mp4".into(),
            fallback: "best_at_or_below".into(),
            languages: vec!["en".into()],
            subtitles: true,
            supplements: true,
            concurrency: 2,
        };
        let selected = lecture_assets(&video, &settings).unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].height, Some(360));
        assert_eq!(selected[1].language.as_deref(), Some("en"));
        settings.fallback = "exact".into();
        assert!(lecture_assets(&video, &settings).is_err());
        settings.height = 1080;
        assert_eq!(
            lecture_assets(&video, &settings).unwrap()[0].height,
            Some(1080)
        );
        assert!(!safe_component("../../bad/path").contains('/'));
    }
}
