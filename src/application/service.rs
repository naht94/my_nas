use crate::domain::errors::NasError;
use crate::domain::models::{
    FolderMetadata, ListItem, ObjectMetadata, Role, Status, SubtitleTrackInfo,
    GLOBAL_ROOT_CREW_ID,
};
use crate::application::crew::FolderAccess;
use crate::domain::ports::{
    AuditRepositoryPort, AuthRepositoryPort, CrewRepositoryPort, FilesRepositoryPort, GreetingService,
    StoragePort,
};
use crate::infrastructure::crypto;
use crate::infrastructure::rate_limit::LoginRateLimiter;
use crate::infrastructure::subtitle;
use std::sync::Arc;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use axum::body::BodyDataStream;
use futures::{AsyncWriteExt, Stream};
use mime_guess;
use serde_json::Value;
use serde_json::json;
use std::fmt;
use std::path::Path;
use tokio::io::{AsyncReadExt, duplex};
use tokio_util::io::ReaderStream;
// 서비스는 구체적인 구현체(DiskStorage, SqliteRepo)를 모릅니다.
// 오직 Trait(Port)만 알고 있습니다. (의존성 역전)
pub struct NasService {
    pub storage: Arc<dyn StoragePort>,
    pub repository: Arc<dyn FilesRepositoryPort>,
    pub crew_repository: Arc<dyn CrewRepositoryPort>,
    pub auth_repository: Arc<dyn AuthRepositoryPort>,
    pub audit_repository: Arc<dyn AuditRepositoryPort>,
    pub(crate) login_rate_limiter: Arc<LoginRateLimiter>,
    /// 토큰 해시 → (user_id, 캐시 시각). WebDAV/세션 검증의 DB 조회를 줄인다.
    pub(crate) token_cache: Mutex<HashMap<String, (i64, Instant)>>,
}

impl fmt::Debug for NasService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NasService").finish()
    }
}
pub struct FirstSt;

impl FirstSt {
    pub fn hello() -> Value {
        let greeting = GreetingService::say_hello();
        json!({ "message": greeting.message})
    }
}

impl NasService {
    pub fn new(
        storage: Arc<dyn StoragePort>,
        repository: Arc<dyn FilesRepositoryPort>,
        crew_repository: Arc<dyn CrewRepositoryPort>,
        auth_repository: Arc<dyn AuthRepositoryPort>,
        audit_repository: Arc<dyn AuditRepositoryPort>,
    ) -> Self {
        Self {
            storage,
            repository,
            crew_repository,
            auth_repository,
            audit_repository,
            login_rate_limiter: Arc::new(LoginRateLimiter::new()),
            token_cache: Mutex::new(HashMap::new()),
        }
    }
    pub fn get_storage_usage(&self) -> (u64, u64) {
        self.storage.get_capacity()
    }

