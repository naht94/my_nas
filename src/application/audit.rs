use crate::domain::errors::NasError;
use crate::domain::models::{AuditLogEntry, GLOBAL_ROOT_CREW_ID};

use super::service::NasService;

impl NasService {
    /// 감사 로그 기록 실패는 요청 처리를 막지 않는다.
    pub async fn record_audit(
        &self,
        user_id: Option<i64>,
        username: Option<&str>,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        detail: Option<&str>,
        ip_address: Option<&str>,
    ) {
        if let Err(e) = self
            .audit_repository
            .insert(
                user_id,
                username,
                action,
                target_type,
                target_id,
                detail,
                ip_address,
            )
            .await
        {
            tracing::warn!("audit log write failed: {:?}", e);
        }
    }

    /// 글로벌 크루 멤버 관리 권한(Owner/Manager)만 최근 감사 로그를 조회할 수 있다.
    pub async fn list_audit_logs(
        &self,
        actor_id: i64,
        limit: i32,
    ) -> Result<Vec<AuditLogEntry>, NasError> {
        self.require_crew_role(actor_id, GLOBAL_ROOT_CREW_ID, |r| r.can_manage_members())
            .await?;
        self.audit_repository
            .list_recent(limit)
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }
}
