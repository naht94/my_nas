-- 동시성 경쟁 시 중복 이름 생성을 막는 방어선 (앱 로직의 중복 검사 보완)
-- 소프트 삭제(is_deleted=1)된 행은 제약에서 제외한다.
-- NULL은 SQLite UNIQUE에서 서로 다르게 취급되므로 COALESCE로 정규화한다.
-- 폴더: (Crew, 상위폴더, 이름)이 활성 행 중 유일해야 한다.
--   - Crew 루트 폴더(parent_id NULL)는 crew_id가 서로 달라 충돌하지 않는다.
--   - 개인/전역 최상위(crew_id NULL, parent_id NULL)는 이름으로 유일.
CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_unique_name
ON folders (COALESCE(crew_id, ''), COALESCE(parent_id, ''), name)
WHERE is_deleted = 0;

-- 파일: (폴더, 이름)이 활성 행 중 유일해야 한다.
CREATE UNIQUE INDEX IF NOT EXISTS idx_files_unique_name
ON files (COALESCE(folder_id, ''), name)
WHERE is_deleted = 0;
