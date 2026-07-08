use crate::domain::errors::{StorageError, StorageResult};
use crate::domain::ports::StoragePort;
use async_trait::async_trait;
use axum::body::BodyDataStream;
use futures::StreamExt;
use std::env;
use std::path::PathBuf;
use sysinfo::Disks;
use tokio::{fs, io::AsyncWriteExt};
use tracing::{debug, error};
pub struct DiskStorage {
    base_path: PathBuf,
}

impl DiskStorage {
    pub fn new(path: &str) -> Self {
        Self {
            base_path: PathBuf::from(path),
        }
    }
}

#[async_trait]
impl StoragePort for DiskStorage {
    // async fn save_file(&self, id: &str, data: &[u8]) -> StorageResult<()> {
    //     let path = self.get_physical_path(id);
    //
    //     if let Some(parent) = path.parent() {
    //         fs::create_dir_all(parent).await?;
    //     }
    //
    //     fs::write(&path, data).await?;
    //     debug!("File saved successfully at: {:?}", path);
    //     Ok(())
    // }
    async fn save_file_stream(
        &self,
        id: &str,
        mut stream: BodyDataStream,
    ) -> StorageResult<(u64, String)> {
        let path = self.get_physical_path(id);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&path).await?;
        let mut hasher = blake3::Hasher::new();
        let mut total_size: u64 = 0;
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| {
                error!("Stream read error: {}", e);
                StorageError::Network(e.to_string())
            })?;

            total_size += chunk.len() as u64;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
        }

        file.flush().await.map_err(|e| {
            error!("Flush failed: {}", e);
            e
        })?;
        let checksum = hasher.finalize().to_hex().to_string();

        debug!("File saved successfully viia stream at: {:?}", path);
        Ok((total_size, checksum))
    }
    async fn get_file(&self, id: &str) -> StorageResult<tokio::fs::File> {
        // 단순히 join(key)가 아니라 샤딩된 경로를 가져와야 합니다.
        let file_path = self.get_physical_path(id);
        let file = tokio::fs::File::open(file_path).await?;
        Ok(file)
    }

    async fn delete_file(&self, id: &str) -> StorageResult<()> {
        // 삭제 시에도 샤딩된 경로를 사용합니다.
        let file_path = self.get_physical_path(id);

        // 파일이 존재하지 않을 때 에러를 무시하고 싶다면 아래와 같이 처리합니다.
        match fs::remove_file(file_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn get_physical_path(&self, id: &str) -> PathBuf {
        if id.len() >= 4 {
            let shard1 = &id[0..2];
            let shard2 = &id[2..4];
            self.base_path.join(shard1).join(shard2).join(id)
        } else {
            self.base_path.join(id)
        }
    }
    fn get_capacity(&self) -> (u64, u64) {
        let target_path_str = env::var("STORAGE_PATH").expect("STORAGE might be selected by user");
        let target_path = std::path::Path::new(&target_path_str);
        let disks = Disks::new_with_refreshed_list();

        let mut best_match: Option<&sysinfo::Disk> = None;
        let mut max_len = 0;
        for disk in disks.list() {
            let mount_point = disk.mount_point();
            if target_path.starts_with(mount_point) {
                let mount_str = mount_point.to_str().unwrap_or("");
                if mount_str.len() >= max_len {
                    max_len = mount_str.len();
                    best_match = Some(disk);
                }
            }
        }
        best_match
            .map(|d| (d.total_space(), d.available_space()))
            .unwrap_or((0, 0))
    }
    async fn get_file_for_write(&self, id: &str) -> StorageResult<tokio::fs::File> {
        let path = self.get_physical_path(id); // 기존에 쓰시던 경로 생성 함수
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // 💡 2. 파일을 쓰기 모드로 엽니다.
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .await?;
        Ok(file)
    }
}
