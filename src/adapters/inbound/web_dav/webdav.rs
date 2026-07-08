use crate::application::service::NasService;
use crate::application::webdav_vfs_service::{VfsNode, WebDavVfsService};
use crate::domain::models::ObjectMetadata;
use crate::domain::ports::StoragePort;
use crate::infrastructure::webdav_auth::WebDavCredentials;
use chrono::{DateTime, Utc};
use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavMetaData, FsError, FsResult, FsStream, GuardedFileSystem,
    OpenOptions, ReadDirMeta,
};
use futures_util::future::BoxFuture;
use futures_util::stream::StreamExt; // 💡 비동기 스트림 처리를 위해 추가됨
use std::io::SeekFrom;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

// =================================================================
// 1. WebDAV 파일시스템 어댑터
// =================================================================
#[derive(Clone)]
pub struct NasWebDavAdapter {
    pub vfs_service: Arc<WebDavVfsService>,
    pub storage_port: Arc<dyn StoragePort>,
}

impl GuardedFileSystem<WebDavCredentials> for NasWebDavAdapter {
    fn get_quota<'until_done>(
        &'until_done self,
        _credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<(u64, Option<u64>)>> {
        Box::pin(async move {
            let (total, available) = self.vfs_service.nas_service.get_storage_usage();
            let used = total.saturating_sub(available);
            tracing::info!(
                "📊 [WebDAV Quota] Used: {} bytes, total: {} bytes",
                used,
                total
            );
            // dav-server는 (used, total)을 기대한다. available을 넘기면 클라이언트가 전체 용량을 잘못 표시한다.
            Ok((used, Some(total)))
        })
    }
    fn read_dir<'until_done>(
        &'until_done self,
        path: &'until_done DavPath,
        _meta: ReadDirMeta,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<FsStream<Box<dyn DavDirEntry>>>> {
        Box::pin(async move {
            let path_str = path.as_url_string();
            let user_id = credentials.user_id;

            if path_str == "/" || path_str.is_empty() {
                return Err(FsError::NotFound);
            }

            match self.vfs_service.list_directory(&path_str, user_id).await {
                Ok(nodes) => {
                    tracing::info!(
                        "📂 [WebDAV read_dir] path='{}' entries={}",
                        path_str,
                        nodes.len()
                    );
                    let mut entries: Vec<Result<Box<dyn DavDirEntry>, FsError>> = Vec::new();

                    for node in nodes {
                        entries.push(Ok(
                            Box::new(NasWebDavDirEntry { node }) as Box<dyn DavDirEntry>
                        ));
                    }

                    let stream = futures_util::stream::iter(entries).boxed();
                    Ok(stream)
                }
                Err(e) => {
                    tracing::error!("❌ [WebDAV read_dir] path='{}' error: {}", path_str, e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }
    fn metadata<'until_done>(
        &'until_done self,
        path: &'until_done DavPath,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<Box<dyn DavMetaData>>> {
        Box::pin(async move {
            let normal_path = path.as_url_string();
            let user_id = credentials.user_id;
            let raw_url = path.as_url_string();

            tracing::info!("--- [Metadata 요청 시작] ---");
            tracing::info!("🔗 클라이언트 전송 URL (Raw): '{}'", raw_url);
            tracing::info!("📂 라이브러리 해석 경로 (Path): '{}'", normal_path);

            if normal_path == "/" || normal_path.is_empty() {
                return Err(FsError::NotFound);
            }

            match self.vfs_service.resolve_path(&normal_path, user_id).await {
                Ok(Some(node)) => {
                    // 🔍 여기서 로그를 찍어봅니다.
                    match &node {
                        VfsNode::File(f) => {
                            tracing::info!(
                                "📄 [Metadata] 파일 발견: 이름={}, 크기={}, IS_FILE={}",
                                f.name,
                                f.size,
                                matches!(VfsNode::File(f.clone()), VfsNode::File(_))
                            );
                        }
                        VfsNode::Folder(f) => {
                            tracing::info!("📁 [Metadata] 폴더 발견: 이름={}", f.name);
                        }
                    }
                    let meta = if is_webdav_mount_root(&normal_path) {
                        let (total, available) =
                            self.vfs_service.nas_service.get_storage_usage();
                        NasWebDavMetaData::MountRoot {
                            node,
                            disk_used: total.saturating_sub(available),
                        }
                    } else {
                        NasWebDavMetaData::Node(node)
                    };
                    Ok(Box::new(meta) as Box<dyn DavMetaData>)
                }
                Ok(None) => {
                    tracing::warn!("❓ [Metadata] 경로를 찾을 수 없음: {}", normal_path);
                    Err(FsError::NotFound)
                }
                Err(e) => {
                    tracing::error!("❌ [Metadata] VFS 에러: {:?}", e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }

    fn open<'until_done>(
        &'until_done self,
        path: &'until_done DavPath,
        options: OpenOptions,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<Box<dyn DavFile>>> {
        Box::pin(async move {
            let path_str = path.as_url_string();
            let user_id = credentials.user_id;
            tracing::info!(
                "🚀 [WebDAV] Open 요청 발생! 경로: '{}', 쓰기옵션: {}",
                path_str,
                options.write
            );

            match self.vfs_service.resolve_path(&path_str, user_id).await {
                // ✅ [Case 1] 기존 파일이 존재하는 경우 (읽기 또는 덮어쓰기)
                Ok(Some(VfsNode::File(file_meta))) => {
                    // 🔥 핵심: 클라이언트가 데이터를 '쓰려고' 파일을 열었는지 확인!
                    let tokio_file = if options.write {
                        tracing::info!("🔄 [WebDAV] 기존 파일 덮어쓰기 모드로 열기: {}", path_str);
                        self.vfs_service
                            .nas_service
                            .storage
                            .get_file_for_write(&file_meta.id) // 쓰기 모드로 오픈!
                            .await
                            .map_err(|_| FsError::GeneralFailure)?
                    } else {
                        tracing::info!("📖 [WebDAV] 기존 파일 읽기 모드로 열기: {}", path_str);
                        self.storage_port
                            .get_file(&file_meta.id) // 기존처럼 읽기 전용으로 오픈
                            .await
                            .map_err(|_| FsError::NotFound)?
                    };

                    Ok(Box::new(NasWebDavFile {
                        file: tokio_file,
                        meta: file_meta,
                        nas_service: self.vfs_service.nas_service.clone(),
                    }) as Box<dyn DavFile>)
                }

                // ✅ [Case 2] 파일이 없는데 '생성' 옵션이 켜져 있는 경우 (새 업로드의 1단계)
                Ok(None) if options.create => {
                    tracing::info!("🆕 [WebDAV] 새 파일 생성 요청: {}", path_str);

                    let (new_id, parent_id, final_name) = self
                        .vfs_service
                        .prepare_file_for_put(&path_str, user_id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;

                    let tokio_file = self
                        .vfs_service
                        .nas_service
                        .storage
                        .get_file_for_write(&new_id)
                        .await
                        .map_err(|_| FsError::GeneralFailure)?;

                    let now = chrono::Utc::now().to_rfc3339();
                    let initial_meta = ObjectMetadata {
                        id: new_id.clone(),
                        folder_id: parent_id.clone(),
                        name: final_name.clone(),
                        size: 0,
                        file_type: Some(
                            mime_guess::from_path(&final_name)
                                .first_or_octet_stream()
                                .to_string(),
                        ),
                        is_deleted: false,
                        created_at: now.clone(),
                        updated_at: now,
                        checksum: None,
                        version: 1,
                    };

                    // Windows는 PUT 완료 전에도 PROPFIND로 목록을 갱신한다.
                    // DB에 먼저 등록해 두어야 새로고침 후에도 항목이 유지된다.
                    self.vfs_service
                        .nas_service
                        .repository
                        .save_metadata(initial_meta.clone())
                        .await
                        .map_err(|e| {
                            tracing::error!("❌ WebDAV 초기 메타데이터 저장 실패: {:?}", e);
                            FsError::GeneralFailure
                        })?;

                    Ok(Box::new(NasWebDavFile {
                        file: tokio_file,
                        meta: initial_meta,
                        nas_service: self.vfs_service.nas_service.clone(),
                    }) as Box<dyn DavFile>)
                }

                // ❌ [Case 3] 그 외 (폴더이거나, 없는데 생성 옵션도 없는 경우)
                _ => Err(FsError::NotFound),
            }
        })
    }

    fn create_dir<'until_done>(
        &'until_done self,
        path: &'until_done DavPath,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<()>> {
        Box::pin(async move {
            let path_str = path.as_url_string();
            let user_id = credentials.user_id;

            self.vfs_service
                .create_folder_by_path(&path_str, user_id)
                .await
                .map_err(|e| {
                    tracing::error!("❌ 폴더 생성 실패: {}", e);
                    FsError::GeneralFailure
                })?;

            tracing::info!("✅ 폴더 생성 완료: {}", path_str);
            Ok(())
        })
    }

    fn rename<'until_done>(
        &'until_done self,
        from: &'until_done DavPath,
        to: &'until_done DavPath,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<()>> {
        Box::pin(async move {
            let from_str = from.as_url_string();
            let to_str = to.as_url_string();
            let user_id = credentials.user_id;
            tracing::info!("✏️ [WebDAV] Rename/MOVE: '{}' -> '{}'", from_str, to_str);

            self.vfs_service
                .rename_path(&from_str, &to_str, user_id)
                .await
                .map_err(|e| {
                    tracing::error!("❌ Rename 실패: {}", e);
                    if e.contains("찾을 수 없") {
                        FsError::NotFound
                    } else {
                        FsError::GeneralFailure
                    }
                })
        })
    }

    // 🗑️ 1. 파일 삭제 (DELETE)
    fn remove_file<'until_done>(
        &'until_done self,
        path: &'until_done DavPath,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<()>> {
        Box::pin(async move {
            let path_str = path.as_url_string();
            let user_id = credentials.user_id;
            tracing::info!("🗑️ [WebDAV] 파일 삭제 요청: {}", path_str);

            match self.vfs_service.resolve_path(&path_str, user_id).await {
                Ok(Some(VfsNode::File(file_meta))) => {
                    self.vfs_service
                        .nas_service
                        .delete_file(&file_meta.id, Some(user_id))
                        .await
                        .map_err(|e| {
                            tracing::error!("❌ 파일 삭제 실패: {:?}", e);
                            FsError::GeneralFailure
                        })?;
                    Ok(())
                }
                // 대상이 폴더이거나 없는 경우 에러 처리
                Ok(Some(VfsNode::Folder(_))) => Err(FsError::Forbidden),
                _ => Err(FsError::NotFound),
            }
        })
    }

    // 🗑️ 2. 폴더 삭제 (DELETE)
    fn remove_dir<'until_done>(
        &'until_done self,
        path: &'until_done DavPath,
        credentials: &'until_done WebDavCredentials,
    ) -> BoxFuture<'until_done, FsResult<()>> {
        Box::pin(async move {
            let path_str = path.as_url_string();
            let user_id = credentials.user_id;
            tracing::info!("🗑️ [WebDAV] 폴더 삭제 요청: {}", path_str);

            match self.vfs_service.resolve_path(&path_str, user_id).await {
                Ok(Some(VfsNode::Folder(folder_meta))) => {
                    self.vfs_service
                        .nas_service
                        .delete_folder(&folder_meta.id, Some(user_id))
                        .await
                        .map_err(|e| {
                            tracing::error!("❌ 폴더 삭제 실패: {:?}", e);
                            FsError::GeneralFailure
                        })?;
                    Ok(())
                }
                // 대상이 파일이거나 없는 경우 에러 처리
                Ok(Some(VfsNode::File(_))) => Err(FsError::Forbidden),
                _ => Err(FsError::NotFound),
            }
        })
    }
}

// =================================================================
// 2. 가상 메타데이터 명세서
// =================================================================
#[derive(Debug, Clone)]
pub enum NasWebDavMetaData {
    Root,
    Node(VfsNode),
    /// Crew WebDAV 마운트 루트(`/crew-uuid/`). dav-server는 path != "/" 일 때
    /// `meta.len()`을 quota-used-bytes 로 쓰므로 디스크 사용량을 len 에 넣는다.
    MountRoot { node: VfsNode, disk_used: u64 },
}

impl NasWebDavMetaData {
    fn vfs_node(&self) -> Option<&VfsNode> {
        match self {
            Self::Node(n) | Self::MountRoot { node: n, .. } => Some(n),
            Self::Root => None,
        }
    }
}

impl DavMetaData for NasWebDavMetaData {
    fn len(&self) -> u64 {
        match self {
            Self::Root => 0,
            Self::MountRoot { disk_used, .. } => *disk_used,
            Self::Node(VfsNode::Folder(_)) => 0,
            Self::Node(VfsNode::File(f)) => f.size,
        }
    }
    fn is_dir(&self) -> bool {
        match self.vfs_node() {
            None => true,
            Some(VfsNode::Folder(_)) => true,
            Some(VfsNode::File(_)) => false,
        }
    }
    fn modified(&self) -> FsResult<SystemTime> {
        match self.vfs_node() {
            None => Ok(SystemTime::now()),
            Some(VfsNode::Folder(f)) => parse_rfc3339_time(&f.updated_at),
            Some(VfsNode::File(f)) => parse_rfc3339_time(&f.updated_at),
        }
    }
    fn created(&self) -> FsResult<SystemTime> {
        match self.vfs_node() {
            None => Ok(SystemTime::now()),
            Some(VfsNode::Folder(f)) => {
                parse_rfc3339_time(&f.created_at).or_else(|_| parse_rfc3339_time(&f.updated_at))
            }
            Some(VfsNode::File(f)) => {
                parse_rfc3339_time(&f.created_at).or_else(|_| parse_rfc3339_time(&f.updated_at))
            }
        }
    }
    // 💡 1. 이게 진짜 파일인지 윈도우에게 확신을 줍니다.
    fn is_file(&self) -> bool {
        matches!(self.vfs_node(), Some(VfsNode::File(_)))
    }
    fn etag(&self) -> Option<String> {
        match self.vfs_node() {
            Some(VfsNode::File(f)) => {
                f.checksum.clone().or_else(|| Some(f.id.clone()))
            }
            Some(VfsNode::Folder(f)) => Some(format!("folder-{}", f.id)),
            None => None,
        }
    }
}

/// `/crew-uuid/` 처럼 Crew ID만 있는 WebDAV 마운트 루트인지 확인한다.
fn is_webdav_mount_root(path: &str) -> bool {
    path.trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .count()
        <= 1
}

fn parse_rfc3339_time(value: &str) -> FsResult<SystemTime> {
    if value.is_empty() {
        return Ok(SystemTime::now());
    }
    DateTime::parse_from_rfc3339(value)
        .map(|dt| SystemTime::from(dt.with_timezone(&Utc)))
        .or_else(|_| {
            // SQLite datetime('now') 등 레거시 형식 대비
            value
                .parse::<DateTime<Utc>>()
                .map(|dt| SystemTime::from(dt))
        })
        .map_err(|_| FsError::GeneralFailure)
}

// =================================================================
// 3. 가상 디렉토리 항목 (리스트에 뿌려질 녀석들)
// =================================================================
#[derive(Debug)]
pub struct NasWebDavDirEntry {
    node: VfsNode,
}

impl DavDirEntry for NasWebDavDirEntry {
    fn name(&self) -> Vec<u8> {
        match &self.node {
            VfsNode::Folder(f) => f.name.as_bytes().to_vec(),
            VfsNode::File(f) => f.name.as_bytes().to_vec(),
        }
    }
    fn metadata(&self) -> BoxFuture<'_, FsResult<Box<dyn DavMetaData>>> {
        Box::pin(async move {
            Ok(Box::new(NasWebDavMetaData::Node(self.node.clone())) as Box<dyn DavMetaData>)
        })
    }
}

// =================================================================
// 4. 가상 파일 스트리머
// =================================================================
#[derive(Debug)]
pub struct NasWebDavFile {
    file: tokio::fs::File,
    meta: ObjectMetadata,
    pub nas_service: Arc<NasService>,
}

impl DavFile for NasWebDavFile {
    // 1. 윈도우가 "이 파일 정보 다시 줘" 할 때 DB 정보를 정확히 넘깁니다.
    fn metadata(&mut self) -> BoxFuture<'_, FsResult<Box<dyn DavMetaData>>> {
        Box::pin(async move {
            Ok(
                Box::new(NasWebDavMetaData::Node(VfsNode::File(self.meta.clone())))
                    as Box<dyn DavMetaData>,
            )
        })
    }

    // 2. 핵심: 동영상 플레이어가 요청한 '만큼'만 딱 잘라서 보냅니다.
    fn read_bytes(&mut self, count: usize) -> BoxFuture<'_, FsResult<bytes::Bytes>> {
        Box::pin(async move {
            // 💡 BytesMut으로 메모리 공간을 미리 확보합니다.
            let actual_count = std::cmp::min(count, 64 * 1024);
            // 안전을 위해 0으로 채우거나, 바로 읽기 버퍼로 사용합니다.
            // 여기서는 읽기 버퍼로 바로 넘기기 위해 임시 벡터 대신
            // 기존 방식을 유지하되 Bytes로의 변환 효율을 높입니다.
            let mut temp_vec = vec![0u8; actual_count];
            match self.file.read(&mut temp_vec).await {
                Ok(0) => Ok(bytes::Bytes::new()), // 파일 끝
                Ok(n) => {
                    temp_vec.truncate(n); // 읽은 만큼만 자르기
                    Ok(bytes::Bytes::from(temp_vec))
                }
                Err(e) => {
                    tracing::error!("File Read Error: {:?}", e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }

    // 3. 핵심: 동영상의 특정 시간대(위치)로 점프할 수 있게 합니다.
    fn seek(&mut self, pos: SeekFrom) -> BoxFuture<'_, FsResult<u64>> {
        Box::pin(async move {
            match self.file.seek(pos).await {
                Ok(new_pos) => Ok(new_pos),
                Err(e) => {
                    tracing::error!("File Seek Error: {:?}", e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }

    // webdav.rs 내 NasWebDavFile 구현부
    fn write_buf(&mut self, buf: Box<dyn bytes::Buf + Send>) -> BoxFuture<'_, FsResult<()>> {
        Box::pin(async move {
            // 💡 1. buf에서 현재 데이터 조각을 가져옵니다.
            let bytes = buf.chunk();

            // 💡 2. Cursor를 사용하여 복사합니다.
            // self.file을 직접 가변 참조로 넘깁니다.
            let mut cursor = std::io::Cursor::new(bytes);

            if let Err(e) = tokio::io::copy_buf(&mut cursor, &mut self.file).await {
                tracing::error!("🔥 파일 쓰기 실패: {:?}", e);
                return Err(FsError::GeneralFailure);
            }

            Ok(())
        })
    }
    // 윈도우는 보통 write_bytes를 더 많이 씁니다.
    fn write_bytes(&mut self, data: bytes::Bytes) -> BoxFuture<'_, FsResult<()>> {
        Box::pin(async move {
            if let Err(e) = self.file.write_all(&data).await {
                tracing::error!("🔥 파일 쓰기 실패: {:?}", e);
                return Err(FsError::GeneralFailure);
            }
            Ok(())
        })
    }
    fn flush(&mut self) -> BoxFuture<'_, FsResult<()>> {
        Box::pin(async move {
            self.file
                .flush()
                .await
                .map_err(|_| FsError::GeneralFailure)?;

            let attr = self
                .file
                .metadata()
                .await
                .map_err(|_| FsError::GeneralFailure)?;
            let final_size = attr.len();

            let now = chrono::Utc::now().to_rfc3339();
            let created_at = self
                .nas_service
                .repository
                .find_file_by_id(&self.meta.id)
                .await
                .ok()
                .flatten()
                .map(|m| m.created_at)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    if self.meta.created_at.is_empty() {
                        now.clone()
                    } else {
                        self.meta.created_at.clone()
                    }
                });

            let final_meta = ObjectMetadata {
                id: self.meta.id.clone(),
                folder_id: self.meta.folder_id.clone(),
                name: self.meta.name.clone(),
                size: final_size,
                file_type: Some(
                    mime_guess::from_path(&self.meta.name)
                        .first_or_octet_stream()
                        .to_string(),
                ),
                is_deleted: false,
                created_at,
                updated_at: now,
                checksum: None,
                version: 1,
            };

            self.nas_service
                .repository
                .save_metadata(final_meta.clone())
                .await
                .map_err(|e| {
                    tracing::error!("❌ DB 메타데이터 저장 실패: {:?}", e);
                    FsError::GeneralFailure
                })?;

            self.meta = final_meta;
            Ok(())
        })
    }
}
