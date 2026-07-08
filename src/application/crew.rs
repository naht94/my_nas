use crate::domain::errors::NasError;
use crate::domain::models::{
    Crew, CrewGuestView, CrewListEntry, CrewMemberView, CrewVisibility, DeletableCrew,
    FolderMetadata, ManageableCrew, Role, Status, WebDavMountInfo, DEFAULT_MAX_SUB_CREW_DEPTH,
    GLOBAL_ROOT_CREW_ID,
};
use std::collections::BTreeMap;
use chrono::Utc;
use uuid::Uuid;

use super::service::NasService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderAccess {
    Open,
    Read,
    Write,
    GuestMetaOnly,
    Denied,
}

impl NasService {
    pub async fn login_user(&self, username: &str, password: &str) -> Result<(i64, String), NasError> {
        let user = self
            .crew_repository
            .find_user_by_username(username)
            .await?
            .ok_or_else(|| {
                NasError::Forbidden("아이디 또는 비밀번호가 올바르지 않습니다.".into())
            })?;

        if !crate::infrastructure::crypto::verify_password(password, &user.password_hash) {
            return Err(NasError::Forbidden(
                "아이디 또는 비밀번호가 올바르지 않습니다.".into(),
            ));
        }

        Ok((user.id, user.username))
    }

    pub async fn authorize_webdav_crew(&self, user_id: i64, crew_id: &str) -> Result<Crew, NasError> {
        if crew_id == GLOBAL_ROOT_CREW_ID {
            return Err(NasError::Forbidden(
                "글로벌 루트 Crew는 WebDAV 마운트 대상이 아닙니다.".into(),
            ));
        }

        self.require_crew_role(user_id, crew_id, |r| r == Role::Owner)
            .await?;

        let crew = self
            .crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        if crew.root_folder_id.is_none() {
            return Err(NasError::BadRequest(
                "Crew 루트 폴더가 설정되지 않았습니다.".into(),
            ));
        }

        Ok(crew)
    }

    pub async fn list_webdav_mounts(
        &self,
        user_id: i64,
        webdav_base: &str,
    ) -> Result<Vec<WebDavMountInfo>, NasError> {
        let crews = self.crew_repository.list_owned_crews(user_id).await?;
        let base = webdav_base.trim_end_matches('/');

        Ok(crews
            .into_iter()
            .map(|c| WebDavMountInfo {
                crew_id: c.id.clone(),
                crew_name: c.name,
                mount_path: format!("{}/{}/", base, c.id),
            })
            .collect())
    }

    pub async fn ensure_global_root(&self) -> Result<(), NasError> {
        self.crew_repository
            .ensure_global_root_crew()
            .await
            .map_err(|e| NasError::Internal(e.to_string()))
    }

