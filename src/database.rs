use std::{path::Path, time::Duration};

use include_dir::{Dir, include_dir};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use rusqlite_migration::Migrations;

static MIGRATION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

pub type DbResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn migrations() -> rusqlite_migration::Result<Migrations<'static>> {
    Migrations::from_directory(&MIGRATION_DIR)
}

pub fn configure_connection(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(Duration::from_secs(30))
}

pub fn migrate(path: impl AsRef<Path>) -> DbResult<()> {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut conn = Connection::open(path)?;
    configure_connection(&conn)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrations()?.to_latest(&mut conn)?;
    let has_legacy_download_dir: bool = conn
        .prepare("PRAGMA table_info(application_settings)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|col| col == "download_directory");
    if has_legacy_download_dir {
        conn.execute_batch("ALTER TABLE application_settings DROP COLUMN download_directory;")?;
    }
    Ok(())
}

pub fn open_pool(path: impl AsRef<Path>) -> DbResult<Pool<SqliteConnectionManager>> {
    migrate(path.as_ref())?;
    let manager = SqliteConnectionManager::file(path).with_init(|conn| configure_connection(conn));
    Ok(Pool::builder()
        .max_size(16)
        .connection_timeout(Duration::from_secs(30))
        .build(manager)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid_and_repeatable() -> DbResult<()> {
        let migrations = migrations()?;
        migrations.validate()?;
        let mut conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        migrations.to_latest(&mut conn)?;
        migrations.to_latest(&mut conn)?;
        assert_eq!(
            conn.query_row(
                "SELECT preferred_video_height FROM application_settings",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            1080
        );
        assert!(
            !conn
                .prepare("PRAGMA foreign_key_check")?
                .query([])?
                .next()?
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn relationships_and_download_settings_are_enforced() -> DbResult<()> {
        let mut conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        migrations()?.to_latest(&mut conn)?;
        conn.execute_batch("INSERT INTO courses(id,remote_id,slug,name) VALUES (1,'c1','c1','Course'),(2,'c2','c2','Other');
            INSERT INTO modules(id,course_id,remote_id,name,position) VALUES (1,1,'m1','Module',0);
            INSERT INTO lessons(id,course_id,module_id,remote_id,name,position) VALUES (1,1,1,'l1','Lesson',0);
            INSERT INTO items(id,course_id,lesson_id,remote_id,name,content_type,position) VALUES (1,1,1,'i1','Lecture','lecture',0);
            INSERT INTO assets(id,item_id,kind,variant_key,format,height) VALUES (1,1,'video','video:mp4:720p','mp4',720);
            INSERT INTO assets(item_id,kind,variant_key,format,language_code) VALUES (1,'subtitle','subtitle:vtt:en','vtt','en');")?;
        for invalid in [
            "INSERT INTO items(course_id,lesson_id,remote_id,name,content_type,position) VALUES (2,1,'bad','Bad','lecture',1)",
            "INSERT INTO assets(item_id,kind,variant_key,format) VALUES (1,'video','video:mp4:360p','mp4')",
            "UPDATE application_settings SET max_concurrent_downloads = 0",
            "UPDATE application_settings SET subtitle_languages_json = '[42]'",
            "DELETE FROM application_settings",
            "INSERT INTO download_jobs(selection_json) VALUES ('{}')",
        ] {
            assert!(
                conn.execute_batch(invalid).is_err(),
                "accepted invalid change: {invalid}"
            );
        }
        conn.execute_batch("UPDATE assets SET removed_at = 1 WHERE id = 1;
            INSERT INTO assets(item_id,kind,variant_key,format,height) VALUES (1,'video','video:mp4:360p','mp4',360);
            UPDATE application_settings SET subtitle_languages_json = '[]';")?;
        Ok(())
    }

    #[test]
    fn existing_catalog_survives_pending_migrations() -> DbResult<()> {
        let mut conn = Connection::open_in_memory()?;
        configure_connection(&conn)?;
        migrations()?.to_version(&mut conn, 3)?;
        conn.execute_batch("INSERT INTO courses(id, remote_id, slug, name) VALUES (1, 'existing', 'existing', 'Existing course');")?;
        migrations()?.to_latest(&mut conn)?;
        assert_eq!(
            conn.query_row("SELECT name FROM courses WHERE id = 1", [], |r| {
                r.get::<_, String>(0)
            })?,
            "Existing course"
        );
        Ok(())
    }
}
