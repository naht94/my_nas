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

use adapters::outbound::repository::sqlite::SqliteAuditRepository;
use adapters::outbound::repository::sqlite::SqliteAuthRepository;
use adapters::outbound::repository::sqlite::SqliteCrewRepository;
use adapters::outbound::repository::sqlite::SqliteFilesRepository;
use adapters::outbound::storage::storage::DiskStorage;
use application::service::NasService;
use infrastructure::server::build_server;

use crate::{
    adapters::inbound::web_dav::webdav::NasWebDavAdapter,
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

    let db_url = resolve_database_url(
        &env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file or environment"),
    );

    let storage_path = env::var("STORAGE_PATH").expect("STORAGE_PATH might be find by user");

    let pool = SqlitePoolOptions::new()
        .connect(&db_url)
        .await
        .expect("Failed to connect to SQLite");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    // Storage 구현체 생성
    let base_path = PathBuf::from(&storage_path);
    std::fs::create_dir_all(&base_path).expect("Failed to create base storage directory");
    let storage = Arc::new(DiskStorage::new(base_path.to_str().unwrap()));

    // Repository 구현체 생성
    let file_repo = Arc::new(SqliteFilesRepository::new(pool.clone()));
    let crew_repo = Arc::new(SqliteCrewRepository::new(pool.clone()));
    let auth_repo = Arc::new(SqliteAuthRepository::new(pool.clone()));
    let audit_repo = Arc::new(SqliteAuditRepository::new(pool.clone()));

    // 서비스 조립
    let service = Arc::new(NasService::new(
        storage.clone(),
        file_repo.clone(),
        crew_repo.clone(),
        auth_repo.clone(),
        audit_repo.clone(),
    ));

    service
        .ensure_global_root()
        .await
        .expect("Failed to ensure global root crew");

    if let Err(e) = service.cleanup_expired_sessions().await {
        tracing::warn!("만료 세션 정리 실패: {:?}", e);
    }

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

/// `sqlite://db/nas.db` 같은 상대 경로를 프로젝트 루트 기준으로 해석합니다.
/// `sqlite:///absolute/path` 형태는 그대로 둡니다.
fn resolve_database_url(url: &str) -> String {
    let Some(path) = url.strip_prefix("sqlite://") else {
        return url.to_string();
    };

    if path.starts_with('/') {
        return url.to_string();
    }

    let db_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    // sqlx: `sqlite:///abs/path` (슬래시 3개 + 절대경로)
    format!("sqlite://{}", db_path.display())
}