    pub async fn create_crew(
        &self,
        actor_id: i64,
        parent_crew_id: &str,
        name: &str,
        visibility: CrewVisibility,
        requested_max_sub_depth: Option<i32>,
    ) -> Result<Crew, NasError> {
        let parent = self
            .crew_repository
            .find_crew_by_id(parent_crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        // 최상위(글로벌 루트 하위) Crew는 활성 멤버 누구나 만들 수 있게 한다.
        // (글로벌 루트의 일반 멤버도 자신의 Crew를 만들 수 있어야 하므로)
        // 그 외 하위 Crew는 기존대로 Owner/Manager만 생성 가능하다.
        if parent_crew_id == GLOBAL_ROOT_CREW_ID {
            self.require_crew_role(actor_id, parent_crew_id, |_| true)
                .await?;
        } else {
            self.require_crew_role(actor_id, parent_crew_id, |r| r.can_create_sub_crew())
                .await?;
        }

        if parent.max_sub_crew_depth < 1 {
            return Err(NasError::BadRequest(
                "이 Crew는 더 이상 하위 Crew를 만들 수 없습니다.".into(),
            ));
        }

        let child_max = requested_max_sub_depth
            .unwrap_or(DEFAULT_MAX_SUB_CREW_DEPTH)
            .clamp(0, parent.max_sub_crew_depth.saturating_sub(1));

        let crew_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let root_folder_id = Uuid::new_v4().to_string();

        let root_folder = FolderMetadata {
            id: root_folder_id.clone(),
            parent_id: None,
            crew_id: Some(crew_id.clone()),
            name: name.to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        let crew = Crew {
            id: crew_id.clone(),
            name: name.to_string(),
            parent_id: Some(parent_crew_id.to_string()),
            depth: parent.depth + 1,
            visibility,
            max_sub_crew_depth: child_max,
            root_folder_id: Some(root_folder_id.clone()),
            created_at: now,
        };

        self.repository.save_folder(root_folder).await?;
        self.crew_repository.insert_crew(&crew).await?;
        self.crew_repository
            .update_crew_root_folder(&crew_id, &root_folder_id)
            .await?;
        self.crew_repository
            .add_crew_member(actor_id, &crew_id, Role::Owner as u8, Status::Active as u8)
            .await?;

        Ok(crew)
    }

    pub async fn update_crew_settings(
        &self,
        actor_id: i64,
        crew_id: &str,
        max_sub_crew_depth: Option<i32>,
        visibility: Option<CrewVisibility>,
    ) -> Result<Crew, NasError> {
        // Owner만 설정을 변경할 수 있다. 글로벌 루트도 Owner(최초 가입자)면 허용한다.
        self.require_crew_role(actor_id, crew_id, |r| r.can_manage_crew_settings())
            .await?;

        let crew = self
            .crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let is_global = crew_id == GLOBAL_ROOT_CREW_ID;

        // 글로벌 루트(전역 홈)는 항상 공개 성격이므로 공개 범위 변경을 막는다.
        if is_global {
            if let Some(v) = visibility {
                if v != crew.visibility {
                    return Err(NasError::BadRequest(
                        "글로벌 루트 Crew의 공개 범위는 변경할 수 없습니다.".into(),
                    ));
                }
            }
        }

        let new_max = max_sub_crew_depth.unwrap_or(crew.max_sub_crew_depth).max(0);
        let new_visibility = if is_global {
            crew.visibility
        } else {
            visibility.unwrap_or(crew.visibility)
        };

        // depth 제약을 늘리거나 줄이면, 그 차이만큼 모든 하위 Crew의 제약도 함께 조정한다.
        // (상대적인 depth 예산을 보존하여 더 깊은 중첩이 가능/불가능해지도록)
        let delta = new_max - crew.max_sub_crew_depth;

        self.crew_repository
            .update_crew_settings(crew_id, new_max, new_visibility.as_i32())
            .await?;

        if delta != 0 {
            self.crew_repository
                .shift_descendant_max_depths(crew_id, delta)
                .await?;
        }

        self.crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)
    }

    /// Owner만 조회 가능한 Crew 설정(현재 값). 설정 화면 초기 표시에 사용.
    pub async fn get_crew_settings(&self, actor_id: i64, crew_id: &str) -> Result<Crew, NasError> {
        self.require_crew_role(actor_id, crew_id, |r| r.can_manage_crew_settings())
            .await?;
        self.crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)
    }

    /// 글로벌 루트 Crew에서의 멤버 상태. 가입 승인 대기 여부 판단용.
    pub async fn global_membership_status(
        &self,
        user_id: i64,
    ) -> Result<Option<Status>, NasError> {
        let membership = self
            .crew_repository
            .find_membership(user_id, GLOBAL_ROOT_CREW_ID)
            .await?;
        Ok(membership.and_then(|m| Status::from_u8(m.status)))
    }

    /// 현재 사용자가 관리(Owner/Manager)하는 Crew 목록.
    pub async fn list_manageable_crews(
        &self,
        user_id: i64,
    ) -> Result<Vec<ManageableCrew>, NasError> {
        let rows = self.crew_repository.list_manageable_crews(user_id).await?;
        Ok(rows
            .into_iter()
            .map(|(c, my_role)| ManageableCrew {
                id: c.id,
                name: c.name,
                visibility: c.visibility,
                root_folder_id: c.root_folder_id,
                max_sub_crew_depth: c.max_sub_crew_depth,
                my_role,
            })
            .collect())
    }

    /// Crew 멤버 목록. 멤버 관리 권한(Owner/Manager)이 있어야 조회 가능.
    pub async fn list_crew_members(
        &self,
        actor_id: i64,
        crew_id: &str,
    ) -> Result<Vec<CrewMemberView>, NasError> {
        self.require_crew_role(actor_id, crew_id, |r| r.can_manage_members())
            .await?;
        Ok(self.crew_repository.list_crew_members(crew_id).await?)
    }

