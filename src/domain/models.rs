use serde::{Deserialize, Serialize};

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

pub struct Greeting {
    pub message: String,
}
