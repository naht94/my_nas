use serde::{Deserialize, Serialize};

// file 및 folder 구조

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FolderMetadata {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
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
}

// User 및 Crew 구조

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)] // 상태 비교를 위해 파생 트레이트 추가
pub enum Role {
    Owner = 0,
    Manager = 1,
    Member = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending = 0,
    Active = 1,
    Invited = 2,
    Banned = 3,
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
    pub access_level: i32,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CrewMembership {
    pub user_id: i64,
    pub crew_id: String,
    pub role: u8,   // 0: Owner, 1: Manager, 2: Member
    pub status: u8, // 0: Pending, 1: Active, 2: Invited, 3: Banned
}

// Connect test
pub struct Greeting {
    pub message: String,
}