    /// 아이디(username)로 사용자를 찾아 Crew에 초대한다.
    pub async fn invite_to_crew_by_username(
        &self,
        actor_id: i64,
        crew_id: &str,
        username: &str,
        role: Role,
    ) -> Result<(), NasError> {
        let user = self
            .crew_repository
            .find_user_by_username(username.trim())
            .await?
            .ok_or_else(|| NasError::BadRequest("해당 아이디의 사용자를 찾을 수 없습니다.".into()))?;
        self.invite_to_crew(actor_id, crew_id, user.id, role).await
    }

    /// 현재 사용자의 폴더 접근 권한을 (읽기, 쓰기)로 요약한다. UI 버튼 노출 판단용.
    pub async fn describe_folder_access(
        &self,
        user_id: Option<i64>,
        folder_id: Option<&str>,
    ) -> (bool, bool) {
        let can_read = matches!(
            self.resolve_folder_access(user_id, folder_id, false).await,
            Ok(FolderAccess::Open) | Ok(FolderAccess::Read) | Ok(FolderAccess::Write)
        );
        let can_write = matches!(
            self.resolve_folder_access(user_id, folder_id, true).await,
            Ok(FolderAccess::Open) | Ok(FolderAccess::Write)
        );
        (can_read, can_write)
    }

    pub async fn list_my_crews(&self, user_id: i64) -> Result<Vec<Crew>, NasError> {
        Ok(self.crew_repository.list_crews_for_user(user_id).await?)
    }

    pub async fn list_discoverable_crews(&self, user_id: i64) -> Result<Vec<Crew>, NasError> {
        let public = self.crew_repository.list_public_crews().await?;
        let mine: std::collections::HashSet<String> = self
            .crew_repository
            .list_crews_for_user(user_id)
            .await?
            .into_iter()
            .map(|c| c.id)
            .collect();

        Ok(public
            .into_iter()
            .filter(|c| !mine.contains(&c.id))
            .collect())
    }

    /// 홈 화면에 함께 노출할 Crew 목록.
    /// - 공개 Crew: 비로그인 포함 누구에게나 노출 (글로벌 루트 제외)
    /// - 비공개 Crew: 활성 멤버인 사용자에게만 노출
    /// 멤버이면서 동시에 공개인 경우 멤버 항목으로 합쳐진다.
    pub async fn list_home_crews(
        &self,
        user_id: Option<i64>,
    ) -> Result<Vec<CrewListEntry>, NasError> {
        let mut by_id: BTreeMap<String, CrewListEntry> = BTreeMap::new();

        for c in self.crew_repository.list_public_crews().await? {
            if c.root_folder_id.is_none() {
                continue;
            }
            by_id.insert(
                c.id.clone(),
                CrewListEntry {
                    id: c.id,
                    name: c.name,
                    visibility: c.visibility,
                    root_folder_id: c.root_folder_id,
                    is_member: false,
                },
            );
        }

        if let Some(uid) = user_id {
            for c in self.crew_repository.list_crews_for_user(uid).await? {
                if c.id == GLOBAL_ROOT_CREW_ID || c.root_folder_id.is_none() {
                    continue;
                }
                by_id.insert(
                    c.id.clone(),
                    CrewListEntry {
                        id: c.id,
                        name: c.name,
                        visibility: c.visibility,
                        root_folder_id: c.root_folder_id,
                        is_member: true,
                    },
                );
            }
        }

        Ok(by_id.into_values().collect())
    }

    pub async fn get_crew_guest_view(
        &self,
        viewer_id: Option<i64>,
        crew_id: &str,
    ) -> Result<CrewGuestView, NasError> {
        let crew = self
            .crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        if crew_id == GLOBAL_ROOT_CREW_ID {
            return Ok(CrewGuestView {
                id: crew.id,
                name: crew.name,
                visibility: crew.visibility,
                root_folder_id: crew.root_folder_id,
                root_folder_name: None,
            });
        }

        if let Some(uid) = viewer_id {
            if let Some(m) = self.crew_repository.find_membership(uid, crew_id).await? {
                if Status::from_u8(m.status).is_some_and(|s| s.is_active()) {
                    let root_name = if let Some(ref fid) = crew.root_folder_id {
                        self.repository
                            .find_folder_by_id(fid)
                            .await?
                            .map(|f| f.name)
                    } else {
                        None
                    };
                    return Ok(CrewGuestView {
                        id: crew.id,
                        name: crew.name,
                        visibility: crew.visibility,
                        root_folder_id: crew.root_folder_id,
                        root_folder_name: root_name,
                    });
                }
            }
        }

        if crew.visibility == CrewVisibility::Private {
            return Err(NasError::DataNotFound);
        }

        let root_name = if let Some(ref fid) = crew.root_folder_id {
            self.repository
                .find_folder_by_id(fid)
                .await?
                .map(|f| f.name)
        } else {
            None
        };

        Ok(CrewGuestView {
            id: crew.id,
            name: crew.name,
            visibility: crew.visibility,
            root_folder_id: crew.root_folder_id,
            root_folder_name: root_name,
        })
    }

