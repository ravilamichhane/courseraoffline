CREATE TABLE modules (
 id INTEGER PRIMARY KEY,
 course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
 remote_id TEXT NOT NULL,
 slug TEXT,
 name TEXT NOT NULL,
 description TEXT,
 position INTEGER NOT NULL CHECK(position >= 0),
 time_commitment_raw INTEGER CHECK(time_commitment_raw >= 0),
 removed_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 UNIQUE(course_id, remote_id),
 UNIQUE(id, course_id)
);


CREATE TABLE lessons (
 id INTEGER PRIMARY KEY,
 course_id INTEGER NOT NULL,
 module_id INTEGER NOT NULL,
 remote_id TEXT NOT NULL,
 slug TEXT,
 name TEXT NOT NULL,
 position INTEGER NOT NULL CHECK(position >= 0),
 time_commitment_raw INTEGER CHECK(time_commitment_raw >= 0),
 removed_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 FOREIGN KEY(module_id, course_id) REFERENCES modules(id, course_id) ON DELETE CASCADE,
 UNIQUE(course_id, remote_id),
 UNIQUE(id, course_id)
);


CREATE TABLE items (
 id INTEGER PRIMARY KEY,
 course_id INTEGER NOT NULL,
 lesson_id INTEGER NOT NULL,
 remote_id TEXT NOT NULL,
 slug TEXT,
 name TEXT NOT NULL,
 content_type TEXT NOT NULL,
 position INTEGER NOT NULL CHECK(position >= 0),
 is_locked INTEGER NOT NULL DEFAULT 0 CHECK(is_locked IN (0,1)),
 duration_ms INTEGER CHECK(duration_ms >= 0),
 time_commitment_raw INTEGER CHECK(time_commitment_raw >= 0),
 content_summary_json TEXT CHECK(content_summary_json IS NULL OR json_valid(content_summary_json)),
 removed_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 FOREIGN KEY(lesson_id, course_id) REFERENCES lessons(id, course_id) ON DELETE CASCADE,
 UNIQUE(course_id, remote_id)
);


CREATE INDEX modules_order ON modules(course_id, position);
CREATE INDEX lessons_order ON lessons(module_id, course_id, position);
CREATE INDEX items_order ON items(lesson_id, course_id, position);
