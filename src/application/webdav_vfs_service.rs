use crate::application::service::NasService;
use crate::domain::models::{FolderMetadata, ObjectMetadata};
use crate::domain::ports::FilesRepositoryPort;
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;
use urlencoding::decode;

#[derive(Debug, Clone)]
pub struct ParsedCrewPath {
    pub crew_id: String,
    pub inner_segments: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum VfsNode {
    Folder(FolderMetadata),
    File(ObjectMetadata),
}

pub struct WebDavVfsService {
    pub nas_service: Arc<NasService>,
    pub repo: Arc<dyn FilesRepositoryPort>,
}

impl WebDavVfsService {
    pub fn new(nas_service: Arc<NasService>, repo: Arc<dyn FilesRepositoryPort>) -> Self {
        Self { nas_service, repo }
    }

    /// Windows WebDAV 경로 세그먼트 정규화 (NFC + 끝 공백/마침표 제거).
    fn normalize_segment(segment: &str) -> String {
        let trimmed = segment.trim_end_matches(|c: char| c == ' ' || c == '.');
        trimmed.nfc().collect::<String>()
    }

    fn parse_crew_path(path: &str) -> Result<ParsedCrewPath, String> {
        let decoded_path = decode(path)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| path.to_string());

        let segments: Vec<String> = decoded_path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .map(Self::normalize_segment)
            .collect();

        if segments.is_empty() {
            return Err("WebDAV 경로에 Crew ID가 필요합니다.".into());
        }

        Ok(ParsedCrewPath {
            crew_id: segments[0].clone(),
            inner_segments: segments[1..].to_vec(),
        })
    }

