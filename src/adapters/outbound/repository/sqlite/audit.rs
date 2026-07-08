use crate::domain::errors::RepositoryResult;
use crate::domain::models::AuditLogEntry;
use crate::domain::ports::AuditRepositoryPort;
use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};

pub struct SqliteAuditRepository {
    pool: Pool<Sqlite>,
}

impl SqliteAuditRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditRepositoryPort for SqliteAuditRepository {
    async fn insert(
        &self,
        user_id: Option<i64>,
        username: Option<&str>,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        detail: Option<&str>,
        ip_address: Option<&str>,
    ) -> RepositoryResult<()> {
        sqlx::query(
            "INSERT INTO audit_logs
             (user_id, username, action, target_type, target_id, detail, ip_address)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(username)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(detail)
        .bind(ip_address)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_recent(&self, limit: i32) -> RepositoryResult<Vec<AuditLogEntry>> {
        let limit = limit.clamp(1, 500);
        let rows = sqlx::query(
            "SELECT id, user_id, username, action, target_type, target_id, detail, ip_address, created_at
             FROM audit_logs ORDER BY id DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AuditLogEntry {
                id: r.get("id"),
                user_id: r.get("user_id"),
                username: r.get("username"),
                action: r.get("action"),
                target_type: r.get("target_type"),
                target_id: r.get("target_id"),
                detail: r.get("detail"),
                ip_address: r.get("ip_address"),
                created_at: r.get("created_at"),
            })
            .collect())
    }
}