    pub async fn upload_file(
        &self,
        client_filename: &str,
        parent_id: Option<String>,
        expected_size: u64,
        stream: BodyDataStream,
        user_id: Option<i64>,
    ) -> Result<String, NasError> {
        self.resolve_folder_access(user_id, parent_id.as_deref(), true)
            .await?;
        let path = Path::new(client_filename);
        let stem = path.file_stem().unwrap_or_default().to_str().unwrap_or("");
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{}", s))
            .unwrap_or_default();
        let mut final_name = client_filename.to_string();
        let mut count = 1;

        while self
            .repository
            .exists_file_by_name_and_folder(&final_name, parent_id.as_deref())
            .await?
        {
            final_name = format!("{} ({}){}", stem, count, ext);
            count += 1;
        }

        let mut new_id = uuid::Uuid::new_v4().to_string();
        while self.repository.exists_file_id_by_id(&new_id).await? {
            new_id = uuid::Uuid::new_v4().to_string();
        }
        // 1. 물리적 저장은 Storage가 알아서 샤딩해서 저장
        let (total_size, checksum_hex) = self.storage.save_file_stream(&new_id, stream).await?;

        if expected_size != total_size {
            tracing::warn!(
                "File size mismatch for {}: expected {}. but got {}",
                client_filename,
                expected_size,
                total_size
            )
        }
        // 2. DB에는 가상 위치(folder_id)와 함께 기록
        let now = chrono::Utc::now().to_rfc3339();
        let meta = ObjectMetadata {
            id: new_id.clone(),
            folder_id: parent_id, // 가상 폴더 ID 연결
            name: final_name,
            size: total_size,
            file_type: Some(
                mime_guess::from_path(client_filename)
                    .first_or_octet_stream()
                    .to_string(),
            ),
            is_deleted: false,
            created_at: now.clone(),
            updated_at: now,
            checksum: Some(checksum_hex),
            version: 1,
        };

        self.repository.save_metadata(meta).await?;
        Ok(new_id)
    }
    // 가상 폴더 생성 로직
    pub async fn create_folder(
        &self,
        name: Option<&str>,
        parent_id: Option<&str>,
        user_id: Option<i64>,
        explicit_crew_id: Option<&str>,
        dedupe_on_conflict: bool,
    ) -> Result<String, NasError> {
        self.resolve_folder_access(user_id, parent_id, true)
            .await?;
        // 1.  디폴트 이름
        let mut target_name = name.unwrap_or("새 폴더").trim();
        if target_name.is_empty() {
            target_name = "새 폴더";
        }

        // 2. 중복 확인 및 이름 결정
        let mut final_name = target_name.to_string();
        if dedupe_on_conflict {
            let mut count = 1;
            while self
                .repository
                .exists_folder_by_name_and_parent(&final_name, parent_id)
                .await?
            {
                final_name = format!("{} ({})", target_name, count);
                count += 1;
            }
        } else if self
            .repository
            .exists_folder_by_name_and_parent(&final_name, parent_id)
            .await?
        {
            return Err(NasError::BadRequest(
                "같은 이름의 폴더가 이미 있습니다.".into(),
            ));
        }
        let mut new_id = uuid::Uuid::new_v4().to_string();
        while self.repository.exists_folder_id_by_id(&new_id).await? {
            new_id = uuid::Uuid::new_v4().to_string();
        }
        let now = chrono::Utc::now().to_rfc3339();

        let crew_id = if let Some(cid) = explicit_crew_id {
            Some(cid.to_string())
        } else if let Some(pid) = parent_id {
            self.repository
                .find_folder_by_id(pid)
                .await?
                .and_then(|f| f.crew_id)
        } else {
            None
        };

        let folder = FolderMetadata {
            id: new_id.clone(),
            parent_id: parent_id.map(|id| id.to_string()),
            crew_id,
            name: final_name,
            created_at: now.clone(),
            updated_at: now,
        };

        self.repository.save_folder(folder).await?;
        Ok(new_id)
    }

    pub async fn rename_folder(
        &self,
        folder_id: &str,
        new_name: &str,
        new_parent_id: Option<&str>,
        user_id: Option<i64>,
    ) -> Result<(), NasError> {
        let folder = self
            .repository
            .find_folder_by_id(folder_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        if folder.parent_id.is_none() {
            return Err(NasError::Forbidden(
                "Crew 루트 폴더는 이동하거나 이름을 바꿀 수 없습니다.".into(),
            ));
        }

        self.resolve_folder_access(user_id, folder.parent_id.as_deref(), true)
            .await?;
        self.resolve_folder_access(user_id, new_parent_id, true)
            .await?;

        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(NasError::BadRequest("폴더 이름이 비어 있습니다.".into()));
        }

        if folder.parent_id.as_deref() != new_parent_id || folder.name != new_name {
            self.ensure_unique_sibling_name(new_name, new_parent_id)
                .await?;
        }

        self.repository
            .update_folder_location(folder_id, new_name, new_parent_id)
            .await?;
        Ok(())
    }

