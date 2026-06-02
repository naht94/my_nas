use crate::domain::errors::NasError;
use crate::domain::models::FolderMetadata;
use crate::domain::ports::FilesRepositoryPort;
use crate::domain::{errors::RepositoryResult, models::ObjectMetadata};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};

pub struct SqliteFilesRepository {
    pool: Pool<Sqlite>,
}

impl SqliteFilesRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FilesRepositoryPort for SqliteFilesRepository {
    async fn save_metadata(&self, meta: ObjectMetadata) -> RepositoryResult<()> {
        let size_i64 = meta.size as i64;
        sqlx::query!(
            r#"
        INSERT INTO files (
            id, folder_id, name, size, 
            file_type, checksum, version, is_deleted, 
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            size = excluded.size,
            file_type = excluded.file_type,
            checksum = excluded.checksum,
            version = files.version + 1,
            updated_at = excluded.updated_at
        "#,
            meta.id, // 이것이 곧 물리 파일의 식별자입니다.
            meta.folder_id,
            meta.name,
            size_i64,
            meta.file_type,
            meta.checksum,
            meta.version,
            meta.is_deleted,
            meta.created_at,
            meta.updated_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
    async fn exists_folder_id_by_id(&self, id: &str) -> Result<bool, NasError> {
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }
    async fn exists_file_id_by_id(&self, id: &str) -> Result<bool, NasError> {
        let count: i32 = sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE id = ?")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }
    async fn find_file_by_id(&self, id: &str) -> RepositoryResult<Option<ObjectMetadata>> {
        let r = sqlx::query!(
            r#"
            SELECT id as "id!", folder_id, name as "name!", size as "size!", file_type, 
                   checksum, version as "version!", is_deleted as "is_deleted!", 
                   created_at as "created_at!", updated_at as "updated_at!"
            FROM files 
            WHERE id = ? AND is_deleted = 0
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(r.map(|r| ObjectMetadata {
            id: r.id,
            folder_id: r.folder_id,
            name: r.name,
            size: r.size as u64,
            file_type: r.file_type,
            checksum: r.checksum,
            version: r.version as i32,
            is_deleted: r.is_deleted != 0,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn find_folder_by_id(&self, id: &str) -> RepositoryResult<Option<FolderMetadata>> {
        let r = sqlx::query!(
            r#"
            SELECT id as "id!", parent_id, name as "name!", created_at as "created_at!", updated_at as "updated_at!"
            FROM folders
            WHERE id = ? AND is_deleted = 0
            "#,
            id
        ).fetch_optional(&self.pool)
            .await?;

        Ok(r.map(|r| FolderMetadata {
            id: r.id,
            parent_id: r.parent_id,
            name: r.name,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn find_folder_by_name_and_parent_id(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Option<FolderMetadata>> {
        let r = sqlx::query!(
        r#"
        SELECT id as "id!", parent_id, name as "name!", created_at as "created_at!", updated_at as "updated_at!"
        FROM folders
        WHERE parent_id IS ? AND name = ? AND is_deleted = 0
        "#,
            parent_id,
            name
    ).fetch_optional(&self.pool)
            .await?;

        Ok(r.map(|r| FolderMetadata {
            id: r.id,
            parent_id: r.parent_id,
            name: r.name,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    async fn find_file_by_name_and_folder_id(
        &self,
        name: &str,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Option<ObjectMetadata>> {
        let r = sqlx::query!(
            r#"
        SELECT id as "id!", folder_id, name as "name!", size as "size!", file_type, 
        checksum, version as "version!", is_deleted as "is_deleted!", 
        created_at as "created_at!", updated_at as "updated_at!"
        FROM files 
        WHERE folder_id IS ? AND name = ? AND is_deleted = 0
        "#,
            folder_id,
            name,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(r.map(|r| ObjectMetadata {
            id: r.id,
            folder_id: r.folder_id,
            name: r.name,
            size: r.size as u64,
            file_type: r.file_type,
            checksum: r.checksum,
            version: r.version as i32,
            is_deleted: r.is_deleted != 0,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }
    async fn list_files_by_folder(
        &self,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>> {
        // 1. 쿼리 수정: object_key, real_path 등을 제거하고 folder_id로 필터링
        let records = sqlx::query!(
            r#"
        SELECT 
            id as "id!", folder_id, name as "name!", size as "size!", 
            file_type, checksum, version as "version!", is_deleted as "is_deleted!", 
            created_at, updated_at
        FROM files 
        WHERE folder_id IS ? AND is_deleted = 0
        "#,
            folder_id // Option<&str>을 그대로 전달 (SQLite에서 IS NULL 처리가 가능함)
        )
        .fetch_all(&self.pool)
        .await?;

        // 2. 새로운 ObjectMetadata 구조체에 맞게 매핑 (제거된 필드 제외)
        let metas = records
            .into_iter()
            .map(|r| ObjectMetadata {
                id: r.id,
                folder_id: r.folder_id,
                name: r.name,
                size: r.size as u64,
                file_type: r.file_type,
                checksum: r.checksum,
                version: r.version as i32,
                is_deleted: r.is_deleted != 0,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect();

        Ok(metas)
    }
    async fn delete_file(&self, id: &str) -> RepositoryResult<()> {
        // 하드 삭제 대신 소프트 삭제(Soft Delete) 적용
        sqlx::query!("UPDATE files SET is_deleted = 1 WHERE id = ?", id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
    async fn save_folder(
        &self,
        folder: crate::domain::models::FolderMetadata,
    ) -> RepositoryResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO folders (id, parent_id, name, is_deleted, created_at, updated_at)
            VALUES (?, ?, ?, 0, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                updated_at = excluded.updated_at
            "#,
            folder.id,
            folder.parent_id,
            folder.name,
            folder.created_at,
            folder.updated_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    //  특정 부모 폴더 아래의 depth 0 하위 폴더들 조회
    async fn list_folders_by_parent(
        &self,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Vec<crate::domain::models::FolderMetadata>> {
        let records = sqlx::query!(
            r#"
            SELECT id as "id!", parent_id, name as "name!", 
                   created_at as "created_at!", updated_at as "updated_at!"
            FROM folders 
            WHERE parent_id IS ? AND is_deleted = 0
            "#,
            parent_id
        )
        .fetch_all(&self.pool)
        .await?;

        let folders = records
            .into_iter()
            .map(|r| crate::domain::models::FolderMetadata {
                id: r.id,
                parent_id: r.parent_id,
                name: r.name,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect();

        Ok(folders)
    }

    //  가상 폴더 삭제 (소프트 삭제를 적용하려면 테이블에 is_deleted 컬럼 필요, 현재는 하드 삭제 기준)
    async fn delete_folder(&self, folder_id: &str) -> RepositoryResult<()> {
        let mut tx = self.pool.begin().await?;

        // 1. 폴더들 소프트 삭제 (Recursive CTE)
        sqlx::query!(
            r#"
        WITH RECURSIVE sub_folders(id) AS (
            SELECT id FROM folders WHERE id = ?
            UNION ALL
            SELECT f.id FROM folders f JOIN sub_folders sf ON f.parent_id = sf.id
        )
        UPDATE folders SET is_deleted = 1, updated_at = datetime('now')
        WHERE id IN (SELECT id FROM sub_folders)
        "#,
            folder_id
        )
        .execute(&mut *tx)
        .await?;

        // 2. 파일들 소프트 삭제
        sqlx::query!(
            r#"
        WITH RECURSIVE sub_folders(id) AS (
            SELECT id FROM folders WHERE id = ?
            UNION ALL
            SELECT f.id FROM folders f JOIN sub_folders sf ON f.parent_id = sf.id
        )
        UPDATE files SET is_deleted = 1, updated_at = datetime('now')
        WHERE folder_id IN (SELECT id FROM sub_folders)
        "#,
            folder_id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    // 삭제할 파일들의 ID(물리 파일명) 목록을 가져오는 기능이 필요합니다.
    async fn get_deleted_files(&self) -> RepositoryResult<Vec<ObjectMetadata>> {
        let records = sqlx::query!(
            r#"
        SELECT 
            id as "id!", 
            folder_id, 
            name as "name!", 
            size as "size!", 
            file_type, 
            checksum, 
            version as "version!", 
            is_deleted as "is_deleted!", 
            created_at as "created_at!", 
            updated_at as "updated_at!"
        FROM files 
        WHERE is_deleted = 1
        "#
        )
        .fetch_all(&self.pool)
        .await?;

        let metas = records
            .into_iter()
            .map(|r| ObjectMetadata {
                id: r.id,
                folder_id: r.folder_id,
                name: r.name,
                size: r.size as u64,
                file_type: r.file_type,
                checksum: r.checksum,
                version: r.version as i32,     // 💡 i64를 i32로 명시적 변환
                is_deleted: r.is_deleted != 0, // 💡 0이 아니면 true (bool 변환)
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect();

        Ok(metas)
    }

    // DB에서 완전히 제거하는 기능
    async fn permanent_delete_file(&self, id: &str) -> RepositoryResult<()> {
        sqlx::query!(r#"DELETE FROM files WHERE is_deleted = 1 AND id = ?"#, id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    async fn permanent_delete_folders(&self) -> RepositoryResult<()> {
        loop {
            let result = sqlx::query!(
                r#"
                DELETE FROM folders 
                WHERE is_deleted = 1 
                  AND NOT EXISTS (SELECT 1 FROM files WHERE folder_id = folders.id)
                  AND NOT EXISTS (SELECT 1 FROM folders AS sub WHERE sub.parent_id = folders.id)
                "#
            )
            .execute(&self.pool)
            .await?;

            if result.rows_affected() == 0 {
                break;
            } // 더 이상 지울 빈 폴더가 없으면 종료
        }
        Ok(())
    }

    // 폴더와 파일을 ListItem으로 통합 조회
    async fn list_items(
        &self,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Vec<crate::domain::models::ListItem>> {
        // 1. 하위 폴더들 가져오기
        let folders = self.list_folders_by_parent(folder_id).await?;
        // 2. 하위 파일들 가져오기
        let files = self.list_files_by_folder(folder_id).await?;

        let mut items = Vec::new();

        for f in folders {
            items.push(crate::domain::models::ListItem {
                id: f.id,
                name: f.name,
                is_dir: true,
                size: 0,
                created_at: f.created_at,
                file_type: None,
                has_children: None,
                preview_url: None,
            });
        }

        for f in files {
            items.push(crate::domain::models::ListItem {
                id: f.id,
                name: f.name,
                is_dir: false,
                size: f.size,
                created_at: f.created_at,
                file_type: f.file_type,
                has_children: None,
                preview_url: None,
            });
        }

        Ok(items)
    }

    async fn exists_folder_by_name_and_parent(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<bool, NasError> {
        let row = sqlx::query!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM folders
                WHERE name = ? AND parent_id IS ? AND is_deleted = 0
            ) as exists_flag
            "#,
            name,
            parent_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.exists_flag == 1)
    }

    async fn exists_file_by_name_and_folder(
        &self,
        name: &str,
        folder_id: Option<&str>,
    ) -> Result<bool, NasError> {
        let row = sqlx::query!(
            r#"
        SELECT EXISTS(
            SELECT 1 FROM files
            WHERE name = ? AND folder_id IS ? AND is_deleted = 0
        ) as exists_flag
        "#,
            name,
            folder_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.exists_flag == 1)
    }
}