    pub async fn request_join_public_crew(
        &self,
        user_id: i64,
        crew_id: &str,
    ) -> Result<(), NasError> {
        if crew_id == GLOBAL_ROOT_CREW_ID {
            return Err(NasError::BadRequest(
                "글로벌 루트 Crew는 가입 신청이 필요 없습니다.".into(),
            ));
        }

        let crew = self
            .crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        if crew.visibility != CrewVisibility::Public {
            return Err(NasError::Forbidden(
                "비공개 Crew는 가입 신청할 수 없습니다. 초대를 받아야 합니다.".into(),
            ));
        }

        if self
            .crew_repository
            .find_membership(user_id, crew_id)
            .await?
            .is_some()
        {
            return Err(NasError::BadRequest("이미 신청했거나 멤버입니다.".into()));
        }

        self.crew_repository
            .add_crew_member(user_id, crew_id, Role::Guest as u8, Status::Pending as u8)
            .await?;

        Ok(())
    }

    pub async fn invite_to_crew(
        &self,
        actor_id: i64,
        crew_id: &str,
        target_user_id: i64,
        role: Role,
    ) -> Result<(), NasError> {
        if crew_id == GLOBAL_ROOT_CREW_ID {
            return Err(NasError::BadRequest(
                "글로벌 루트 Crew에는 초대할 수 없습니다.".into(),
            ));
        }

        self.require_crew_role(actor_id, crew_id, |r| r.can_manage_members())
            .await?;

        let crew = self
            .crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        if crew.visibility == CrewVisibility::Public && role == Role::Guest {
            return Err(NasError::BadRequest(
                "공개 Crew 초대 시 Guest 역할은 사용할 수 없습니다.".into(),
            ));
        }

        let member_role = match role {
            Role::Guest => Role::Member,
            other => other,
        };

        if let Some(existing) = self
            .crew_repository
            .find_membership(target_user_id, crew_id)
            .await?
        {
            if Status::from_u8(existing.status).is_some_and(|s| s.is_active()) {
                return Err(NasError::BadRequest("이미 활성 멤버입니다.".into()));
            }
            self.crew_repository
                .update_crew_member(
                    target_user_id,
                    crew_id,
                    member_role as u8,
                    Status::Invited as u8,
                )
                .await?;
        } else {
            self.crew_repository
                .add_crew_member(
                    target_user_id,
                    crew_id,
                    member_role as u8,
                    Status::Invited as u8,
                )
                .await?;
        }

        Ok(())
    }

    pub async fn approve_membership(
        &self,
        actor_id: i64,
        crew_id: &str,
        target_user_id: i64,
    ) -> Result<(), NasError> {
        self.require_crew_role(actor_id, crew_id, |r| r.can_manage_members())
            .await?;

        let membership = self
            .crew_repository
            .find_membership(target_user_id, crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let status = Status::from_u8(membership.status).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 상태입니다.".into())
        })?;

        let new_role = match status {
            Status::Pending => Role::Member as u8,
            Status::Invited => membership.role,
            _ => {
                return Err(NasError::BadRequest(
                    "승인할 수 없는 멤버 상태입니다.".into(),
                ));
            }
        };

        self.crew_repository
            .update_crew_member(target_user_id, crew_id, new_role, Status::Active as u8)
            .await?;

