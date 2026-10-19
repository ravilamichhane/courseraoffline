//! Persistent background download queue. One job plans at a time; file transfers
//! within that job run concurrently up to its saved configuration limit.
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

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
}

pub struct Downloads {
    pool: DbPool,
    pub client: Arc<CourseEraClient>,
    pub downloads_dir: PathBuf,
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
        })
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
                        let result = tokio::select! {
                            result=worker.process(id,selection)=>result,
                            _=worker.wait_for_cancellation(id)=>Ok(()),
                        };
                        if let Err(error) = result {
                            let _=worker.db(move |c| {
                                c.execute("UPDATE download_jobs SET state='failed',last_error=?1,finished_at=unixepoch()*1000,updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?2 AND state!='cancelled'",params![error,id]).map_err(|e|e.to_string())?;
                                c.execute("UPDATE download_tasks SET state='failed',last_error_message=?1,updated_at=unixepoch()*1000 WHERE job_id=?2 AND state IN ('queued','running')",params![error,id]).map_err(|e|e.to_string())?;
                                Ok(())
                            }).await;
                        }
                    }
                    Ok(None) => tokio::time::sleep(Duration::from_millis(750)).await,
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
            for id in &course_ids {
                let overlap:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM download_jobs j WHERE j.state IN ('planning','queued','running','paused') AND (j.course_id=?1 OR EXISTS(SELECT 1 FROM json_each(j.selection_json,'$.course_ids') x WHERE x.value=?1)))",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
                if overlap { return Err("A download for one of these courses is already active.".into()); }
            }
            let settings=tx.query_row("SELECT preferred_video_height,preferred_video_format,quality_fallback_policy,subtitle_languages_json,download_subtitles,download_supplements,max_concurrent_downloads FROM application_settings WHERE id=1",[],|r| {
                let languages:String=r.get(3)?;
                Ok(Settings { height:r.get(0)?,format:r.get(1)?,fallback:r.get(2)?,languages:serde_json::from_str(&languages).unwrap_or_default(),subtitles:r.get(4)?,supplements:r.get(5)?,concurrency:r.get::<_,i64>(6)? as usize })
            }).map_err(|e|e.to_string())?;
            let selection=serde_json::to_string(&Selection { course_ids,settings }).map_err(|e|e.to_string())?;
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

    async fn cancelled(&self, id: i64) -> Result<bool, String> {
        self.db(move |c| {
            c.query_row(
                "SELECT state='cancelled' FROM download_jobs WHERE id=?",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
        })
        .await
    }

    async fn wait_for_cancellation(&self, id: i64) {
        loop {
            if self.cancelled(id).await.unwrap_or(false) {
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
        let mut saw_locked_item = false;
        for course_id in selection.course_ids {
            if self.cancelled(job_id).await? {
                return Ok(());
            }
            let (remote_id, slug, spec_remote_ids) = self
                .db(move |c| {
                    let (remote_id, slug) = c
                        .query_row(
                            "SELECT remote_id,slug FROM courses WHERE id=?",
                            [course_id],
                            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                        )
                        .map_err(|e| e.to_string())?;
                    let mut stmt = c
                        .prepare(
                            "SELECT s.remote_id FROM specializations s
                             JOIN specialization_courses sc ON sc.specialization_id = s.id
                             WHERE sc.course_id = ?",
                        )
                        .map_err(|e| e.to_string())?;
                    let specs = stmt
                        .query_map([course_id], |r| r.get::<_, String>(0))
                        .map_err(|e| e.to_string())?
                        .collect::<rusqlite::Result<Vec<_>>>()
                        .map_err(|e| e.to_string())?;
                    Ok((remote_id, slug, specs))
                })
                .await?;
            for spec_remote_id in spec_remote_ids {
                self.client
                    .ensure_specialization_enrolled(&spec_remote_id)
                    .await;
            }
            let outline = match coursera::fetch_outline_with_enrollment(
                &self.client,
                &remote_id,
                &slug,
            )
            .await
            {
                Ok(value) => value,
                Err(error) => {
                    let message = error.clone();
                    self.db(move |c| { c.execute("UPDATE courses SET outline_state='failed',last_sync_error=?1,updated_at=unixepoch()*1000 WHERE id=?2",params![message,course_id]).map_err(|e|e.to_string())?; Ok(()) }).await?;
                    return Err(error);
                }
            };
            let items = self
                .db(move |c| coursera::store_outline(c, course_id, &outline))
                .await?;
            for item in items {
                if self.cancelled(job_id).await? {
                    return Ok(());
                }
                if item.kind != "lecture"
                    && !(selection.settings.supplements
                        && ["supplement", "reading"].contains(&item.kind.as_str()))
                {
                    continue;
                }
                if item.locked {
                    saw_locked_item = true;
                    continue;
                }
                let assets = self.resolve(&remote_id, &item, &selection.settings).await?;
                let course_slug = slug.clone();
                let planned = self
                    .db(move |c| plan_assets(c, job_id, item.id, &course_slug, assets))
                    .await?;
                tasks.extend(planned);
            }
        }
        if self.cancelled(job_id).await? {
            return Ok(());
        }
        if tasks.is_empty() {
            if saw_locked_item {
                return Err(
                    "Course materials are locked. Check your Coursera enrollment and cookie.txt."
                        .into(),
                );
            }
            return Err("No downloadable videos or resources were found.".into());
        }
        self.db(move |c| { c.execute("UPDATE download_jobs SET state='running',updated_at=unixepoch()*1000,revision=revision+1 WHERE id=? AND state='planning'",[job_id]).map_err(|e|e.to_string())?; Ok(()) }).await?;
        let mut active = tokio::task::JoinSet::new();
        for task in tasks {
            if self.cancelled(job_id).await? {
                break;
            }
            while active.len() >= selection.settings.concurrency.clamp(1, 32) {
                active
                    .join_next()
                    .await
                    .ok_or("Download task missing")?
                    .map_err(|_| "Download task interrupted")?;
            }
            let worker = self.clone();
            active.spawn(async move {
                let task_id=task.id;
                if let Err(error)=worker.transfer(job_id,task).await {
                    let _=worker.db(move |c| { c.execute("UPDATE download_tasks SET state='failed',last_error_message=?1,updated_at=unixepoch()*1000 WHERE id=?2 AND state!='cancelled'",params![error,task_id]).map_err(|e|e.to_string())?; Ok(()) }).await;
                }
            });
        }
        while let Some(result) = active.join_next().await {
            result.map_err(|_| "Download task interrupted")?;
        }
        self.db(move |c| {
            let (total,done,failed):(i64,i64,i64)=c.query_row("SELECT count(*),coalesce(sum(state IN ('completed','skipped')),0),coalesce(sum(state='failed'),0) FROM download_tasks WHERE job_id=?",[job_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
            let state=if total>0 && done==total { "completed" } else if done>0 { "partial_failure" } else { "failed" };
            let error=if failed>0 { Some(format!("{failed} file(s) failed. Start a new download to retry.")) } else { None };
            c.execute("UPDATE download_jobs SET state=?1,last_error=?2,finished_at=unixepoch()*1000,updated_at=unixepoch()*1000,revision=revision+1 WHERE id=?3 AND state!='cancelled'",params![state,error,job_id]).map_err(|e|e.to_string())?;
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
        if self.cancelled(job_id).await? {
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
        let mut response = if let Some(source) = &task.source {
            Some(self.client.asset_response(source).await?)
        } else {
            None
        };
        if self.cancelled(job_id).await? {
            return Ok(());
        }
        let total = task.inline.as_ref().map(|s| s.len() as i64).or_else(|| {
            response
                .as_ref()
                .and_then(|r| r.content_length())
                .map(|n| n as i64)
        });
        let storage_root = root.to_string_lossy().into_owned();
        let relative = task.relative.clone();
        let temp_relative = format!("{}.part", task.relative);
        self.db(move |c| {
            let tx=c.transaction().map_err(|e|e.to_string())?;
            tx.execute("INSERT INTO local_files(asset_id,storage_root,relative_path,temporary_relative_path,state,bytes_present,size_bytes) VALUES (?1,?2,?3,?4,'partial',0,?5) ON CONFLICT(asset_id) DO UPDATE SET storage_root=excluded.storage_root,relative_path=excluded.relative_path,temporary_relative_path=excluded.temporary_relative_path,state='partial',bytes_present=0,size_bytes=excluded.size_bytes,completed_at=NULL,updated_at=unixepoch()*1000",params![asset_id,storage_root,relative,temp_relative,total]).map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state='running',downloaded_bytes=0,total_bytes=?1,attempt_count=attempt_count+1,started_at=unixepoch()*1000,updated_at=unixepoch()*1000 WHERE id=?2 AND state!='cancelled'",params![total,task_id]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())?; Ok(())
        }).await?;
        let mut file = tokio::fs::File::create(&temporary)
            .await
            .map_err(|e| format!("Could not create course file: {e}"))?;
        let mut downloaded = 0i64;
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
                .map_err(|_| "File transfer interrupted. Start a new download to retry.")?
            {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Could not write course file: {e}"))?;
                downloaded += chunk.len() as i64;
                if total.is_some_and(|n| downloaded > n) {
                    return Err("File size exceeded its reported length.".into());
                }
                if last_update.elapsed() >= Duration::from_millis(400) {
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
        if self.cancelled(job_id).await? {
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
            let tx=c.transaction().map_err(|e|e.to_string())?;
            tx.execute("UPDATE download_tasks SET state=CASE WHEN ?1 THEN 'completed' ELSE state END,downloaded_bytes=?2,total_bytes=?3,finished_at=CASE WHEN ?1 THEN unixepoch()*1000 ELSE finished_at END,updated_at=unixepoch()*1000 WHERE id=?4 AND state!='cancelled'",params![complete,bytes,total,task]).map_err(|e|e.to_string())?;
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

fn plan_assets(
    conn: &mut Connection,
    job: i64,
    item: i64,
    course_slug: &str,
    assets: Vec<Asset>,
) -> Result<Vec<Task>, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let cancelled: bool = tx
        .query_row(
            "SELECT state='cancelled' FROM download_jobs WHERE id=?",
            [job],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if cancelled {
        return Ok(Vec::new());
    }
    let mut tasks = Vec::new();
    for asset in assets {
        if asset.kind == "video" {
            tx.execute("UPDATE assets SET removed_at=unixepoch()*1000 WHERE item_id=?1 AND kind='video' AND variant_key!=?2",params![item,asset.variant]).map_err(|e|e.to_string())?;
        }
        let id:i64=tx.query_row("INSERT INTO assets(item_id,kind,variant_key,format,height,language_code,source_url,inline_content,source_resolved_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,unixepoch()*1000) ON CONFLICT(item_id,variant_key) DO UPDATE SET source_url=excluded.source_url,inline_content=excluded.inline_content,source_resolved_at=unixepoch()*1000,removed_at=NULL,updated_at=unixepoch()*1000 RETURNING id",params![item,asset.kind,asset.variant,asset.format,asset.height,asset.language,asset.source,asset.inline],|r|r.get(0)).map_err(|e|e.to_string())?;
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
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!(worker.enqueue(None, Some(id)).await.is_err());
        // Simulate a server restart while the job was running. Its saved quality
        // remains 720 even if the user changes preferences after enqueueing it.
        pool.get().unwrap().execute_batch("UPDATE download_jobs SET state='running'; UPDATE application_settings SET preferred_video_height=1080;").unwrap();
        worker.start().await.unwrap();
        tokio::time::timeout(Duration::from_secs(15), async {
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
                    "job failed: {state}"
                );
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .expect("background job completes without an open UI");
        let files=worker.db(|c| {
            assert_eq!(c.query_row("SELECT count(*) FROM courses WHERE outline_state='ready'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
            assert_eq!(c.query_row("SELECT count(*) FROM assets WHERE kind='video' AND height=360",[],|r|r.get::<_,i64>(0)).unwrap(),2);
            assert_eq!(c.query_row("SELECT count(*) FROM download_tasks WHERE state='completed'",[],|r|r.get::<_,i64>(0)).unwrap(),6);
            let mut stmt=c.prepare("SELECT storage_root,relative_path,size_bytes FROM local_files WHERE state='ready'").unwrap();
            Ok(stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap())
        }).await.unwrap();
        assert_eq!(files.len(), 6);
        for (directory, relative, size) in files {
            assert_eq!(
                tokio::fs::metadata(PathBuf::from(directory).join(relative))
                    .await
                    .unwrap()
                    .len(),
                size as u64
            );
        }
        let queued = worker.enqueue(None, Some(id)).await.unwrap();
        worker.cancel(queued).await.unwrap();
        assert!(worker.cancelled(queued).await.unwrap());
        provider.stop(true).await;
        tokio::fs::remove_dir_all(root).await.unwrap();
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
