mod adapters;
mod application;
mod domain;
mod infrastructure;

use dotenvy::dotenv;
use sqlx::sqlite::SqlitePoolOptions;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use adapters::outbound::repository::sqlite::SqliteFilesRepository;
use adapters::outbound::repository::sqlite::SqliteUsersRepository;
use adapters::outbound::storage::storage::DiskStorage;
use application::service::NasService;
use infrastructure::server::build_server;

use crate::{
    adapters::inbound::webDav::webdav::NasWebDavAdapter,
    application::webdav_vfs_service::WebDavVfsService,
    domain::ports::{FilesRepositoryPort, StoragePort},
};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    // .env 인식
    dotenv().ok();

    let db_url =
        env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file or environment");

    let storage_path = env::var("STORAGE_PATH").expect("STORAGE_PATH might be find by user");

    let pool = SqlitePoolOptions::new()
        .connect(&db_url)
        .await
        .expect("Failed to connect to SQLite");

    // Storage 구현체 생성
    let base_path = PathBuf::from(&storage_path);
    std::fs::create_dir_all(&base_path).expect("Failed to create base storage directory");
    let storage = Arc::new(DiskStorage::new(base_path.to_str().unwrap()));

    // Repository 구현체 생성
    let file_repo = Arc::new(SqliteFilesRepository::new(pool.clone()));
    let user_repo = Arc::new(SqliteUsersRepository::new(pool.clone()));

    // 서비스 조립
    let service = Arc::new(NasService::new(
        storage.clone(),
        file_repo.clone(),
        user_repo.clone(),
    ));

    let repo_port: Arc<dyn FilesRepositoryPort> = file_repo.clone();
    let storage_port: Arc<dyn StoragePort> = storage.clone();

    let vfs_service = Arc::new(WebDavVfsService::new(service.clone(), repo_port.clone()));

    let webdav_adapter = NasWebDavAdapter {
        vfs_service,
        storage_port,
    };

    let app = build_server(service, webdav_adapter);
    tracing::info!("Server running on http://0.0.0.0:3000");

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