        Ok(())
    }

    pub async fn resolve_folder_access(
        &self,
        user_id: Option<i64>,
        folder_id: Option<&str>,
        need_write: bool,
    ) -> Result<FolderAccess, NasError> {
        if let Some(uid) = user_id {
            if let Some(status) = self.global_membership_status(uid).await? {
                if status == Status::Banned {
                    return Err(NasError::Forbidden("차단된 멤버입니다.".into()));
                }
                if !status.is_active() {
                    return Err(NasError::Forbidden("가입 승인 대기 중입니다.".into()));
                }
            }
        }

        let Some(fid) = folder_id else {
            return Ok(FolderAccess::Open);
        };

        let folder = self
            .repository
            .find_folder_by_id(fid)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let Some(crew_id) = folder.crew_id.as_deref() else {
            return Ok(FolderAccess::Open);
        };

        if crew_id == GLOBAL_ROOT_CREW_ID {
            return Ok(if need_write {
                FolderAccess::Write
            } else {
                FolderAccess::Read
            });
        }

        let crew = self
            .crew_repository
            .find_crew_by_id(crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let Some(uid) = user_id else {
            if crew.visibility == CrewVisibility::Public
                && crew.root_folder_id.as_deref() == Some(fid)
            {
                return Ok(FolderAccess::GuestMetaOnly);
            }
            return Err(NasError::DataNotFound);
        };

        let Some(membership) = self.crew_repository.find_membership(uid, crew_id).await? else {
            if crew.visibility == CrewVisibility::Public
                && crew.root_folder_id.as_deref() == Some(fid)
            {
                return Ok(FolderAccess::GuestMetaOnly);
            }
            return Err(NasError::DataNotFound);
        };

        let status = Status::from_u8(membership.status).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 상태입니다.".into())
        })?;

        if status == Status::Banned {
            return Err(NasError::Forbidden("차단된 멤버입니다.".into()));
        }

        if !status.is_active() {
            if crew.visibility == CrewVisibility::Public
                && crew.root_folder_id.as_deref() == Some(fid)
                && membership.role == Role::Guest as u8
            {
                return Ok(FolderAccess::GuestMetaOnly);
            }
            return Err(NasError::DataNotFound);
        }

        let role = Role::from_u8(membership.role).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 역할입니다.".into())
        })?;

        if need_write {
            if role.can_write() {
                Ok(FolderAccess::Write)
            } else {
                Err(NasError::Forbidden("쓰기 권한이 없습니다.".into()))
            }
        } else if role.can_read() {
            Ok(FolderAccess::Read)
        } else if role == Role::Guest
            && crew.root_folder_id.as_deref() == Some(fid)
        {
            Ok(FolderAccess::GuestMetaOnly)
        } else {
            Err(NasError::Forbidden("읽기 권한이 없습니다.".into()))
        }
    }

    pub async fn change_password(
        &self,
        user_id: i64,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), NasError> {
        if new_password.len() < 8 {
            return Err(NasError::BadRequest(
                "새 비밀번호는 8자 이상이어야 합니다.".into(),
            ));
        }

        let user = self
            .crew_repository
            .find_user_by_id(user_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        if !crate::infrastructure::crypto::verify_password(current_password, &user.password_hash) {
            return Err(NasError::Forbidden(
                "현재 비밀번호가 올바르지 않습니다.".into(),
            ));
        }

        let password_hash = crate::infrastructure::crypto::hash_password(new_password)
            .map_err(|e| NasError::Internal(format!("비밀번호 해싱 실패: {e}")))?;

        self.crew_repository
            .update_user_password(user_id, &password_hash)
            .await?;

        Ok(())
    }

    pub async fn ban_crew_member(
        &self,
        actor_id: i64,
        crew_id: &str,
        target_user_id: i64,
    ) -> Result<(), NasError> {
        if target_user_id == actor_id {
            return Err(NasError::BadRequest("자기 자신을 차단할 수 없습니다.".into()));
        }

        let actor_role = self
            .require_crew_role(actor_id, crew_id, |r| r.can_manage_members())
            .await?;

        let membership = self
            .crew_repository
            .find_membership(target_user_id, crew_id)
            .await?
            .ok_or(NasError::DataNotFound)?;

        let target_role = Role::from_u8(membership.role).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 역할입니다.".into())
        })?;

        if target_role == Role::Owner {
            return Err(NasError::Forbidden("Owner는 차단할 수 없습니다.".into()));
        }

        if actor_role == Role::Manager && target_role == Role::Manager {
            return Err(NasError::Forbidden(
                "Manager는 다른 Manager를 차단할 수 없습니다.".into(),
            ));
        }

        self.crew_repository
            .update_crew_member(
                target_user_id,
                crew_id,
                membership.role,
                Status::Banned as u8,
            )
            .await?;

        Ok(())
    }

    pub async fn delete_crew(&self, actor_id: i64, crew_id: &str) -> Result<(), NasError> {
        self.require_crew_delete_authority(actor_id, crew_id).await?;

        let crew_ids = self.crew_repository.list_descendant_crew_ids(crew_id).await?;
        let file_ids = self
            .repository
            .list_file_ids_by_crew_ids(&crew_ids)
            .await?;

        for file_id in &file_ids {
            if let Err(e) = self.storage.delete_file(file_id).await {
                tracing::error!("Crew 삭제 중 물리 파일 삭제 실패 (id: {}): {:?}", file_id, e);
            }
        }

        self.repository
            .delete_files_and_folders_by_crew_ids(&crew_ids)
            .await?;
        self.crew_repository.delete_crews_by_ids(&crew_ids).await?;

        Ok(())
    }

    /// 해당 Crew 또는 조상 Crew 중 하나라도 활성 Owner이면 삭제 가능.
    pub(crate) async fn require_crew_delete_authority(
        &self,
        actor_id: i64,
        crew_id: &str,
    ) -> Result<(), NasError> {
        if crew_id == GLOBAL_ROOT_CREW_ID {
            return Err(NasError::Forbidden(
                "글로벌 루트 Crew는 삭제할 수 없습니다.".into(),
            ));
        }

        let mut current = crew_id.to_string();
        loop {
            if self.is_active_crew_owner(actor_id, &current).await? {
                return Ok(());
            }

            let crew = self
                .crew_repository
                .find_crew_by_id(&current)
                .await?
                .ok_or(NasError::DataNotFound)?;

            match crew.parent_id {
                Some(parent) => current = parent,
                None => break,
            }
        }

        Err(NasError::Forbidden(
            "해당 Crew 또는 상위 Crew의 Owner만 삭제할 수 있습니다.".into(),
        ))
    }

    async fn is_active_crew_owner(&self, user_id: i64, crew_id: &str) -> Result<bool, NasError> {
        let Some(membership) = self
            .crew_repository
            .find_membership(user_id, crew_id)
            .await?
        else {
            return Ok(false);
        };

        let status = Status::from_u8(membership.status).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 상태입니다.".into())
        })?;
        if !status.is_active() {
            return Ok(false);
        }

        let role = Role::from_u8(membership.role).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 역할입니다.".into())
        })?;
        Ok(role == Role::Owner)
    }

    /// 삭제 권한이 있는 Crew 목록 (직접 Owner 또는 상위 Owner로 하위 Crew 포함).
    pub async fn list_deletable_crews(
        &self,
        actor_id: i64,
    ) -> Result<Vec<DeletableCrew>, NasError> {
        let all = self
            .crew_repository
            .list_all_non_global_crews()
            .await
            .map_err(|e| NasError::Internal(e.to_string()))?;

        let mut out = Vec::new();
        for crew in all {
            if self
                .require_crew_delete_authority(actor_id, &crew.id)
                .await
                .is_err()
            {
                continue;
            }
            let is_direct_owner = self.is_active_crew_owner(actor_id, &crew.id).await?;
            out.push(DeletableCrew {
                id: crew.id,
                name: crew.name,
                visibility: crew.visibility,
                parent_id: crew.parent_id,
                is_direct_owner,
            });
        }
        Ok(out)
    }

    pub(crate) async fn require_crew_role<F>(
        &self,
        user_id: i64,
        crew_id: &str,
        check: F,
    ) -> Result<Role, NasError>
    where
        F: FnOnce(Role) -> bool,
    {
        let membership = self
            .crew_repository
            .find_membership(user_id, crew_id)
            .await?
            .ok_or_else(|| NasError::Forbidden("Crew 멤버가 아닙니다.".into()))?;

        let status = Status::from_u8(membership.status).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 상태입니다.".into())
        })?;

        if !status.is_active() {
            return Err(NasError::Forbidden("활성 멤버만 수행할 수 있습니다.".into()));
        }

        let role = Role::from_u8(membership.role).ok_or_else(|| {
            NasError::Internal("알 수 없는 멤버 역할입니다.".into())
        })?;

        if check(role) {
            Ok(role)
        } else {
            Err(NasError::Forbidden("권한이 없습니다.".into()))
        }
    }
}
