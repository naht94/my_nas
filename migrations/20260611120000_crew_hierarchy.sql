-- Crew 계층·가시성·루트 폴더 컬럼 추가
ALTER TABLE crews ADD COLUMN max_sub_crew_depth INTEGER NOT NULL DEFAULT 3;
ALTER TABLE crews ADD COLUMN root_folder_id TEXT REFERENCES folders(id);
ALTER TABLE crews ADD COLUMN visibility INTEGER NOT NULL DEFAULT 0;

-- access_level → visibility 마이그레이션 (기존 컬럼이 있으면 값 복사)
UPDATE crews SET visibility = access_level WHERE visibility = 0 AND access_level != 0;

-- 글로벌 루트 Crew 시드
INSERT OR IGNORE INTO crews (
    id, name, parent_id, depth, access_level, visibility,
    max_sub_crew_depth, root_folder_id, created_at
) VALUES (
    'global-root-uuid', 'Global', NULL, 0, 0, 0,
    10, NULL, datetime('now')
);
