use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("not found")]
    NotFound,

    #[error("permission denied")]
    PermissionDenied,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("network stream error: {0}")]
    Network(String),
}

#[derive(Error, Debug)]
pub enum RepoError {
    #[error("not found")]
    NotFound,

    #[error("permission denied")]
    PermissionDenied,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Error, Debug)]
pub enum NasError {
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("repository error: {0}")]
    Repo(#[from] RepoError),

    #[error("Resource not found")]
    DataNotFound,

    #[error("Internal IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid request: {0}")]
    BadRequest(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Zip process error: {0}")]
    Zip(String), // 또는 #[from] zip::result::ZipError (라이브러리 임포트 필요)

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type StorageResult<T> = Result<T, StorageError>;
pub type RepositoryResult<T> = Result<T, RepoError>;