    pub async fn rename_file(
        &self,
        file_id: &str,
        new_name: &str,
        new_folder_id: Option<&str>,
        user_id: Option<i64>,
    ) -> Result<(), NasError> {
        let file = self
            .repository
            .find_file_by_id(file_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        self.resolve_folder_access(user_id, file.folder_id.as_deref(), true)
            .await?;
        self.resolve_folder_access(user_id, new_folder_id, true)
            .await?;

        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(NasError::BadRequest("파일 이름이 비어 있습니다.".into()));
        }

        if file.folder_id.as_deref() != new_folder_id || file.name != new_name {
            self.ensure_unique_sibling_name(new_name, new_folder_id)
                .await?;
        }

        self.repository
            .update_file_location(file_id, new_name, new_folder_id)
            .await?;
        Ok(())
    }

    pub async fn download_file(
        &self,
        id: &str,
        user_id: Option<i64>,
    ) -> Result<(tokio::fs::File, String, String, u64), NasError> {
        let meta = self
            .repository
            .find_file_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        // 파일이 속한 폴더(Crew)의 읽기 권한을 확인한다.
        match self
            .resolve_folder_access(user_id, meta.folder_id.as_deref(), false)
            .await?
        {
            FolderAccess::GuestMetaOnly | FolderAccess::Denied => {
                return Err(NasError::DataNotFound);
            }
            _ => {}
        }

        // Storage 레이어 내부의 get_physical_path(id)를 통해 샤딩된 파일을 읽어옴
        let file = self.storage.get_file(id).await?;

        Ok((
            file,
            meta.file_type
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            meta.name,
            meta.size,
        ))
    }
    pub async fn download_files_as_zip(
        self: Arc<Self>,
        ids: Vec<String>,
        user_id: Option<i64>,
    ) -> Result<impl Stream<Item = Result<bytes::Bytes, std::io::Error>>, NasError> {
        let (writer, reader) = duplex(2 * 1024 * 1024);
        let service = self.clone();

        tokio::spawn(async move {
            let mut zip = ZipFileWriter::with_tokio(writer);

            for id in ids {
                let Ok(Some(meta)) = service.repository.find_file_by_id(&id).await else {
                    continue;
                };

                // 읽기 권한이 없는 파일은 zip에서 조용히 제외한다.
                match service
                    .resolve_folder_access(user_id, meta.folder_id.as_deref(), false)
                    .await
                {
                    Ok(FolderAccess::GuestMetaOnly) | Ok(FolderAccess::Denied) | Err(_) => continue,
                    Ok(_) => {}
                }

                let Ok(mut tokio_file) = service.storage.get_file(&id).await else {
                    continue;
                };
                let entry = ZipEntryBuilder::new(meta.name.clone().into(), Compression::Stored);

                let mut entry_writer = zip
                    .write_entry_stream(entry)
                    .await
                    .map_err(std::io::Error::other)?;

                let mut buffer = vec![0u8; 256 * 1024]; // 256kb 버퍼
                loop {
                    match tokio_file.read(&mut buffer).await {
                        Ok(0) => break, // 파일 끝
                        Ok(n) => {
                            if let Err(e) = entry_writer.write_all(&buffer[..n]).await {
                                tracing::error!("Write error: {:?}", e);
                                return Err(e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Read error: {:?}", e);
                            return Err(e);
                        }
                    }
                }

                // let mut compat_writer = entry_writer.compat_write();
                //
                // tokio::io::copy(&mut tokio_file, &mut compat_writer).await?;
                //
                // let entry_writer = compat_writer.into_inner();

                entry_writer.close().await.map_err(std::io::Error::other)?;
            }
            zip.close().await.map_err(std::io::Error::other)?;
            Ok::<(), std::io::Error>(())
        });
        Ok(ReaderStream::new(reader))
    }
    pub async fn delete_file(&self, id: &str, user_id: Option<i64>) -> Result<(), NasError> {
        // 1. 장부(DB)에서 파일이 존재하는지 먼저 확인
        // (찾지 못하면 RepoError가 아닌 비즈니스 에러인 NasError::NotFound 반환)
        let meta = self
            .repository
            .find_file_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        // 파일이 속한 폴더(Crew)의 쓰기 권한을 확인한다.
        self.resolve_folder_access(user_id, meta.folder_id.as_deref(), true)
            .await?;

        // 2. 창고(Storage)에서 실제 파일 삭제
        // self.storage.delete_file(id).await?;

        // 3. 장부(DB)에서 메타데이터 삭제 (또는 is_deleted = true 로 소프트 삭제 처리)
        self.repository.delete_file(id).await?;

        Ok(())
    }

    pub async fn delete_folder(&self, id: &str, user_id: Option<i64>) -> Result<(), NasError> {
        // 1. 장부 (DB)에서 폴더가 존재하는지 먼저 확인
        self.repository
            .find_folder_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;
        // 2. 해당 폴더(Crew)의 쓰기 권한 확인
        self.resolve_folder_access(user_id, Some(id), true).await?;
        // 3. DB에서 폴더 삭제
        self.repository.delete_folder(id).await?;
        Ok(())
    }

    pub async fn patch_file(
        &self,
        id: &str,
        new_name: Option<&str>,
        new_folder_id: Option<Option<&str>>,
        user_id: Option<i64>,
    ) -> Result<(), NasError> {
        let file = self
            .repository
            .find_file_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let name = new_name.unwrap_or(&file.name);
        let folder_id = match new_folder_id {
            Some(v) => v,
            None => file.folder_id.as_deref(),
        };

        self.rename_file(id, name, folder_id, user_id).await
    }

    pub async fn patch_folder(
        &self,
        id: &str,
        new_name: Option<&str>,
        new_parent_id: Option<Option<&str>>,
        user_id: Option<i64>,
    ) -> Result<(), NasError> {
        let folder = self
            .repository
            .find_folder_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let name = new_name.unwrap_or(&folder.name);
        let parent_id = match new_parent_id {
            Some(v) => v,
            None => folder.parent_id.as_deref(),
        };

        self.rename_folder(id, name, parent_id, user_id).await
    }

    ///  파일 목록 조회 로직 (DTO 변환 포함)
    pub async fn list_files(
        &self,
        parent_id: Option<String>,
        user_id: Option<i64>,
    ) -> Result<Vec<ListItem>, NasError> {
        match self
            .resolve_folder_access(user_id, parent_id.as_deref(), false)
            .await?
        {
            FolderAccess::GuestMetaOnly => Ok(vec![]),
            FolderAccess::Denied => Err(NasError::DataNotFound),
            _ => {
                let mut items = self.repository.list_items(parent_id.as_deref()).await?;
                for item in &mut items {
                    Self::enrich_preview_url(item);
                }
                Ok(items)
            }
        }
    }

    pub async fn search_files(
        &self,
        user_id: Option<i64>,
        query: &str,
    ) -> Result<Vec<ListItem>, NasError> {
        let query = query.trim();
        if query.len() < 2 {
            return Err(NasError::BadRequest(
                "검색어는 2자 이상이어야 합니다.".into(),
            ));
        }

        let candidates = self
            .repository
            .search_files_by_name(query, 100)
            .await?;

        let mut results = Vec::new();
        for (meta, _crew_id) in candidates {
            match self
                .resolve_folder_access(user_id, meta.folder_id.as_deref(), false)
                .await
            {
                Ok(FolderAccess::GuestMetaOnly) | Ok(FolderAccess::Denied) | Err(_) => continue,
                Ok(_) => {
                    let path = if let Some(ref fid) = meta.folder_id {
                        self.repository.folder_display_path(fid).await.ok()
                    } else {
                        Some("Home".to_string())
                    };
                    let mut item = ListItem::from_file_with_path(&meta, path);
                    Self::enrich_preview_url(&mut item);
                    results.push(item);
                }
            }
        }

        Ok(results)
    }

    fn is_previewable_name(name: &str) -> bool {
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        matches!(
            ext.as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "mp4" | "mkv" | "webm" | "mov"
                | "m4v" | "avi"
        )
    }

    fn enrich_preview_url(item: &mut ListItem) {
        if !item.is_dir && Self::is_previewable_name(&item.name) {
            item.preview_url = Some(format!("/api/files/{}/stream?inline=true", item.id));
        }
    }

    pub async fn list_subtitle_tracks(
        &self,
        video_id: &str,
        user_id: Option<i64>,
    ) -> Result<Vec<SubtitleTrackInfo>, NasError> {
        let video = self
            .repository
            .find_file_by_id(video_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        match self
            .resolve_folder_access(user_id, video.folder_id.as_deref(), false)
            .await?
        {
            FolderAccess::GuestMetaOnly | FolderAccess::Denied => {
                return Err(NasError::DataNotFound);
            }
            _ => {}
        }

        let siblings = self
            .repository
            .list_subtitle_siblings_for_video(video_id)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        Ok(siblings
            .into_iter()
            .map(|sub| SubtitleTrackInfo {
                id: sub.id.clone(),
                name: sub.name.clone(),
                label: subtitle::subtitle_label(&sub.name),
                vtt_url: format!("/api/files/{}/as-vtt", sub.id),
            })
            .collect())
    }

    pub async fn subtitle_as_vtt(
        &self,
        subtitle_id: &str,
        user_id: Option<i64>,
    ) -> Result<String, NasError> {
        const MAX_SUBTITLE_BYTES: u64 = 10 * 1024 * 1024;

        let meta = self
            .repository
            .find_file_by_id(subtitle_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let ext = subtitle::subtitle_extension(&meta.name)
            .ok_or(NasError::BadRequest("자막 파일이 아닙니다.".into()))?;

        match self
            .resolve_folder_access(user_id, meta.folder_id.as_deref(), false)
            .await?
        {
            FolderAccess::GuestMetaOnly | FolderAccess::Denied => {
                return Err(NasError::DataNotFound);
            }
            _ => {}
        }

        if meta.size > MAX_SUBTITLE_BYTES {
            return Err(NasError::BadRequest("자막 파일이 너무 큽니다.".into()));
        }

        let mut file = self.storage.get_file(subtitle_id).await?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        let raw = subtitle::decode_subtitle_bytes(&bytes)
            .map_err(|e| NasError::BadRequest(e))?;
        subtitle::to_vtt(&raw, ext).map_err(|e| NasError::BadRequest(e))
    }

    pub async fn register_new_user(&self, username: &str, password: &str) -> Result<i64, NasError> {
        let username = username.trim();
        if username.is_empty() {
            return Err(NasError::BadRequest("아이디를 입력해주세요.".into()));
        }
        if password.len() < 8 {
            return Err(NasError::BadRequest(
                "비밀번호는 8자 이상이어야 합니다.".into(),
            ));
        }

        let password_hash = crypto::hash_password(password)
            .map_err(|e| NasError::Internal(format!("비밀번호 해싱 실패: {e}")))?;

        let new_user_id = self
            .crew_repository
            .create_user(username, &password_hash)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        self.join_or_create_crew_membership(new_user_id, GLOBAL_ROOT_CREW_ID)
            .await?;

        Ok(new_user_id)
    }

    pub async fn join_or_create_crew_membership(
        &self,
        user_id: i64,
        crew_id: &str,
    ) -> Result<(), NasError> {
        let member_count = self
            .crew_repository
            .count_crew_members(crew_id)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        let assigned_role = if member_count == 0 {
            Role::Owner as u8
        } else {
            Role::Member as u8
        };

        let assigned_status = if assigned_role == (Role::Owner as u8) {
            Status::Active as u8
        } else if crew_id == GLOBAL_ROOT_CREW_ID {
            // 글로벌 크루 신규 가입자는 오너 승인 전까지 Pending
            Status::Pending as u8
        } else {
            Status::Pending as u8
        };

        self.crew_repository
            .add_crew_member(user_id, crew_id, assigned_role, assigned_status)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        Ok(())
    }
}
