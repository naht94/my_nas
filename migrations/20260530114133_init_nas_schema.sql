CREATE TABLE folders (
    id TEXT PRIMARY KEY,
    parent_id TEXT,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL, is_deleted INTEGER DEFAULT 0, crew_id TEXT,
    FOREIGN KEY (parent_id) REFERENCES folders(id) ON DELETE CASCADE
);
CREATE TABLE files (
    id TEXT PRIMARY KEY,
    folder_id TEXT,
    name TEXT NOT NULL,
    size INTEGER NOT NULL,
    file_type TEXT,
    checksum TEXT,
    version INTEGER DEFAULT 1,
    is_deleted INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (folder_id) REFERENCES folders(id) ON DELETE CASCADE
);
CREATE INDEX idx_files_folder ON files(folder_id);
CREATE INDEX idx_folders_parent ON folders(parent_id);
CREATE INDEX idx_files_folder_id_name ON files (folder_id, name);
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE crews (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    parent_id TEXT,
    depth INTEGER DEFAULT 0,
    access_level INTEGER DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(parent_id) REFERENCES crews(id) ON DELETE CASCADE
);
CREATE TABLE crew_user (
    user_id INTEGER NOT NULL,
    crew_id TEXT NOT NULL,
    role INTEGER NOT NULL,
    status INTEGER NOT NULL,
    PRIMARY KEY (user_id, crew_id),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(crew_id) REFERENCES crews(id) ON DELETE CASCADE
);
CREATE INDEX idx_folders_crew ON folders(crew_id);