    async fn find_folder_child(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<Option<FolderMetadata>, String> {
        let normalized = Self::normalize_segment(name);
        for candidate in [name, normalized.as_str()] {
            if let Ok(found) = self
                .repo
                .find_folder_by_name_and_parent_id(candidate, parent_id)
                .await
            {
                if found.is_some() {
                    return Ok(found);
                }
            }
        }
        Ok(None)
    }

    async fn find_file_child(
        &self,
        name: &str,
        folder_id: Option<&str>,
    ) -> Result<Option<ObjectMetadata>, String> {
        let normalized = Self::normalize_segment(name);
        for candidate in [name, normalized.as_str()] {
            if let Ok(found) = self
                .repo
                .find_file_by_name_and_folder_id(candidate, folder_id)
                .await
            {
                if found.is_some() {
                    return Ok(found);
                }
            }
        }
        Ok(None)
    }

    async fn authorized_crew(
        &self,
        user_id: i64,
        crew_id: &str,
    ) -> Result<crate::domain::models::Crew, String> {
        self.nas_service
            .authorize_webdav_crew(user_id, crew_id)
            .await
            .map_err(|e| format!("{:?}", e))
    }

    async fn folder_id_for_parsed(
        &self,
        user_id: i64,
        parsed: &ParsedCrewPath,
    ) -> Result<Option<String>, String> {
        let crew = self.authorized_crew(user_id, &parsed.crew_id).await?;
        let root_id = crew
            .root_folder_id
            .ok_or_else(|| "Crew 루트 폴더가 없습니다.".to_string())?;

        if parsed.inner_segments.is_empty() {
            return Ok(Some(root_id));
        }

        let mut current_parent_id = Some(root_id);
        for (i, segment) in parsed.inner_segments.iter().enumerate() {
            let is_last = i == parsed.inner_segments.len() - 1;
            if let Some(folder) = self
                .find_folder_child(segment, current_parent_id.as_deref())
                .await?
            {
                if is_last {
                    return Ok(Some(folder.id));
                }
                current_parent_id = Some(folder.id);
            } else if is_last {
                return Ok(None);
            } else {
                return Err(format!("경로를 찾을 수 없습니다: {}", segment));
            }
        }

        Ok(current_parent_id)
    }

    pub async fn resolve_path(&self, path: &str, user_id: i64) -> Result<Option<VfsNode>, String> {
        let parsed = Self::parse_crew_path(path)?;
        let crew = self.authorized_crew(user_id, &parsed.crew_id).await?;
        let root_id = crew.root_folder_id.unwrap();

        if parsed.inner_segments.is_empty() {
            let folder = self
                .repo
                .find_folder_by_id(&root_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "Crew 루트 폴더를 찾을 수 없습니다.".to_string())?;
            return Ok(Some(VfsNode::Folder(folder)));
        }

        let mut current_parent_id = Some(root_id);

        for (i, segment) in parsed.inner_segments.iter().enumerate() {
            let is_last = i == parsed.inner_segments.len() - 1;

            if let Some(folder) = self
                .find_folder_child(segment, current_parent_id.as_deref())
                .await?
            {
                if is_last {
                    return Ok(Some(VfsNode::Folder(folder)));
                }
                current_parent_id = Some(folder.id);
            } else if is_last {
                if let Some(file) = self
                    .find_file_child(segment, current_parent_id.as_deref())
                    .await?
                {
                    return Ok(Some(VfsNode::File(file)));
                }
                return Ok(None);
            } else {
                return Ok(None);
            }
        }

        Ok(None)
    }

    /// Web UI(`list_items`)와 동일한 방식으로 폴더 내용을 조회한다.
    pub async fn list_directory(&self, path: &str, user_id: i64) -> Result<Vec<VfsNode>, String> {
        let parsed = Self::parse_crew_path(path)?;
        self.authorized_crew(user_id, &parsed.crew_id).await?;

        let folder_id = if parsed.inner_segments.is_empty() {
            self.authorized_crew(user_id, &parsed.crew_id)
                .await?
                .root_folder_id
        } else {
            self.folder_id_for_parsed(user_id, &parsed).await?
        };

        let Some(folder_id) = folder_id else {
            return Err(format!(
                "폴더를 찾을 수 없습니다: {}",
                parsed.inner_segments.join("/")
            ));
        };

        let items = self
            .repo
            .list_items(Some(&folder_id))
            .await
            .map_err(|e| e.to_string())?;

        let mut nodes = Vec::with_capacity(items.len());
        for item in items {
            if item.is_dir {
                if let Ok(Some(folder)) = self.repo.find_folder_by_id(&item.id).await {
                    nodes.push(VfsNode::Folder(folder));
                }
            } else if let Ok(Some(file)) = self.repo.find_file_by_id(&item.id).await {
                nodes.push(VfsNode::File(file));
            }
        }

        Ok(nodes)
    }

    async fn resolve_parent_and_name_in_crew(
        &self,
        user_id: i64,
        parsed: &ParsedCrewPath,
    ) -> Result<(Option<String>, String), String> {
        if parsed.inner_segments.is_empty() {
            return Err("Crew 루트에는 파일을 직접 만들 수 없습니다.".into());
        }

        let crew = self.authorized_crew(user_id, &parsed.crew_id).await?;
        let root_id = crew.root_folder_id.unwrap();

        let target_name = parsed
            .inner_segments
            .last()
            .cloned()
            .ok_or_else(|| "대상 이름이 없습니다.".to_string())?;

        let parent_segments = &parsed.inner_segments[..parsed.inner_segments.len() - 1];
        let mut current_parent_id: Option<String> = Some(root_id);

        for segment in parent_segments {
            if let Some(folder) = self
                .find_folder_child(segment, current_parent_id.as_deref())
                .await?
            {
                current_parent_id = Some(folder.id);
            } else {
                return Err(format!("상위 경로를 찾을 수 없습니다: {}", segment));
            }
        }

        Ok((current_parent_id, target_name))
    }

    pub async fn create_folder_by_path(&self, path: &str, user_id: i64) -> Result<String, String> {
        let parsed = Self::parse_crew_path(path)?;
        let (parent_id, name) = self
            .resolve_parent_and_name_in_crew(user_id, &parsed)
            .await?;

        self.nas_service
            .resolve_folder_access(Some(user_id), parent_id.as_deref(), true)
            .await
            .map_err(|e| format!("{:?}", e))?;

        self.nas_service
            .create_folder(
                Some(&name),
                parent_id.as_deref(),
                Some(user_id),
                Some(&parsed.crew_id),
                false,
            )
            .await
            .map_err(|e| format!("{:?}", e))
    }

    pub async fn prepare_file_for_put(
        &self,
        path: &str,
        user_id: i64,
    ) -> Result<(String, Option<String>, String), String> {
        let parsed = Self::parse_crew_path(path)?;
        let (parent_id, name) = self
            .resolve_parent_and_name_in_crew(user_id, &parsed)
            .await?;

        self.nas_service
            .resolve_folder_access(Some(user_id), parent_id.as_deref(), true)
            .await
            .map_err(|e| format!("{:?}", e))?;

        let new_id = uuid::Uuid::new_v4().to_string();
        Ok((new_id, parent_id, name))
    }

    /// WebDAV MOVE: 같은 Crew 안에서 파일/폴더 이름 변경 또는 이동.
    pub async fn rename_path(
        &self,
        from_path: &str,
        to_path: &str,
        user_id: i64,
    ) -> Result<(), String> {
        let from_parsed = Self::parse_crew_path(from_path)?;
        let to_parsed = Self::parse_crew_path(to_path)?;

        if from_parsed.crew_id != to_parsed.crew_id {
            return Err("Crew 간 이동은 지원하지 않습니다.".into());
        }

        self.authorized_crew(user_id, &from_parsed.crew_id).await?;

        let from_node = self
            .resolve_path(from_path, user_id)
            .await?
            .ok_or_else(|| "원본 항목을 찾을 수 없습니다.".to_string())?;

        let (new_parent_id, new_name) = self
            .resolve_parent_and_name_in_crew(user_id, &to_parsed)
            .await?;

        match from_node {
            VfsNode::Folder(folder) => {
                self.nas_service
                    .rename_folder(
                        &folder.id,
                        &new_name,
                        new_parent_id.as_deref(),
                        Some(user_id),
                    )
                    .await
                    .map_err(|e| format!("{:?}", e))?;
            }
            VfsNode::File(file) => {
                self.nas_service
                    .rename_file(
                        &file.id,
                        &new_name,
                        new_parent_id.as_deref(),
                        Some(user_id),
                    )
                    .await
                    .map_err(|e| format!("{:?}", e))?;
            }
        }

        Ok(())
    }
}
