CREATE TABLE assets (
 id INTEGER PRIMARY KEY,
 item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
 remote_asset_id TEXT,
 kind TEXT NOT NULL CHECK(kind IN ('video','audio','subtitle','transcript','reading','attachment','image')),
 variant_key TEXT NOT NULL,
 format TEXT NOT NULL,
 language_code TEXT,
 height INTEGER CHECK(height > 0),
 bitrate INTEGER CHECK(bitrate > 0),
 mime_type TEXT,
 source_url TEXT,
 source_resolved_at INTEGER,
 source_expires_at INTEGER,
 expected_bytes INTEGER CHECK(expected_bytes >= 0),
 inline_content TEXT,
 removed_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 UNIQUE(item_id, variant_key)
);



CREATE UNIQUE INDEX assets_selected_video ON assets(item_id) WHERE kind = 'video' AND removed_at IS NULL;
CREATE TABLE local_files (
 id INTEGER PRIMARY KEY,
 asset_id INTEGER NOT NULL UNIQUE REFERENCES assets(id) ON DELETE RESTRICT,
 storage_root TEXT NOT NULL,
 relative_path TEXT NOT NULL,
 temporary_relative_path TEXT,
 state TEXT NOT NULL DEFAULT 'partial' CHECK(state IN ('partial','ready','missing','corrupt')),
 bytes_present INTEGER NOT NULL DEFAULT 0 CHECK(bytes_present >= 0),
 size_bytes INTEGER CHECK(size_bytes >= 0),
 etag TEXT,
 last_modified TEXT,
 checksum_sha256 TEXT,
 completed_at INTEGER,
 verified_at INTEGER,
 created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
 UNIQUE(storage_root, relative_path),
 CHECK(state != 'ready' OR (size_bytes IS NOT NULL AND completed_at IS NOT NULL AND bytes_present = size_bytes))
);
