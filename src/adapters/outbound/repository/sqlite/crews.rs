use crate::domain::errors::RepositoryResult;
use crate::domain::models::{
    Crew, CrewMemberView, CrewMembership, CrewVisibility, User, GLOBAL_ROOT_CREW_ID,
};
use crate::domain::ports::CrewRepositoryPort;
use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};

pub struct SqliteCrewRepository {
    pool: Pool<Sqlite>,
}

impl SqliteCrewRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    fn map_crew(row: &sqlx::sqlite::SqliteRow) -> Crew {
        Crew {
            id: row.get("id"),
            name: row.get("name"),
            parent_id: row.get("parent_id"),
            depth: row.get("depth"),
            visibility: CrewVisibility::from_i32(row.get::<i32, _>("visibility")),
            max_sub_crew_depth: row.get("max_sub_crew_depth"),
            root_folder_id: row.get("root_folder_id"),
            created_at: row.get("created_at"),
        }
    }
}

#[async_trait]
impl CrewRepositoryPort for SqliteCrewRepository {
    async fn count_crew_members(&self, crew_id: &str) -> RepositoryResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM crew_user WHERE crew_id = ?")
            .bind(crew_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    async fn create_user(&self, username: &str, password_hash: &str) -> RepositoryResult<i64> {
        let new_user_id: i64 = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash) VALUES (?, ?) RETURNING id",
        )
        .bind(username)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok(new_user_id)
    }

    async fn find_user_by_username(&self, username: &str) -> RepositoryResult<Option<User>> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, created_at FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            username: r.get("username"),
            password_hash: r.get("password_hash"),
            created_at: r.get("created_at"),
        }))
    }

    async fn find_user_by_id(&self, user_id: i64) -> RepositoryResult<Option<User>> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, created_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| User {
            id: r.get("id"),
            username: r.get("username"),
            password_hash: r.get("password_hash"),
            created_at: r.get("created_at"),
        }))
    }

    async fn update_user_password(
        &self,
        user_id: i64,
        password_hash: &str,
    ) -> RepositoryResult<()> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_descendant_crew_ids(&self, crew_id: &str) -> RepositoryResult<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"
            WITH RECURSIVE subtree(id) AS (
                SELECT id FROM crews WHERE id = ?
                UNION ALL
                SELECT c.id FROM crews c JOIN subtree s ON c.parent_id = s.id
            )
            SELECT id FROM subtree
            "#,
        )
        .bind(crew_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn list_all_non_global_crews(&self) -> RepositoryResult<Vec<Crew>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, parent_id, depth, visibility,
                   max_sub_crew_depth, root_folder_id, created_at
            FROM crews
            WHERE id != ?
            ORDER BY depth, name
            "#,
        )
        .bind(GLOBAL_ROOT_CREW_ID)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Self::map_crew).collect())
    }

    async fn delete_crews_by_ids(&self, crew_ids: &[String]) -> RepositoryResult<()> {
        if crew_ids.is_empty() {
            return Ok(());
        }
        let placeholders = crew_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT id FROM crews WHERE id IN ({placeholders}) ORDER BY depth DESC"
        );
        let mut q = sqlx::query_scalar::<_, String>(&sql);
        for id in crew_ids {
            q = q.bind(id);
        }
        let ordered = q.fetch_all(&self.pool).await?;
        for id in ordered {
            sqlx::query("DELETE FROM crews WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn delete_crew_by_id(&self, crew_id: &str) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM crews WHERE id = ?")
            .bind(crew_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_owned_crews(&self, user_id: i64) -> RepositoryResult<Vec<Crew>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.name, c.parent_id, c.depth, c.visibility,
                   c.max_sub_crew_depth, c.root_folder_id, c.created_at
            FROM crews c
            INNER JOIN crew_user cu ON cu.crew_id = c.id
            WHERE cu.user_id = ? AND cu.role = 0 AND cu.status = 1
              AND c.id != ? AND c.root_folder_id IS NOT NULL
            ORDER BY c.name
            "#,
        )
        .bind(user_id)
        .bind(GLOBAL_ROOT_CREW_ID)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Self::map_crew).collect())
    }

    async fn add_crew_member(
        &self,
        user_id: i64,
        crew_id: &str,
        role: u8,
        status: u8,
    ) -> RepositoryResult<()> {
        sqlx::query("INSERT INTO crew_user (user_id, crew_id, role, status) VALUES (?, ?, ?, ?)")
            .bind(user_id)
            .bind(crew_id)
            .bind(role)
            .bind(status)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_crew_member(
        &self,
        user_id: i64,
        crew_id: &str,
        role: u8,
        status: u8,
    ) -> RepositoryResult<()> {
        sqlx::query(
            "UPDATE crew_user SET role = ?, status = ? WHERE user_id = ? AND crew_id = ?",
        )
        .bind(role)
        .bind(status)
        .bind(user_id)
        .bind(crew_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_membership(
        &self,
        user_id: i64,
        crew_id: &str,
    ) -> RepositoryResult<Option<CrewMembership>> {
        let row = sqlx::query(
            "SELECT user_id, crew_id, role, status FROM crew_user WHERE user_id = ? AND crew_id = ?",
        )
        .bind(user_id)
        .bind(crew_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| CrewMembership {
            user_id: r.get("user_id"),
            crew_id: r.get("crew_id"),
            role: r.get("role"),
            status: r.get("status"),
        }))
    }

    async fn find_crew_by_id(&self, crew_id: &str) -> RepositoryResult<Option<Crew>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, parent_id, depth, visibility,
                   max_sub_crew_depth, root_folder_id, created_at
            FROM crews WHERE id = ?
            "#,
        )
        .bind(crew_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(Self::map_crew))
    }

    async fn insert_crew(&self, crew: &Crew) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            INSERT INTO crews (
                id, name, parent_id, depth, access_level, visibility,
                max_sub_crew_depth, root_folder_id, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&crew.id)
        .bind(&crew.name)
        .bind(&crew.parent_id)
        .bind(crew.depth)
        .bind(crew.visibility.as_i32())
        .bind(crew.visibility.as_i32())
        .bind(crew.max_sub_crew_depth)
        .bind(&crew.root_folder_id)
        .bind(&crew.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_crew_root_folder(
        &self,
        crew_id: &str,
        root_folder_id: &str,
    ) -> RepositoryResult<()> {
        sqlx::query("UPDATE crews SET root_folder_id = ? WHERE id = ?")
            .bind(root_folder_id)
            .bind(crew_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_crew_settings(
        &self,
        crew_id: &str,
        max_sub_crew_depth: i32,
        visibility: i32,
    ) -> RepositoryResult<()> {
        sqlx::query(
            "UPDATE crews SET max_sub_crew_depth = ?, visibility = ?, access_level = ? WHERE id = ?",
        )
        .bind(max_sub_crew_depth)
        .bind(visibility)
        .bind(visibility)
        .bind(crew_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_public_crews(&self) -> RepositoryResult<Vec<Crew>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, parent_id, depth, visibility,
                   max_sub_crew_depth, root_folder_id, created_at
            FROM crews
            WHERE visibility = 0 AND id != ?
            ORDER BY name
            "#,
        )
        .bind(GLOBAL_ROOT_CREW_ID)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Self::map_crew).collect())
    }

    async fn list_crews_for_user(&self, user_id: i64) -> RepositoryResult<Vec<Crew>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.name, c.parent_id, c.depth, c.visibility,
                   c.max_sub_crew_depth, c.root_folder_id, c.created_at
            FROM crews c
            INNER JOIN crew_user cu ON cu.crew_id = c.id
            WHERE cu.user_id = ? AND cu.status = 1
            ORDER BY c.depth, c.name
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(Self::map_crew).collect())
    }

    async fn list_manageable_crews(&self, user_id: i64) -> RepositoryResult<Vec<(Crew, u8)>> {
        let rows = sqlx::query(
            r#"
            SELECT c.id, c.name, c.parent_id, c.depth, c.visibility,
                   c.max_sub_crew_depth, c.root_folder_id, c.created_at, cu.role AS my_role
            FROM crews c
            INNER JOIN crew_user cu ON cu.crew_id = c.id
            WHERE cu.user_id = ? AND cu.status = 1 AND cu.role IN (0, 1)
              AND c.id != ? AND c.root_folder_id IS NOT NULL
            ORDER BY c.name
            "#,
        )
        .bind(user_id)
        .bind(GLOBAL_ROOT_CREW_ID)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| (Self::map_crew(r), r.get::<i64, _>("my_role") as u8))
            .collect())
    }

    async fn list_crew_members(&self, crew_id: &str) -> RepositoryResult<Vec<CrewMemberView>> {
        let rows = sqlx::query(
            r#"
            SELECT cu.user_id, u.username, cu.role, cu.status
            FROM crew_user cu
            INNER JOIN users u ON u.id = cu.user_id
            WHERE cu.crew_id = ?
            ORDER BY cu.status, cu.role, u.username
            "#,
        )
        .bind(crew_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| CrewMemberView {
                user_id: r.get("user_id"),
                username: r.get("username"),
                role: r.get::<i64, _>("role") as u8,
                status: r.get::<i64, _>("status") as u8,
            })
            .collect())
    }

    async fn shift_descendant_max_depths(&self, crew_id: &str, delta: i32) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            WITH RECURSIVE descendants(id) AS (
                SELECT id FROM crews WHERE parent_id = ?
                UNION ALL
                SELECT c.id FROM crews c JOIN descendants d ON c.parent_id = d.id
            )
            UPDATE crews
            SET max_sub_crew_depth = MAX(0, max_sub_crew_depth + ?)
            WHERE id IN (SELECT id FROM descendants)
            "#,
        )
        .bind(crew_id)
        .bind(delta)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn ensure_global_root_crew(&self) -> RepositoryResult<()> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO crews (
                id, name, parent_id, depth, access_level, visibility,
                max_sub_crew_depth, root_folder_id, created_at
            ) VALUES (?, 'Global', NULL, 0, 0, 0, 10, NULL, datetime('now'))
            "#,
        )
        .bind(GLOBAL_ROOT_CREW_ID)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
