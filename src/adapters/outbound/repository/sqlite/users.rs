use crate::domain::errors::RepositoryResult;
use crate::domain::ports::UsersRepositoryPort;
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};

pub struct SqliteUsersRepository {
    pool: Pool<Sqlite>,
}

impl SqliteUsersRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UsersRepositoryPort for SqliteUsersRepository {
    async fn count_crew_members(&self, crew_id: &str) -> RepositoryResult<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM crew_user WHERE crew_id = ?")
            .bind(crew_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(count)
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
    async fn create_user(&self, username: &str, password_hash: &str) -> RepositoryResult<i64> {
        // SQLite의 `RETURNING id` 문법을 사용하면 방금 생성된 유저의 PK를 바로 가져올 수 있습니다.
        let new_user_id: i64 = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash) VALUES (?, ?) RETURNING id",
        )
        .bind(username)
        .bind(password_hash) // ⚠️ 추후 운영 환경에서는 꼭 해싱된 값을 넣어야 합니다!
        .fetch_one(&self.pool)
        .await?;

        Ok(new_user_id)
    }
}
