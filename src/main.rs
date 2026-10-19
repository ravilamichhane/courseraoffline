use actix_web::{App, HttpServer, middleware, web::Data};
mod appstate;
use courseviewer::courseeraclient;
mod courses;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let data_dir = courseviewer::data_dir();
    let downloads_dir = courseviewer::downloads_dir();
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&downloads_dir)?;

    let db_path = courseviewer::database_path();
    let pool = courseviewer::database::open_pool(&db_path).map_err(std::io::Error::other)?;

    let api_key = courseviewer::api_key().ok_or_else(|| {
        std::io::Error::other(
            "API_KEY environment variable is required (e.g. API_KEY=your-secret-key).",
        )
    })?;
    let auth_config = Data::new(courseviewer::auth::ApiKeyConfig::required(api_key));

    let is_dev = courseviewer::is_dev_mode();
    let frontend_config = Data::new(courseviewer::frontend::FrontendConfig::new(
        is_dev,
        downloads_dir.clone(),
    ));

    let host = courseviewer::env_var("HOST").unwrap_or_else(|| "0.0.0.0".to_string());
    let port: u16 = courseviewer::env_var("PORT")
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    println!("Starting server...");
    println!("Data directory: {}", data_dir.display());
    println!("Database: {} (migrations applied)", db_path.display());
    println!(
        "Mode: {}",
        if is_dev {
            "development (proxying frontend to localhost:3000)"
        } else {
            "production (serving embedded/dist frontend)"
        }
    );
    println!("Listening on http://{host}:{port}");

    let client = std::sync::Arc::new(courseeraclient::CourseEraClient::new());
    let state = Data::new(appstate::AppState::new(client.clone()));
    let downloads =
        courseviewer::downloads::Downloads::with_dir(pool.clone(), client, downloads_dir);
    downloads.start().await.map_err(std::io::Error::other)?;

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .app_data(auth_config.clone())
            .app_data(frontend_config.clone())
            .wrap(middleware::Logger::default())
            .wrap(middleware::NormalizePath::default())
            .configure(courseviewer::auth::configure)
            .configure(courses::configure)
            .service(courseviewer::admin::routes_with_downloads(
                pool.clone(),
                downloads.clone(),
            ))
            .configure(courseviewer::frontend::configure)
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}
