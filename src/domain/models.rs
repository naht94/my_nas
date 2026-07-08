use serde::{Deserialize, Serialize};

pub const GLOBAL_ROOT_CREW_ID: &str = "global-root-uuid";

#[derive(Debug, Serialize, Clone)]
pub struct AuditLogEntry {
    pub id: i64,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub detail: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: String,
}
pub const DEFAULT_MAX_SUB_CREW_DEPTH: i32 = 3;

// file 및 folder 구조

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FolderMetadata {
    pub id: String,
    pub parent_id: Option<String>,
    pub crew_id: Option<String>,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

impl FolderMetadata {
    pub fn is_crew_root(&self) -> bool {
        self.parent_id.is_none() && self.crew_id.is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObjectMetadata {
    pub id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub size: u64,
    pub file_type: Option<String>,
    pub checksum: Option<String>,
    pub version: i32,
    pub is_deleted: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListItem {
    pub id: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub created_at: String,
    pub file_type: Option<String>,
    pub has_children: Option<bool>,
    pub preview_url: Option<String>,
    /// 검색 결과 등에서 표시할 폴더 경로 (예: `Crew명 / 폴더 / 하위`)
    pub path: Option<String>,
}

impl ListItem {
    pub fn from_folder(folder: &FolderMetadata) -> Self {
        Self {
            id: folder.id.clone(),
            name: folder.name.clone(),
            is_dir: true,
            size: 0,
            created_at: folder.created_at.clone(),
            file_type: None,
            has_children: None,
            preview_url: None,
            path: None,
        }
    }

    pub fn from_file(file: &ObjectMetadata) -> Self {
        Self {
            id: file.id.clone(),
            name: file.name.clone(),
            is_dir: false,
            size: file.size,
            created_at: file.created_at.clone(),
            file_type: file.file_type.clone(),
            has_children: None,
            preview_url: None,
            path: None,
        }
    }

    pub fn from_file_with_path(file: &ObjectMetadata, path: Option<String>) -> Self {
        Self {
            id: file.id.clone(),
            name: file.name.clone(),
            is_dir: false,
            size: file.size,
            created_at: file.created_at.clone(),
            file_type: file.file_type.clone(),
            has_children: None,
            preview_url: None,
            path,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub label: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    pub is_current: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubtitleTrackInfo {
    pub id: String,
    pub name: String,
    pub label: String,
    /// 브라우저 `<track>` 에 넣을 WebVTT URL (상대 경로)
    pub vtt_url: String,
}

// User 및 Crew 구조

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner = 0,
    Manager = 1,
    Member = 2,
    Guest = 3,
}

impl Role {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Role::Owner),
            1 => Some(Role::Manager),
            2 => Some(Role::Member),
            3 => Some(Role::Guest),
            _ => None,
        }
    }

    pub fn can_write(self) -> bool {
        matches!(self, Role::Owner | Role::Manager)
    }

    pub fn can_read(self) -> bool {
        matches!(self, Role::Owner | Role::Manager | Role::Member)
    }

    pub fn can_manage_members(self) -> bool {
        matches!(self, Role::Owner | Role::Manager)
    }

    pub fn can_manage_crew_settings(self) -> bool {
        matches!(self, Role::Owner)
    }

    pub fn can_create_sub_crew(self) -> bool {
        matches!(self, Role::Owner | Role::Manager)
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending = 0,
    Active = 1,
    Invited = 2,
    Banned = 3,
}

impl Status {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Status::Pending),
            1 => Some(Status::Active),
            2 => Some(Status::Invited),
            3 => Some(Status::Banned),
            _ => None,
        }
    }

    pub fn is_active(self) -> bool {
        self == Status::Active
    }
}

/// Public = 비멤버도 Crew 존재·최상위 폴더 메타 조회 가능
/// Private = 멤버만 Crew 인지 가능
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrewVisibility {
    Public = 0,
    Private = 1,
}

impl CrewVisibility {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => CrewVisibility::Private,
            _ => CrewVisibility::Public,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,

    #[serde(skip_serializing)]
    pub password_hash: String,

    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Crew {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub depth: i32,
    pub visibility: CrewVisibility,
    pub max_sub_crew_depth: i32,
    pub root_folder_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrewMembership {
    pub user_id: i64,
    pub crew_id: String,
    pub role: u8,
    pub status: u8,
}

/// 앱 비밀번호 메타데이터 (평문 토큰은 발급 시 1회만 노출)
#[derive(Debug, Serialize, Deserialize)]
pub struct AppPasswordInfo {
    pub id: String,
    pub label: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebDavMountInfo {
    pub crew_id: String,
    pub crew_name: String,
    pub mount_path: String,
}

/// Crew 멤버 관리 화면용. 역할/상태는 Role/Status의 u8 값을 그대로 노출한다.
#[derive(Debug, Serialize, Deserialize)]
pub struct CrewMemberView {
    pub user_id: i64,
    pub username: String,
    pub role: u8,
    pub status: u8,
}

/// 현재 사용자가 관리(Owner/Manager) 권한을 가진 Crew. my_role은 본인의 역할.
#[derive(Debug, Serialize, Deserialize)]
pub struct ManageableCrew {
    pub id: String,
    pub name: String,
    pub visibility: CrewVisibility,
    pub root_folder_id: Option<String>,
    pub max_sub_crew_depth: i32,
    pub my_role: u8,
}

/// 삭제 권한이 있는 Crew (직접 Owner 또는 상위 Crew Owner).
#[derive(Debug, Serialize, Deserialize)]
pub struct DeletableCrew {
    pub id: String,
    pub name: String,
    pub visibility: CrewVisibility,
    pub parent_id: Option<String>,
    /// 이 Crew 자체의 Owner이면 true, 상위 Owner 권한만 있으면 false.
    pub is_direct_owner: bool,
}

/// 메인(홈) 화면의 파일/폴더 목록 옆에 함께 노출되는 Crew 항목.
/// 공개 Crew는 모두에게, 비공개 Crew는 멤버에게만 포함된다.
#[derive(Debug, Serialize, Deserialize)]
pub struct CrewListEntry {
    pub id: String,
    pub name: String,
    pub visibility: CrewVisibility,
    pub root_folder_id: Option<String>,
    /// 현재 사용자가 이 Crew의 활성 멤버인지 여부 (비로그인 시 false).
    pub is_member: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrewGuestView {
    pub id: String,
    pub name: String,
    pub visibility: CrewVisibility,
    pub root_folder_id: Option<String>,
    pub root_folder_name: Option<String>,
}

// Connect test
pub struct Greeting {
    pub message: String,
}
