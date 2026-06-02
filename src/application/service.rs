use crate::domain::errors::NasError;
use crate::domain::models::{FolderMetadata, ListItem, ObjectMetadata, Role, Status};
use crate::domain::ports::{
    FilesRepositoryPort, GreetingService, StoragePort, UsersRepositoryPort,
};
use async_zip::tokio::write::ZipFileWriter;
use async_zip::{Compression, ZipEntryBuilder};
use axum::body::BodyDataStream;
use futures::{AsyncWriteExt, Stream};
use mime_guess;
use serde_json::Value;
use serde_json::json;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, duplex};
use tokio_util::io::ReaderStream;
// 서비스는 구체적인 구현체(DiskStorage, SqliteRepo)를 모릅니다.
// 오직 Trait(Port)만 알고 있습니다. (의존성 역전)
pub struct NasService {
    pub storage: Arc<dyn StoragePort>,
    pub repository: Arc<dyn FilesRepositoryPort>,
    pub crew_repository: Arc<dyn UsersRepositoryPort>,
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
        crew_repository: Arc<dyn UsersRepositoryPort>,
    ) -> Self {
        Self {
            storage,
            repository,
            crew_repository,
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
    ) -> Result<String, NasError> {
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
    ) -> Result<String, NasError> {
        // 1.  디폴트 이름
        let mut target_name = name.unwrap_or("새 폴더").trim();
        if target_name.is_empty() {
            target_name = "새 폴더";
        }

        // 2. 중복 확인 및 이름 결정
        let mut final_name = target_name.to_string();
        let mut count = 1;

        // 레포지토리에서 부모 폴더 에서 중복 이름 확인
        while self
            .repository
            .exists_folder_by_name_and_parent(&final_name, parent_id)
            .await?
        {
            final_name = format!("{} ({})", target_name, count);
            count += 1;
        }
        let mut new_id = uuid::Uuid::new_v4().to_string();
        while self.repository.exists_folder_id_by_id(&new_id).await? {
            new_id = uuid::Uuid::new_v4().to_string();
        }
        let now = chrono::Utc::now().to_rfc3339();

        let folder = FolderMetadata {
            id: new_id.clone(),
            parent_id: parent_id.map(|id| id.to_string()),
            name: final_name,
            created_at: now.clone(),
            updated_at: now,
        };

        self.repository.save_folder(folder).await?;
        Ok(new_id)
    }
    pub async fn download_file(
        &self,
        id: &str,
    ) -> Result<(tokio::fs::File, String, String, u64), NasError> {
        let meta = self
            .repository
            .find_file_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;

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
    ) -> Result<impl Stream<Item = Result<bytes::Bytes, std::io::Error>>, NasError> {
        let (writer, reader) = duplex(2 * 1024 * 1024);
        let service = self.clone();

        tokio::spawn(async move {
            let mut zip = ZipFileWriter::with_tokio(writer);

            for id in ids {
                let Ok(Some(meta)) = service.repository.find_file_by_id(&id).await else {
                    continue;
                };

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
    pub async fn delete_file(&self, id: &str) -> Result<(), NasError> {
        // 1. 장부(DB)에서 파일이 존재하는지 먼저 확인
        // (찾지 못하면 RepoError가 아닌 비즈니스 에러인 NasError::NotFound 반환)
        self.repository
            .find_file_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        // 2. 창고(Storage)에서 실제 파일 삭제
        // self.storage.delete_file(id).await?;

        // 3. 장부(DB)에서 메타데이터 삭제 (또는 is_deleted = true 로 소프트 삭제 처리)
        self.repository.delete_file(id).await?;

        Ok(())
    }

    pub async fn delete_folder(&self, id: &str) -> Result<(), NasError> {
        // 1. 장부 (DB)에서 폴더가 존재하는지 먼저 확인
        self.repository
            .find_folder_by_id(id)
            .await?
            .ok_or(NasError::DataNotFound)?;
        // 2. DB에서 폴더 삭제
        self.repository.delete_folder(id).await?;
        Ok(())
    }

    pub async fn empty_trash(&self) -> Result<(), NasError> {
        // 1. 휴지통에 있는 파일 목록 가져오기
        let files_to_delete = self.repository.get_deleted_files().await?;

        // 2. 루프를 돌며 물리 파일 삭제
        for file in files_to_delete {
            match self.storage.delete_file(&file.id).await {
                Ok(_) => {
                    let _ = self.repository.permanent_delete_file(&file.id).await;
                }
                Err(e) => {
                    tracing::error!(
                        "물리 파일 삭제 실패 (name: {}, size: {}:): {:?}",
                        &file.name,
                        &file.size,
                        e
                    );
                }
            }
        }

        // 3. DB 장부 정리 (파일 & 폴더)
        self.repository.permanent_delete_folders().await?;

        Ok(())
    }
    ///  파일 목록 조회 로직 (DTO 변환 포함)
    pub async fn list_files(&self, parent_id: Option<String>) -> Result<Vec<ListItem>, NasError> {
        let items = self.repository.list_items(parent_id.as_deref()).await?;
        Ok(items)
    }

    pub async fn register_new_user(&self, username: &str, password: &str) -> Result<i64, NasError> {
        let new_user_id = self
            .crew_repository
            .create_user(username, password)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        self.join_or_create_crew_membership(new_user_id, "global-root-uuid")
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

        let assigned_status =
            if assigned_role == (Role::Owner as u8) || crew_id == "global-root-uuid" {
                Status::Active as u8
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
