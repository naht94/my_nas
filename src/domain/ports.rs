use std::path::PathBuf;

use crate::domain::errors::{NasError, RepositoryResult, StorageResult};
use crate::domain::models::{
    AppPasswordInfo, AuditLogEntry, Crew, CrewMemberView, CrewMembership, FolderMetadata, Greeting,
    ListItem, ObjectMetadata, User,
};
use async_trait::async_trait;
use axum::body::BodyDataStream;

#[async_trait]
pub trait StoragePort: Send + Sync {
    async fn save_file_stream(
        &self,
        id: &str,
        stream: BodyDataStream,
    ) -> StorageResult<(u64, String)>;
    async fn get_file(&self, key: &str) -> StorageResult<tokio::fs::File>;
    async fn delete_file(&self, key: &str) -> StorageResult<()>;
    fn get_capacity(&self) -> (u64, u64);
    fn get_physical_path(&self, id: &str) -> PathBuf;
    async fn get_file_for_write(&self, id: &str) -> StorageResult<tokio::fs::File>;
}

#[async_trait]
pub trait FilesRepositoryPort: Send + Sync {
    async fn save_metadata(&self, meta: ObjectMetadata) -> RepositoryResult<()>;
    async fn exists_folder_id_by_id(&self, id: &str) -> Result<bool, NasError>;
    async fn exists_file_id_by_id(&self, id: &str) -> Result<bool, NasError>;
    /// `active_only = true`이면 `is_deleted = 0`인 항목만 조회한다.
    async fn find_file(&self, id: &str, active_only: bool) -> RepositoryResult<Option<ObjectMetadata>>;
    async fn find_folder(&self, id: &str, active_only: bool) -> RepositoryResult<Option<FolderMetadata>>;
    async fn find_file_by_id(&self, id: &str) -> RepositoryResult<Option<ObjectMetadata>> {
        self.find_file(id, true).await
    }
    async fn find_folder_by_id(&self, id: &str) -> RepositoryResult<Option<FolderMetadata>> {
        self.find_folder(id, true).await
    }
    /// 폴더가 속한 Crew id. `active_only`로 삭제된 폴더 포함 여부를 제어한다.
    async fn crew_id_for_folder(
        &self,
        folder_id: &str,
        active_only: bool,
    ) -> RepositoryResult<Option<String>>;
    /// 휴지통 항목(파일/폴더)이 속한 Crew 스코프.
    async fn trash_item_crew_id(&self, id: &str, is_dir: bool) -> RepositoryResult<Option<String>>;
    async fn delete_file(&self, id: &str) -> RepositoryResult<()>;
    async fn list_files_by_folder(
        &self,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>>;
    async fn save_folder(&self, folder: FolderMetadata) -> RepositoryResult<()>;
    async fn list_folders_by_parent(
        &self,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Vec<FolderMetadata>>;
    async fn delete_folder(&self, folder_id: &str) -> RepositoryResult<()>;
    /// 특정 Crew 스코프의 소프트 삭제된 파일 목록. `crew_id = None`이면 Crew 미소속(개인/전역 최상위) 파일.
    async fn get_deleted_files(
        &self,
        crew_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>>;
    async fn permanent_delete_file(&self, id: &str) -> RepositoryResult<()>;
    /// 특정 Crew 스코프의 빈 소프트 삭제 폴더를 영구 삭제.
    async fn permanent_delete_folders(&self, crew_id: Option<&str>) -> RepositoryResult<()>;
    async fn list_items(&self, folder_id: Option<&str>) -> RepositoryResult<Vec<ListItem>>;
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
    async fn update_folder_location(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> RepositoryResult<()>;
    async fn update_file_location(
        &self,
        id: &str,
        name: &str,
        folder_id: Option<&str>,
    ) -> RepositoryResult<()>;
    async fn restore_file(&self, id: &str) -> RepositoryResult<()>;
    async fn restore_folder_tree(&self, folder_id: &str) -> RepositoryResult<()>;
    async fn list_trash_items(
        &self,
        crew_id: Option<&str>,
    ) -> RepositoryResult<Vec<ListItem>>;
    async fn list_file_ids_in_folder_tree(&self, folder_id: &str) -> RepositoryResult<Vec<String>>;
    async fn permanent_delete_folder_tree(&self, folder_id: &str) -> RepositoryResult<()>;
    async fn search_files_by_name(
        &self,
        query: &str,
        limit: i32,
    ) -> RepositoryResult<Vec<(ObjectMetadata, Option<String>)>>;
    async fn folder_display_path(&self, folder_id: &str) -> RepositoryResult<String>;
    async fn list_subtitle_siblings_for_video(
        &self,
        video_id: &str,
    ) -> RepositoryResult<Vec<ObjectMetadata>>;
    async fn list_file_ids_by_crew_ids(&self, crew_ids: &[String]) -> RepositoryResult<Vec<String>>;
    async fn delete_files_and_folders_by_crew_ids(
        &self,
        crew_ids: &[String],
    ) -> RepositoryResult<()>;
}

pub struct GreetingService;

#[async_trait]
pub trait CrewRepositoryPort: Send + Sync {
    async fn count_crew_members(&self, crew_id: &str) -> RepositoryResult<i64>;
    async fn create_user(&self, username: &str, password_hash: &str) -> RepositoryResult<i64>;
    async fn find_user_by_username(&self, username: &str) -> RepositoryResult<Option<User>>;
    async fn find_user_by_id(&self, user_id: i64) -> RepositoryResult<Option<User>>;
    async fn update_user_password(
        &self,
        user_id: i64,
        password_hash: &str,
    ) -> RepositoryResult<()>;
    async fn list_descendant_crew_ids(&self, crew_id: &str) -> RepositoryResult<Vec<String>>;
    /// 글로벌 루트를 제외한 모든 Crew.
    async fn list_all_non_global_crews(&self) -> RepositoryResult<Vec<Crew>>;
    /// subtree 내 Crew 행을 depth 내림차순(깊은 것부터)으로 삭제한다.
    async fn delete_crews_by_ids(&self, crew_ids: &[String]) -> RepositoryResult<()>;
    async fn delete_crew_by_id(&self, crew_id: &str) -> RepositoryResult<()>;
    async fn list_owned_crews(&self, user_id: i64) -> RepositoryResult<Vec<Crew>>;
    async fn add_crew_member(
        &self,
        user_id: i64,
        crew_id: &str,
        role: u8,
        status: u8,
    ) -> RepositoryResult<()>;
    async fn update_crew_member(
        &self,
        user_id: i64,
        crew_id: &str,
        role: u8,
        status: u8,
    ) -> RepositoryResult<()>;
    async fn find_membership(
        &self,
        user_id: i64,
        crew_id: &str,
    ) -> RepositoryResult<Option<CrewMembership>>;
    async fn find_crew_by_id(&self, crew_id: &str) -> RepositoryResult<Option<Crew>>;
    async fn insert_crew(&self, crew: &Crew) -> RepositoryResult<()>;
    async fn update_crew_root_folder(&self, crew_id: &str, root_folder_id: &str)
        -> RepositoryResult<()>;
    async fn update_crew_settings(
        &self,
        crew_id: &str,
        max_sub_crew_depth: i32,
        visibility: i32,
    ) -> RepositoryResult<()>;
    async fn list_public_crews(&self) -> RepositoryResult<Vec<Crew>>;
    async fn list_crews_for_user(&self, user_id: i64) -> RepositoryResult<Vec<Crew>>;
    /// 사용자가 Owner/Manager(활성)로 속한 Crew 목록을 (crew, 본인 role)로 반환. 글로벌 루트 제외.
    async fn list_manageable_crews(&self, user_id: i64) -> RepositoryResult<Vec<(Crew, u8)>>;
    /// Crew의 전체 멤버(아이디 포함)를 반환.
    async fn list_crew_members(&self, crew_id: &str) -> RepositoryResult<Vec<CrewMemberView>>;
    /// 특정 Crew의 모든 하위(자손) Crew의 max_sub_crew_depth를 delta만큼 가감(0 미만은 0으로).
    async fn shift_descendant_max_depths(&self, crew_id: &str, delta: i32) -> RepositoryResult<()>;
    async fn ensure_global_root_crew(&self) -> RepositoryResult<()>;
}

/// 웹 세션과 WebDAV 앱 비밀번호의 영속화를 담당한다.
/// 저장되는 것은 항상 토큰의 해시이며, 평문은 보관하지 않는다.
#[async_trait]
pub trait AuthRepositoryPort: Send + Sync {
    async fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        label: Option<&str>,
        created_at: &str,
        expires_at: &str,
    ) -> RepositoryResult<()>;
    /// 토큰 해시로 세션을 조회한다. 반환: (user_id, expires_at)
    async fn find_session(&self, token_hash: &str) -> RepositoryResult<Option<(i64, String)>>;
    async fn delete_session(&self, token_hash: &str) -> RepositoryResult<()>;
    async fn delete_expired_sessions(&self, now: &str) -> RepositoryResult<()>;
    /// (token_hash, label, created_at, expires_at)
    async fn list_session_records(
        &self,
        user_id: i64,
    ) -> RepositoryResult<Vec<(String, Option<String>, String, String)>>;
    async fn delete_other_sessions(
        &self,
        user_id: i64,
        keep_token_hash: &str,
    ) -> RepositoryResult<u64>;

    async fn create_app_password(
        &self,
        id: &str,
        user_id: i64,
        token_hash: &str,
        label: Option<&str>,
        created_at: &str,
    ) -> RepositoryResult<()>;
    /// 토큰 해시로 앱 비밀번호의 소유자(user_id)를 조회한다.
    async fn find_app_password_user(&self, token_hash: &str) -> RepositoryResult<Option<i64>>;
    async fn touch_app_password(&self, token_hash: &str, now: &str) -> RepositoryResult<()>;
    async fn list_app_passwords(&self, user_id: i64) -> RepositoryResult<Vec<AppPasswordInfo>>;
    async fn delete_app_password(&self, id: &str, user_id: i64) -> RepositoryResult<()>;
}

#[async_trait]
pub trait AuditRepositoryPort: Send + Sync {
    async fn insert(
        &self,
        user_id: Option<i64>,
        username: Option<&str>,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        detail: Option<&str>,
        ip_address: Option<&str>,
    ) -> RepositoryResult<()>;

    async fn list_recent(&self, limit: i32) -> RepositoryResult<Vec<AuditLogEntry>>;
}

impl GreetingService {
    pub fn say_hello() -> Greeting {
        Greeting {
            message: "hello".into(),
        }
    }
}
