use crate::domain::errors::NasError;

use super::service::NasService;

impl NasService {
    /// 같은 부모 아래에 동일 이름의 파일·폴더가 없는지 확인한다.
    pub(crate) async fn ensure_unique_sibling_name(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<(), NasError> {
        if self
            .repository
            .exists_folder_by_name_and_parent(name, parent_id)
            .await?
        {
            return Err(NasError::BadRequest(
                "같은 이름의 폴더가 이미 있습니다.".into(),
            ));
        }
        if self
            .repository
            .exists_file_by_name_and_folder(name, parent_id)
            .await?
        {
            return Err(NasError::BadRequest(
                "같은 이름의 파일이 이미 있습니다.".into(),
            ));
        }
        Ok(())
    }
}
