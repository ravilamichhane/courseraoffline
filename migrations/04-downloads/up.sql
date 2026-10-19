CREATE TABLE download_jobs (
 id INTEGER PRIMARY KEY,
 course_id INTEGER REFERENCES courses(id) ON DELETE RESTRICT,
 specialization_id INTEGER REFERENCES specializations(id) ON DELETE RESTRICT,
 state TEXT NOT NULL DEFAULT 'planning' CHECK(state IN ('planning','queued','running','paused','completed','partial_failure','failed','cancelled')),
 selection_json TEXT NOT NULL CHECK(json_valid(selection_json) AND json_type(selection_json) = 'object'),
 request_key TEXT UNIQUE,
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
 last_error TEXT,
 started_at INTEGER,
 finished_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 CHECK((course_id IS NOT NULL) + (specialization_id IS NOT NULL) = 1)
);


CREATE TABLE download_tasks (
 id INTEGER PRIMARY KEY,
 job_id INTEGER NOT NULL REFERENCES download_jobs(id) ON DELETE CASCADE,
 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','paused','completed','failed','cancelled','skipped')),
 downloaded_bytes INTEGER NOT NULL DEFAULT 0 CHECK(downloaded_bytes >= 0),
 total_bytes INTEGER CHECK(total_bytes >= 0),
 attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),
 next_attempt_at INTEGER,
 last_error_code TEXT,
 last_error_message TEXT,
 started_at INTEGER,
 finished_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 UNIQUE(job_id, asset_id),
 CHECK(total_bytes IS NULL OR downloaded_bytes <= total_bytes)
);


CREATE INDEX download_jobs_queue ON download_jobs(state, created_at);
CREATE INDEX download_jobs_course ON download_jobs(course_id);
CREATE INDEX download_jobs_specialization ON download_jobs(specialization_id);
CREATE INDEX download_tasks_queue ON download_tasks(state, next_attempt_at);
CREATE INDEX download_tasks_job_state ON download_tasks(job_id, state);
CREATE INDEX download_tasks_asset ON download_tasks(asset_id);
CREATE UNIQUE INDEX download_tasks_running_asset ON download_tasks(asset_id) WHERE state = 'running';
