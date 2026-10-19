CREATE TABLE playback_progress (
 item_id INTEGER PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
 position_ms INTEGER NOT NULL DEFAULT 0 CHECK(position_ms >= 0),
 is_completed INTEGER NOT NULL DEFAULT 0 CHECK(is_completed IN (0,1)),
 last_played_at INTEGER,
 completed_at INTEGER,
 playback_rate REAL NOT NULL DEFAULT 1.0 CHECK(playback_rate > 0),
 subtitle_language TEXT,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
CREATE INDEX playback_continue_watching ON playback_progress(is_completed, last_played_at DESC);
