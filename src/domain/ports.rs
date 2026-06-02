use std::path::PathBuf;

use crate::domain::errors::{NasError, RepositoryResult, StorageResult};
use crate::domain::models::{FolderMetadata, Greeting, ListItem, ObjectMetadata};
use async_trait::async_trait;
use axum::body::BodyDataStream;

// StoragePort: 파일을 실제로 저장하는 "창고"의 인터페이스
// 나중에 HDD가 아니라 AWS S3나 메모리로 바뀌어도 이 인터페이스만 지키면 됩니다.
#[async_trait]
pub trait StoragePort: Send + Sync {
    async fn save_file_stream(
        &self,
        id: &str,
        stream: BodyDataStream,
    ) -> StorageResult<(u64, String)>;
    async fn get_file(&self, key: &str) -> StorageResult<tokio::fs::File>; // 스트리밍을 위해 File 반환
    async fn delete_file(&self, key: &str) -> StorageResult<()>;
    fn get_base_path(&self) -> String;
    fn get_capacity(&self) -> (u64, u64);
    fn get_physical_path(&self, id: &str) -> PathBuf;
    async fn get_file_for_write(&self, id: &str) -> StorageResult<tokio::fs::File>;
}

// RepositoryPort: 메타데이터를 관리하는 "장부"의 인터페이스
// 나중에 SQLite가 아니라 PostgreSQL이나 Redis로 바뀌어도 상관없습니다.
#[async_trait]
pub trait FilesRepositoryPort: Send + Sync {
    // --- 파일(File) 관련 ---
    async fn save_metadata(&self, meta: ObjectMetadata) -> RepositoryResult<()>;
    async fn exists_folder_id_by_id(&self, id: &str) -> Result<bool, NasError>;
    async fn exists_file_id_by_id(&self, id: &str) -> Result<bool, NasError>;
    async fn find_file_by_id(&self, id: &str) -> RepositoryResult<Option<ObjectMetadata>>;
    async fn find_folder_by_id(&self, id: &str) -> RepositoryResult<Option<FolderMetadata>>;
    async fn delete_file(&self, id: &str) -> RepositoryResult<()>;

    // 수정: 물리적 경로(prefix) 대신 가상 폴더 ID를 기준으로 파일 목록 조회
    async fn list_files_by_folder(
        &self,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>>;

    // --- 폴더(Folder) 관련 (새로 추가) ---
    //  가상 폴더 생성 및 수정
    async fn save_folder(&self, folder: FolderMetadata) -> RepositoryResult<()>;

    //  특정 부모 폴더 아래의 하위 폴더 목록 조회
    async fn list_folders_by_parent(
        &self,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Vec<FolderMetadata>>;

    //  가상 폴더 삭제
    async fn delete_folder(&self, folder_id: &str) -> RepositoryResult<()>;

    async fn get_deleted_files(&self) -> RepositoryResult<Vec<ObjectMetadata>>;
    async fn permanent_delete_file(&self, id: &str) -> RepositoryResult<()>;
    async fn permanent_delete_folders(&self) -> RepositoryResult<()>;
    // --- 통합 조회 ---
    async fn list_items(&self, folder_id: Option<&str>) -> RepositoryResult<Vec<ListItem>>;

    // 폴더 내 중복 이름 확인
    async fn exists_folder_by_name_and_parent(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<bool, NasError>;

    async fn find_file_by_name_and_folder_id(
        &self,
        name: &str,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Option<ObjectMetadata>>;

    async fn exists_file_by_name_and_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<bool, NasError>;

    async fn find_folder_by_name_and_parent_id(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Option<FolderMetadata>>;
}
pub struct GreetingService;

#[async_trait]
pub trait UsersRepositoryPort: Send + Sync {
    async fn count_crew_members(&self, crew_id: &str) -> RepositoryResult<i64>;
    async fn create_user(&self, username: &str, password_hash: &str) -> RepositoryResult<i64>;
    async fn add_crew_member(
        &self,
        user_id: i64,
        crew_id: &str,
        role: u8,
        status: u8,
    ) -> RepositoryResult<()>;
}

impl GreetingService {
    pub fn say_hello() -> Greeting {
        Greeting {
            message: "hello".into(),
        }
    }
}
