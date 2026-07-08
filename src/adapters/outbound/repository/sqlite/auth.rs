use crate::domain::errors::RepositoryResult;
use crate::domain::models::AppPasswordInfo;
use crate::domain::ports::AuthRepositoryPort;
use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};

pub struct SqliteAuthRepository {
    pool: Pool<Sqlite>,
}

impl SqliteAuthRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthRepositoryPort for SqliteAuthRepository {
    async fn create_session(
        &self,
        token_hash: &str,
        user_id: i64,
        label: Option<&str>,
        created_at: &str,
        expires_at: &str,
    ) -> RepositoryResult<()> {
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, label, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(token_hash)
        .bind(user_id)
        .bind(label)
        .bind(created_at)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_session(&self, token_hash: &str) -> RepositoryResult<Option<(i64, String)>> {
        let row = sqlx::query(
            "SELECT user_id, expires_at FROM sessions WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| (r.get::<i64, _>("user_id"), r.get::<String, _>("expires_at"))))
    }

    async fn delete_session(&self, token_hash: &str) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_expired_sessions(&self, now: &str) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_session_records(
        &self,
        user_id: i64,
    ) -> RepositoryResult<Vec<(String, Option<String>, String, String)>> {
        let rows = sqlx::query(
            "SELECT token_hash, label, created_at, expires_at FROM sessions
             WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| {
                (
                    r.get::<String, _>("token_hash"),
                    r.get::<Option<String>, _>("label"),
                    r.get::<String, _>("created_at"),
                    r.get::<String, _>("expires_at"),
                )
            })
            .collect())
    }

    async fn delete_other_sessions(
        &self,
        user_id: i64,
        keep_token_hash: &str,
    ) -> RepositoryResult<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?")
            .bind(user_id)
            .bind(keep_token_hash)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn create_app_password(
        &self,
        id: &str,
        user_id: i64,
        token_hash: &str,
        label: Option<&str>,
        created_at: &str,
    ) -> RepositoryResult<()> {
        sqlx::query(
            "INSERT INTO app_passwords (id, user_id, token_hash, label, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(user_id)
        .bind(token_hash)
        .bind(label)
        .bind(created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_app_password_user(&self, token_hash: &str) -> RepositoryResult<Option<i64>> {
        let row = sqlx::query("SELECT user_id FROM app_passwords WHERE token_hash = ?")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<i64, _>("user_id")))
    }

    async fn touch_app_password(&self, token_hash: &str, now: &str) -> RepositoryResult<()> {
        sqlx::query("UPDATE app_passwords SET last_used_at = ? WHERE token_hash = ?")
            .bind(now)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_app_passwords(&self, user_id: i64) -> RepositoryResult<Vec<AppPasswordInfo>> {
        let rows = sqlx::query(
            "SELECT id, label, created_at, last_used_at FROM app_passwords
             WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|r| AppPasswordInfo {
                id: r.get("id"),
                label: r.get("label"),
                created_at: r.get("created_at"),
                last_used_at: r.get("last_used_at"),
            })
            .collect())
    }

    async fn delete_app_password(&self, id: &str, user_id: i64) -> RepositoryResult<()> {
        sqlx::query("DELETE FROM app_passwords WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
