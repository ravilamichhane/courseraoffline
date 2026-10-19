CREATE TABLE application_settings (
 id INTEGER PRIMARY KEY CHECK(id = 1),
 preferred_video_height INTEGER NOT NULL DEFAULT 1080 CHECK(preferred_video_height IN (240,360,540,720,1080)),
 preferred_video_format TEXT NOT NULL DEFAULT 'mp4' CHECK(preferred_video_format IN ('mp4','webm')),
 quality_fallback_policy TEXT NOT NULL DEFAULT 'best_at_or_below' CHECK(quality_fallback_policy IN ('best_at_or_below','exact')),
 subtitle_languages_json TEXT NOT NULL DEFAULT '["en"]' CHECK(json_valid(subtitle_languages_json) AND json_type(subtitle_languages_json) = 'array'),
 download_subtitles INTEGER NOT NULL DEFAULT 1 CHECK(download_subtitles IN (0,1)),
 download_supplements INTEGER NOT NULL DEFAULT 1 CHECK(download_supplements IN (0,1)),
 max_concurrent_downloads INTEGER NOT NULL DEFAULT 4 CHECK(max_concurrent_downloads BETWEEN 1 AND 32),
 updated_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);
INSERT INTO application_settings(id) VALUES (1);


CREATE TRIGGER application_settings_languages_insert BEFORE INSERT ON application_settings
WHEN EXISTS (SELECT 1 FROM json_each(NEW.subtitle_languages_json) WHERE type != 'text' OR length(trim(value)) = 0)
BEGIN SELECT RAISE(ABORT, 'subtitle languages must be nonempty strings'); END;
CREATE TRIGGER application_settings_languages_update BEFORE UPDATE OF subtitle_languages_json ON application_settings
WHEN EXISTS (SELECT 1 FROM json_each(NEW.subtitle_languages_json) WHERE type != 'text' OR length(trim(value)) = 0)
BEGIN SELECT RAISE(ABORT, 'subtitle languages must be nonempty strings'); END;
CREATE TRIGGER application_settings_prevent_delete BEFORE DELETE ON application_settings
BEGIN SELECT RAISE(ABORT, 'application settings singleton cannot be deleted'); END;
