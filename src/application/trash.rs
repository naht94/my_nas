use crate::domain::errors::NasError;
use crate::domain::models::{ListItem, ObjectMetadata};

use super::service::NasService;

/// 휴지통에서 영구 삭제할 대상. 공통 purge 경로로 수렴한다.
enum TrashPurgeTarget<'a> {
    /// Crew 스코프 전체 (empty trash)
    CrewScope(Option<&'a str>),
    /// 단일 소프트 삭제 파일
    File(&'a str),
    /// 소프트 삭제된 폴더와 하위 트리
    FolderTree(&'a str),
}

impl NasService {
    async fn authorize_trash_scope(
        &self,
        user_id: Option<i64>,
        scope_folder_id: Option<&str>,
    ) -> Result<Option<String>, NasError> {
        self.resolve_folder_access(user_id, scope_folder_id, true)
            .await?;
        match scope_folder_id {
            Some(fid) => self.repository.crew_id_for_folder(fid, false).await.map_err(Into::into),
            None => Ok(None),
        }
    }

    fn crew_scope_matches(expected: &Option<String>, actual: &Option<String>) -> bool {
        expected.as_deref() == actual.as_deref()
    }

    async fn try_purge_deleted_file(&self, file: &ObjectMetadata) {
        match self.storage.delete_file(&file.id).await {
            Ok(_) => {
                if let Err(e) = self.repository.permanent_delete_file(&file.id).await {
                    tracing::error!("DB 파일 영구 삭제 실패 (id: {}): {:?}", file.id, e);
                }
            }
            Err(e) => {
                tracing::error!(
                    "물리 파일 삭제 실패 (name: {}, size: {}): {:?}",
                    file.name,
                    file.size,
                    e
                );
            }
        }
    }

    async fn purge_deleted_file(&self, id: &str) -> Result<(), NasError> {
        self.storage.delete_file(id).await?;
        self.repository.permanent_delete_file(id).await?;
        Ok(())
    }

    async fn purge_deleted_folder_tree(&self, folder_id: &str) -> Result<(), NasError> {
        let file_ids = self
            .repository
            .list_file_ids_in_folder_tree(folder_id)
            .await?;
        for file_id in &file_ids {
            if let Err(e) = self.storage.delete_file(file_id).await {
                tracing::error!("물리 파일 삭제 실패 (id: {}): {:?}", file_id, e);
            }
        }
        self.repository
            .permanent_delete_folder_tree(folder_id)
            .await?;
        Ok(())
    }

    async fn purge_crew_scope_trash(&self, crew_id: Option<&str>) -> Result<(), NasError> {
        let files = self.repository.get_deleted_files(crew_id).await?;
        for file in files {
            self.try_purge_deleted_file(&file).await;
        }
        self.repository.permanent_delete_folders(crew_id).await?;
        Ok(())
    }

    async fn purge_trash(&self, target: TrashPurgeTarget<'_>) -> Result<(), NasError> {
        match target {
            TrashPurgeTarget::CrewScope(crew_id) => self.purge_crew_scope_trash(crew_id).await,
            TrashPurgeTarget::File(id) => self.purge_deleted_file(id).await,
            TrashPurgeTarget::FolderTree(id) => self.purge_deleted_folder_tree(id).await,
        }
    }

    async fn assert_item_in_trash_scope(
        &self,
        scope_crew: &Option<String>,
        id: &str,
        is_dir: bool,
    ) -> Result<(), NasError> {
        let item_crew = self.repository.trash_item_crew_id(id, is_dir).await?;

        if !Self::crew_scope_matches(scope_crew, &item_crew) {
            return Err(NasError::Forbidden(
                "이 위치의 휴지통 항목이 아닙니다.".into(),
            ));
        }

        if is_dir {
            let folder = self
                .repository
                .find_folder(id, false)
                .await?
                .ok_or(NasError::DataNotFound)?;
            if folder.is_crew_root() {
                return Err(NasError::Forbidden(
                    "Crew 루트 폴더에는 이 작업을 할 수 없습니다.".into(),
                ));
            }
        } else {
            let file = self
                .repository
                .find_file(id, false)
                .await?
                .ok_or(NasError::DataNotFound)?;
            if !file.is_deleted {
                return Err(NasError::BadRequest("휴지통에 없는 항목입니다.".into()));
            }
        }
        Ok(())
    }

    async fn assert_restorable(&self, id: &str, is_dir: bool) -> Result<(), NasError> {
        if is_dir {
            let folder = self
                .repository
                .find_folder(id, false)
                .await?
                .ok_or(NasError::DataNotFound)?;
            if let Some(ref parent_id) = folder.parent_id {
                self.repository
                    .find_folder(parent_id, true)
                    .await?
                    .ok_or(NasError::BadRequest(
                        "상위 폴더가 삭제되어 있어 복구할 수 없습니다. 상위 폴더를 먼저 복구하세요.".into(),
                    ))?;
            }
        } else {
            let file = self
                .repository
                .find_file(id, false)
                .await?
                .ok_or(NasError::DataNotFound)?;
            if let Some(ref folder_id) = file.folder_id {
                self.repository
                    .find_folder(folder_id, true)
                    .await?
                    .ok_or(NasError::BadRequest(
                        "파일이 속한 폴더가 삭제되어 있어 복구할 수 없습니다.".into(),
                    ))?;
            }
        }
        Ok(())
    }

    pub async fn list_trash(
        &self,
        user_id: Option<i64>,
        folder_id: Option<String>,
    ) -> Result<Vec<ListItem>, NasError> {
        let crew_id = self
            .authorize_trash_scope(user_id, folder_id.as_deref())
            .await?;
        self.repository
            .list_trash_items(crew_id.as_deref())
            .await
            .map_err(Into::into)
    }

    pub async fn empty_trash(
        &self,
        user_id: Option<i64>,
        folder_id: Option<String>,
    ) -> Result<(), NasError> {
        let crew_id = self
            .authorize_trash_scope(user_id, folder_id.as_deref())
            .await?;
        self.purge_trash(TrashPurgeTarget::CrewScope(crew_id.as_deref()))
            .await
    }

    pub async fn restore_trash_item(
        &self,
        user_id: Option<i64>,
        scope_folder_id: Option<String>,
        id: &str,
        is_dir: bool,
    ) -> Result<(), NasError> {
        let scope_crew = self
            .authorize_trash_scope(user_id, scope_folder_id.as_deref())
            .await?;
        self.assert_item_in_trash_scope(&scope_crew, id, is_dir)
            .await?;
        self.assert_restorable(id, is_dir).await?;

        if is_dir {
            self.repository.restore_folder_tree(id).await?;
        } else {
            self.repository.restore_file(id).await?;
        }
        Ok(())
    }

    pub async fn permanent_delete_trash_item(
        &self,
        user_id: Option<i64>,
        scope_folder_id: Option<String>,
        id: &str,
        is_dir: bool,
    ) -> Result<(), NasError> {
        let scope_crew = self
            .authorize_trash_scope(user_id, scope_folder_id.as_deref())
            .await?;
        self.assert_item_in_trash_scope(&scope_crew, id, is_dir)
            .await?;

        let target = if is_dir {
            TrashPurgeTarget::FolderTree(id)
        } else {
            TrashPurgeTarget::File(id)
        };
        self.purge_trash(target).await
    }
}
