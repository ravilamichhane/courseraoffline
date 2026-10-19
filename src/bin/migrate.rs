use courseviewer::database::{self, DbResult};

fn main() -> DbResult<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "up".into());
    let path = args
        .next()
        .unwrap_or_else(|| courseviewer::database_path().to_string_lossy().into_owned());
    if args.next().is_some() {
        return Err("Usage: migrate [up|status|check] [database-path]".into());
    }
    match command.as_str() {
        "up" => {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            database::migrate(&path)?;
            println!("Migrations applied to {path}");
        }
        "status" => {
            let conn = rusqlite::Connection::open_with_flags(
                &path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            println!(
                "{path}: {:?}",
                database::migrations()?.current_version(&conn)?
            );
        }
        "check" => {
            database::migrations()?.validate()?;
            println!("Migrations are valid");
        }
        _ => return Err("Usage: migrate [up|status|check] [database-path]".into()),
    }
    Ok(())
}
