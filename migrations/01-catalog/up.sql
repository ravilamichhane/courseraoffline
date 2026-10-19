CREATE TABLE courses (
 id INTEGER PRIMARY KEY,
 remote_id TEXT NOT NULL UNIQUE,
 slug TEXT NOT NULL UNIQUE,
 name TEXT NOT NULL,
 description TEXT,
 image_url TEXT,
 estimated_workload TEXT,
 remote_status TEXT,
 metadata_state TEXT NOT NULL DEFAULT 'summary' CHECK(metadata_state IN ('summary','ready','failed')),
 outline_state TEXT NOT NULL DEFAULT 'not_fetched' CHECK(outline_state IN ('not_fetched','ready','failed')),
 metadata_synced_at INTEGER,
 outline_synced_at INTEGER,
 last_sync_error TEXT,
 archived_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);


CREATE TABLE specializations (
 id INTEGER PRIMARY KEY,
 remote_id TEXT NOT NULL UNIQUE,
 slug TEXT NOT NULL UNIQUE,
 name TEXT NOT NULL,
 description TEXT,
 tagline TEXT,
 image_url TEXT,
 synced_at INTEGER,
 last_sync_error TEXT,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);


CREATE TABLE specialization_courses (
 specialization_id INTEGER NOT NULL REFERENCES specializations(id) ON DELETE CASCADE,
 course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE RESTRICT,
 position INTEGER NOT NULL CHECK(position >= 0),
 PRIMARY KEY(specialization_id, course_id)
);


CREATE INDEX specialization_courses_order ON specialization_courses(specialization_id, position);
CREATE INDEX specialization_courses_course ON specialization_courses(course_id);
