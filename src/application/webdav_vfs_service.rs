use crate::application::service::NasService;
use crate::domain::models::{FolderMetadata, ObjectMetadata};
use crate::domain::ports::RepositoryPort;
use std::sync::Arc;
use urlencoding::decode;

// 폴더인지 파일인지 묶어서 반환하기 위한 Enum
#[derive(Debug, Clone)]
pub enum VfsNode {
    Folder(FolderMetadata),
    File(ObjectMetadata),
}

pub struct WebDavVfsService {
    // 💡 NasService와 Repo를 모두 가져와서 일체화합니다.
    pub nas_service: Arc<NasService>,
    pub repo: Arc<dyn RepositoryPort>,
}

impl WebDavVfsService {
    pub fn new(nas_service: Arc<NasService>, repo: Arc<dyn RepositoryPort>) -> Self {
        Self { nas_service, repo }
    }

    /// 💡 핵심 번역기: 경로를 DB 노드로 추적 (webdav.rs 호환용)
    pub async fn resolve_path(&self, path: &str) -> Result<Option<VfsNode>, String> {
        let decoded_path = decode(path)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| path.to_string());

        let segments: Vec<&str> = decoded_path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if segments.is_empty() {
            return Ok(Some(VfsNode::Folder(FolderMetadata {
                id: "".to_string(),
                parent_id: None,
                name: "root".to_string(),
                created_at: "".to_string(),
                updated_at: "".to_string(),
            })));
        }

        let mut current_parent_id: Option<String> = None;
        let mut last_node: Option<VfsNode> = None;

        for (i, &segment) in segments.iter().enumerate() {
            let is_last = i == segments.len() - 1;

            // 1. 먼저 폴더인지 찾아봄
            if let Ok(Some(folder)) = self
                .repo
                .find_folder_by_name_and_parent_id(segment, current_parent_id.as_deref())
                .await
            {
                if is_last {
                    return Ok(Some(VfsNode::Folder(folder)));
                }
                current_parent_id = Some(folder.id.clone());
                last_node = Some(VfsNode::Folder(folder));
            }
            // 2. 마지막 경로라면 파일인지 찾아봄
            else if is_last {
                if let Ok(Some(file)) = self
                    .repo
                    .find_file_by_name_and_folder_id(segment, current_parent_id.as_deref())
                    .await
                {
                    last_node = Some(VfsNode::File(file));
                } else {
                    return Ok(None);
                }
            } else {
                return Ok(None);
            }
        }
        Ok(last_node)
    }

    /// 💡 폴더 내부 목록 (webdav.rs 호환용)
    pub async fn list_directory(&self, folder_id: Option<&str>) -> Result<Vec<VfsNode>, String> {
        let mut nodes = Vec::new();
        if let Ok(folders) = self.repo.list_folders_by_parent(folder_id).await {
            for f in folders {
                nodes.push(VfsNode::Folder(f));
            }
        }
        if let Ok(files) = self.repo.list_files_by_folder(folder_id).await {
            for f in files {
                nodes.push(VfsNode::File(f));
            }
        }
        Ok(nodes)
    }

    /// 헬퍼: 경로에서 부모 ID와 이름을 분리
    pub async fn resolve_parent_and_name(
        &self,
        path: &str,
    ) -> Result<(Option<String>, String), String> {
        let decoded_path = decode(path)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| path.to_string());
        let mut segments: Vec<&str> = decoded_path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if segments.is_empty() {
            return Err("Root cannot be a target".into());
        }
        let target_name = segments.pop().unwrap().to_string();

        let mut current_parent_id: Option<String> = None;
        for segment in segments {
            if let Ok(Some(folder)) = self
                .repo
                .find_folder_by_name_and_parent_id(segment, current_parent_id.as_deref())
                .await
            {
                current_parent_id = Some(folder.id);
            } else {
                return Err(format!("Parent path not found: {}", segment));
            }
        }
        Ok((current_parent_id, target_name))
    }

    /// 📂 NasService 연동 폴더 생성
    pub async fn create_folder_by_path(&self, path: &str) -> Result<String, String> {
        let (parent_id, name) = self.resolve_parent_and_name(path).await?;
        self.nas_service
            .create_folder(Some(&name), parent_id.as_deref())
            .await
            .map_err(|e| format!("{:?}", e))
    }

    /// 📄 NasService 연동 업로드 준비
    pub async fn prepare_file_for_put(
        &self,
        path: &str,
    ) -> Result<(String, Option<String>, String), String> {
        let (parent_id, name) = self.resolve_parent_and_name(path).await?;

        // 💡 중복 체크는 NasService의 로직이 복잡하므로 여기서는 새 ID만 발급하고
        // 이름 결정 로직은 NasService와 맞추는 것이 좋습니다.
        let new_id = uuid::Uuid::new_v4().to_string();
        Ok((new_id, parent_id, name))
    }
}
