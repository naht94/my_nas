use crate::domain::errors::NasError;
use crate::domain::models::{FolderMetadata, ListItem, ObjectMetadata};
use crate::domain::ports::FilesRepositoryPort;
use crate::domain::{errors::RepositoryResult};
use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};

/// 폴더 하위 트리를 대상으로 하는 SQL에 공통으로 붙이는 CTE.
const SUB_FOLDERS_CTE: &str = r#"
WITH RECURSIVE sub_folders(id) AS (
    SELECT id FROM folders WHERE id = ?
    UNION ALL
    SELECT f.id FROM folders f JOIN sub_folders sf ON f.parent_id = sf.id
)"#;

pub struct SqliteFilesRepository {
    pool: Pool<Sqlite>,
}

impl SqliteFilesRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    fn map_folder(row: &sqlx::sqlite::SqliteRow) -> FolderMetadata {
        FolderMetadata {
            id: row.get("id"),
            parent_id: row.get("parent_id"),
            crew_id: row.get("crew_id"),
            name: row.get("name"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    fn map_file(
        id: String,
        folder_id: Option<String>,
        name: String,
        size: i64,
        file_type: Option<String>,
        checksum: Option<String>,
        version: i64,
        is_deleted: i64,
        created_at: String,
        updated_at: String,
    ) -> ObjectMetadata {
        ObjectMetadata {
            id,
            folder_id,
            name,
            size: size as u64,
            file_type,
            checksum,
            version: version as i32,
            is_deleted: is_deleted != 0,
            created_at,
            updated_at,
        }
    }

    fn map_file_row(row: &sqlx::sqlite::SqliteRow) -> ObjectMetadata {
        Self::map_file(
            row.get("id"),
            row.get("folder_id"),
            row.get("name"),
            row.get("size"),
            row.get("file_type"),
            row.get("checksum"),
            row.get("version"),
            row.get("is_deleted"),
            row.get("created_at"),
            row.get("updated_at"),
        )
    }

    fn merge_list_items(
        folders: &[FolderMetadata],
        files: &[ObjectMetadata],
    ) -> Vec<ListItem> {
        let mut items = Vec::with_capacity(folders.len() + files.len());
        items.extend(folders.iter().map(ListItem::from_folder));
        items.extend(files.iter().map(ListItem::from_file));
        items
    }

    async fn list_deleted_files_in_crew(
        &self,
        crew_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>> {
        let records = sqlx::query!(
            r#"
            SELECT
                f.id as "id!",
                f.folder_id,
                f.name as "name!",
                f.size as "size!",
                f.file_type,
                f.checksum,
                f.version as "version!",
                f.is_deleted as "is_deleted!",
                f.created_at as "created_at!",
                f.updated_at as "updated_at!"
            FROM files f
            LEFT JOIN folders fo ON f.folder_id = fo.id
            WHERE f.is_deleted = 1 AND fo.crew_id IS ?
            ORDER BY f.name
            "#,
            crew_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|r| {
                Self::map_file(
                    r.id,
                    r.folder_id,
                    r.name,
                    r.size,
                    r.file_type,
                    r.checksum,
                    r.version,
                    r.is_deleted,
                    r.created_at,
                    r.updated_at,
                )
            })
            .collect())
    }

    async fn list_deleted_folders_in_crew(
        &self,
        crew_id: Option<&str>,
    ) -> RepositoryResult<Vec<FolderMetadata>> {
        let records = sqlx::query(
            r#"
            SELECT id, parent_id, crew_id, name, created_at, updated_at
            FROM folders
            WHERE is_deleted = 1 AND crew_id IS ?
            ORDER BY name
            "#,
        )
        .bind(crew_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records.iter().map(Self::map_folder).collect())
    }

    /// CTE 기반 폴더 트리 변경(소프트 삭제·복구 등)을 한 트랜잭션으로 실행한다.
    async fn mutate_folder_tree(
        &self,
        folder_id: &str,
        folders_sql: &str,
        files_sql: &str,
        timestamp: Option<&str>,
    ) -> RepositoryResult<()> {
        let mut tx = self.pool.begin().await?;

        let folder_stmt = format!("{SUB_FOLDERS_CTE} {folders_sql}");
        let mut fq = sqlx::query(&folder_stmt).bind(folder_id);
        if let Some(ts) = timestamp {
            fq = fq.bind(ts);
        }
        fq.execute(&mut *tx).await?;

        let file_stmt = format!("{SUB_FOLDERS_CTE} {files_sql}");
        let mut file_q = sqlx::query(&file_stmt).bind(folder_id);
        if let Some(ts) = timestamp {
            file_q = file_q.bind(ts);
        }
        file_q.execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn soft_delete_folder_tree(&self, folder_id: &str) -> RepositoryResult<()> {
        self.mutate_folder_tree(
            folder_id,
            "UPDATE folders SET is_deleted = 1, updated_at = datetime('now') WHERE id IN (SELECT id FROM sub_folders)",
            "UPDATE files SET is_deleted = 1, updated_at = datetime('now') WHERE folder_id IN (SELECT id FROM sub_folders)",
            None,
        )
        .await
    }

    async fn restore_folder_tree_inner(&self, folder_id: &str) -> RepositoryResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.mutate_folder_tree(
            folder_id,
            "UPDATE folders SET is_deleted = 0, updated_at = ? WHERE id IN (SELECT id FROM sub_folders) AND is_deleted = 1",
            "UPDATE files SET is_deleted = 0, updated_at = ? WHERE folder_id IN (SELECT id FROM sub_folders) AND is_deleted = 1",
            Some(&now),
        )
        .await
    }

    async fn purge_folder_tree(&self, folder_id: &str) -> RepositoryResult<()> {
        let mut tx = self.pool.begin().await?;

        let delete_files = format!(
            "{SUB_FOLDERS_CTE} DELETE FROM files WHERE folder_id IN (SELECT id FROM sub_folders)"
        );
        sqlx::query(&delete_files)
            .bind(folder_id)
            .execute(&mut *tx)
            .await?;

        loop {
            let delete_folders = format!(
                r#"
                {SUB_FOLDERS_CTE}
                DELETE FROM folders
                WHERE id IN (SELECT id FROM sub_folders)
                  AND NOT EXISTS (SELECT 1 FROM folders AS sub WHERE sub.parent_id = folders.id)
                "#
            );
            let result = sqlx::query(&delete_folders)
                .bind(folder_id)
                .execute(&mut *tx)
                .await?;
            if result.rows_affected() == 0 {
                break;
            }
        }

        tx.commit().await?;
        Ok(())
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
            meta.id,
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

    async fn find_file(&self, id: &str, active_only: bool) -> RepositoryResult<Option<ObjectMetadata>> {
        let sql = if active_only {
            r#"
            SELECT id, folder_id, name, size, file_type, checksum, version, is_deleted,
                   created_at, updated_at
            FROM files
            WHERE id = ? AND is_deleted = 0
            "#
        } else {
            r#"
            SELECT id, folder_id, name, size, file_type, checksum, version, is_deleted,
                   created_at, updated_at
            FROM files
            WHERE id = ?
            "#
        };

        let row = sqlx::query(sql).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(Self::map_file_row))
    }

    async fn find_folder(
        &self,
        id: &str,
        active_only: bool,
    ) -> RepositoryResult<Option<FolderMetadata>> {
        let sql = if active_only {
            r#"
            SELECT id, parent_id, crew_id, name, created_at, updated_at
            FROM folders
            WHERE id = ? AND is_deleted = 0
            "#
        } else {
            r#"
            SELECT id, parent_id, crew_id, name, created_at, updated_at
            FROM folders
            WHERE id = ?
            "#
        };

        let row = sqlx::query(sql).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(Self::map_folder))
    }

    async fn crew_id_for_folder(
        &self,
        folder_id: &str,
        active_only: bool,
    ) -> RepositoryResult<Option<String>> {
        Ok(self
            .find_folder(folder_id, active_only)
            .await?
            .and_then(|f| f.crew_id))
    }

    async fn trash_item_crew_id(&self, id: &str, is_dir: bool) -> RepositoryResult<Option<String>> {
        if is_dir {
            self.crew_id_for_folder(id, false).await
        } else {
            let Some(file) = self.find_file(id, false).await? else {
                return Ok(None);
            };
            match file.folder_id.as_deref() {
                Some(folder_id) => self.crew_id_for_folder(folder_id, false).await,
                None => Ok(None),
            }
        }
    }

    async fn delete_file(&self, id: &str) -> RepositoryResult<()> {
        sqlx::query!("UPDATE files SET is_deleted = 1 WHERE id = ?", id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_files_by_folder(
        &self,
        folder_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>> {
        let records = sqlx::query!(
            r#"
            SELECT
                id as "id!", folder_id, name as "name!", size as "size!",
                file_type, checksum, version as "version!", is_deleted as "is_deleted!",
                created_at as "created_at!", updated_at as "updated_at!"
            FROM files
            WHERE folder_id IS ? AND is_deleted = 0
            "#,
            folder_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(records
            .into_iter()
            .map(|r| {
                Self::map_file(
                    r.id,
                    r.folder_id,
                    r.name,
                    r.size,
                    r.file_type,
                    r.checksum,
                    r.version,
                    r.is_deleted,
                    r.created_at,
                    r.updated_at,
                )
            })
            .collect())
    }

    async fn save_folder(&self, folder: FolderMetadata) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            INSERT INTO folders (id, parent_id, crew_id, name, is_deleted, created_at, updated_at)
            VALUES (?, ?, ?, ?, 0, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                crew_id = excluded.crew_id,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&folder.id)
        .bind(&folder.parent_id)
        .bind(&folder.crew_id)
        .bind(&folder.name)
        .bind(&folder.created_at)
        .bind(&folder.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_folders_by_parent(
        &self,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Vec<FolderMetadata>> {
        let records = sqlx::query(
            r#"
            SELECT id, parent_id, crew_id, name, created_at, updated_at
            FROM folders
            WHERE parent_id IS ? AND is_deleted = 0
              AND (parent_id IS NOT NULL OR crew_id IS NULL)
            "#,
        )
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(records.iter().map(Self::map_folder).collect())
    }

    async fn delete_folder(&self, folder_id: &str) -> RepositoryResult<()> {
        self.soft_delete_folder_tree(folder_id).await
    }

    async fn get_deleted_files(
        &self,
        crew_id: Option<&str>,
    ) -> RepositoryResult<Vec<ObjectMetadata>> {
        self.list_deleted_files_in_crew(crew_id).await
    }

    async fn permanent_delete_file(&self, id: &str) -> RepositoryResult<()> {
        sqlx::query!(r#"DELETE FROM files WHERE is_deleted = 1 AND id = ?"#, id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn permanent_delete_folders(&self, crew_id: Option<&str>) -> RepositoryResult<()> {
        loop {
            let result = sqlx::query!(
                r#"
                DELETE FROM folders
                WHERE is_deleted = 1
                  AND crew_id IS ?
                  AND NOT EXISTS (SELECT 1 FROM files WHERE folder_id = folders.id)
                  AND NOT EXISTS (SELECT 1 FROM folders AS sub WHERE sub.parent_id = folders.id)
                "#,
                crew_id
            )
            .execute(&self.pool)
            .await?;

            if result.rows_affected() == 0 {
                break;
            }
        }
        Ok(())
    }

    async fn list_items(&self, folder_id: Option<&str>) -> RepositoryResult<Vec<ListItem>> {
        let folders = self.list_folders_by_parent(folder_id).await?;
        let files = self.list_files_by_folder(folder_id).await?;
        Ok(Self::merge_list_items(&folders, &files))
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

        Ok(r.map(|r| {
            Self::map_file(
                r.id,
                r.folder_id,
                r.name,
                r.size,
                r.file_type,
                r.checksum,
                r.version,
                r.is_deleted,
                r.created_at,
                r.updated_at,
            )
        }))
    }

    async fn find_folder_by_name_and_parent_id(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> RepositoryResult<Option<FolderMetadata>> {
        let row = sqlx::query(
            r#"
            SELECT id, parent_id, crew_id, name, created_at, updated_at
            FROM folders
            WHERE parent_id IS ? AND name = ? AND is_deleted = 0
            "#,
        )
        .bind(parent_id)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(Self::map_folder))
    }

    async fn update_folder_location(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> RepositoryResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE folders
            SET name = ?, parent_id = ?, updated_at = ?
            WHERE id = ? AND is_deleted = 0
            "#,
        )
        .bind(name)
        .bind(parent_id)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_file_location(
        &self,
        id: &str,
        name: &str,
        folder_id: Option<&str>,
    ) -> RepositoryResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            UPDATE files
            SET name = ?, folder_id = ?, updated_at = ?
            WHERE id = ? AND is_deleted = 0
            "#,
        )
        .bind(name)
        .bind(folder_id)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn restore_file(&self, id: &str) -> RepositoryResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query!(
            r#"UPDATE files SET is_deleted = 0, updated_at = ? WHERE id = ? AND is_deleted = 1"#,
            now,
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn restore_folder_tree(&self, folder_id: &str) -> RepositoryResult<()> {
        self.restore_folder_tree_inner(folder_id).await
    }

    async fn list_trash_items(
        &self,
        crew_id: Option<&str>,
    ) -> RepositoryResult<Vec<ListItem>> {
        let folders = self.list_deleted_folders_in_crew(crew_id).await?;
        let files = self.list_deleted_files_in_crew(crew_id).await?;
        Ok(Self::merge_list_items(&folders, &files))
    }

    async fn list_file_ids_in_folder_tree(&self, folder_id: &str) -> RepositoryResult<Vec<String>> {
        let sql = format!(
            r#"
            {SUB_FOLDERS_CTE}
            SELECT f.id
            FROM files f
            WHERE f.folder_id IN (SELECT id FROM sub_folders)
            "#
        );
        let rows = sqlx::query_scalar::<_, String>(&sql)
            .bind(folder_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn permanent_delete_folder_tree(&self, folder_id: &str) -> RepositoryResult<()> {
        self.purge_folder_tree(folder_id).await
    }

    async fn search_files_by_name(
        &self,
        query: &str,
        limit: i32,
    ) -> RepositoryResult<Vec<(ObjectMetadata, Option<String>)>> {
        let pattern = format!("%{}%", query.replace('%', r"\%").replace('_', r"\_"));
        let rows = sqlx::query!(
            r#"
            SELECT
                f.id as "id!",
                f.folder_id,
                f.name as "name!",
                f.size as "size!",
                f.file_type,
                f.checksum,
                f.version as "version!",
                f.is_deleted as "is_deleted!",
                f.created_at as "created_at!",
                f.updated_at as "updated_at!",
                fo.crew_id as "crew_id?"
            FROM files f
            LEFT JOIN folders fo ON f.folder_id = fo.id
            WHERE f.is_deleted = 0 AND f.name LIKE ? ESCAPE '\'
            ORDER BY f.name
            LIMIT ?
            "#,
            pattern,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    Self::map_file(
                        r.id,
                        r.folder_id,
                        r.name,
                        r.size,
                        r.file_type,
                        r.checksum,
                        r.version,
                        r.is_deleted,
                        r.created_at,
                        r.updated_at,
                    ),
                    r.crew_id,
                )
            })
            .collect())
    }

    async fn folder_display_path(&self, folder_id: &str) -> RepositoryResult<String> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"
            WITH RECURSIVE ancestors(name, parent_id, depth) AS (
                SELECT name, parent_id, 0 FROM folders WHERE id = ?
                UNION ALL
                SELECT f.name, f.parent_id, a.depth + 1
                FROM folders f
                JOIN ancestors a ON f.id = a.parent_id
            )
            SELECT name FROM ancestors ORDER BY depth DESC
            "#,
        )
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(if rows.is_empty() {
            "/".to_string()
        } else {
            rows.join(" / ")
        })
    }

    async fn list_subtitle_siblings_for_video(
        &self,
        video_id: &str,
    ) -> RepositoryResult<Vec<ObjectMetadata>> {
        let Some(video) = self.find_file(video_id, true).await? else {
            return Ok(vec![]);
        };
        let siblings = self.list_files_by_folder(video.folder_id.as_deref()).await?;
        Ok(siblings
            .into_iter()
            .filter(|f| {
                crate::infrastructure::subtitle::matches_video_subtitle(&video.name, &f.name)
            })
            .collect())
    }

    async fn list_file_ids_by_crew_ids(&self, crew_ids: &[String]) -> RepositoryResult<Vec<String>> {
        if crew_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders = crew_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            r#"
            SELECT f.id
            FROM files f
            INNER JOIN folders fo ON f.folder_id = fo.id
            WHERE fo.crew_id IN ({placeholders})
            "#
        );
        let mut q = sqlx::query_scalar::<_, String>(&sql);
        for id in crew_ids {
            q = q.bind(id);
        }
        Ok(q.fetch_all(&self.pool).await?)
    }

    async fn delete_files_and_folders_by_crew_ids(
        &self,
        crew_ids: &[String],
    ) -> RepositoryResult<()> {
        if crew_ids.is_empty() {
            return Ok(());
        }
        let placeholders = crew_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let delete_files = format!(
            r#"
            DELETE FROM files
            WHERE folder_id IN (SELECT id FROM folders WHERE crew_id IN ({placeholders}))
            "#
        );
        let mut fq = sqlx::query(&delete_files);
        for id in crew_ids {
            fq = fq.bind(id);
        }
        fq.execute(&self.pool).await?;

        let delete_folders = format!("DELETE FROM folders WHERE crew_id IN ({placeholders})");
        let mut dfq = sqlx::query(&delete_folders);
        for id in crew_ids {
            dfq = dfq.bind(id);
        }
        dfq.execute(&self.pool).await?;
        Ok(())
    }
}
