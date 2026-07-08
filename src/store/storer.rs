//! store/storer.rs - 文件存储实现
//!
//! 本文件实现 FileStorer，基于本地文件系统的 Storer trait 实现。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::io::{AsyncWrite, BufWriter};
use tokio::sync::Notify;
use async_trait::async_trait;
use crate::error::{SyncError, Result};
use crate::config::LocalCacheConfig;
use crate::store::{Storer, Reader, DataSetManager, RdbReader, AofReader, RdbWriter, AofWriter};

/// 文件存储实现
///
/// 基于 tokio::fs 实现异步文件存储管理。
pub struct FileStorer {
    /// 存储基础目录
    base_dir: PathBuf,
    /// 存储配置
    config: LocalCacheConfig,
    /// 数据集管理器
    dataset_manager: Arc<DataSetManager>,
    /// 数据通知器（Writer 写入后通知等待者）
    notify: Arc<Notify>,
}

impl FileStorer {
    /// 创建文件存储器
    ///
    /// # 参数
    /// - base_dir: 存储基础目录
    /// - config: 本地缓存配置
    pub fn new(base_dir: PathBuf, config: LocalCacheConfig) -> Self {
        FileStorer {
            base_dir,
            config: config.clone(),
            dataset_manager: Arc::new(DataSetManager::new(config)),
            notify: Arc::new(Notify::new()),
        }
    }

    /// 获取数据通知器
    pub fn data_notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }
    
    /// 创建存储目录（如果不存在）
    async fn ensure_dir_exists(&self, path: &Path) -> Result<()> {
        if !fs::try_exists(path).await? {
            fs::create_dir_all(path).await?;
        }
        Ok(())
    }
    
    /// 获取 runId 目录路径
    fn run_id_dir(&self, run_id: &str) -> PathBuf {
        self.base_dir.join(run_id)
    }
    
    /// 生成 RDB 文件名
    fn rdb_file_name(offset: i64, size: i64) -> String {
        format!("{}_{}.rdb", offset, size)
    }
    
    /// 生成 AOF 文件名
    fn aof_file_name(offset: i64) -> String {
        format!("{}.aof", offset)
    }
    
    /// 执行垃圾回收（清理旧文件）
    async fn do_gc(&self) -> Result<()> {
        let dataset = self.dataset_manager.read().await;
        let max_size = self.config.max_size;
        
        // 计算当前总大小
        let current_size = dataset.total_rdb_size().await? + dataset.total_aof_size().await?;
        
        if current_size <= max_size as u64 {
            return Ok(());
        }
        
        // 获取所有文件并按时间排序（删除最旧的）
        let mut files_to_delete: Vec<(PathBuf, SystemTime)> = Vec::new();
        
        for run_id in dataset.run_ids() {
            // 收集 RDB 文件
            if let Some(rdb_list) = dataset.rdb_files.get(&run_id) {
                for rdb in rdb_list {
                    files_to_delete.push((rdb.path.clone(), rdb.created_at));
                }
            }
            
            // 收集 AOF 文件（保留最新的）
            if let Some(aof_list) = dataset.aof_segments.get(&run_id) {
                for (idx, aof) in aof_list.iter().enumerate() {
                    // 不删除最新的 AOF 文件
                    if idx < aof_list.len() - 1 {
                        files_to_delete.push((aof.path.clone(), aof.created_at));
                    }
                }
            }
        }
        
        // 按时间排序（最旧的在前）
        files_to_delete.sort_by_key(|(_, time)| *time);
        
        // 删除文件直到总大小小于 max_size
        let mut deleted_size = 0u64;
        let target_delete_size = current_size - max_size as u64;
        
        for (path, _) in files_to_delete {
            if deleted_size >= target_delete_size {
                break;
            }
            
            let file_size = fs::metadata(&path).await?.len();
            fs::remove_file(&path).await?;
            deleted_size += file_size;
            
            tracing::info!("GC 删除文件: {} ({} 字节)", path.display(), file_size);
        }
        
        Ok(())
    }
}

#[async_trait]
impl Storer for FileStorer {
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>> {
        let dataset = self.dataset_manager.read().await;

        // 尝试查找所有 runId 下的 RDB 或 AOF
        for run_id in dataset.run_ids() {
            // 先查找 RDB
            if let Some(rdb) = dataset.find_rdb(&run_id, offset) {
                // 计算 RDB 内剩余可读字节数
                let remaining = rdb.offset + rdb.size - offset;
                tracing::debug!("找到 RDB Reader: run_id={}, offset={}, rdb.offset={}, rdb.size={}, remaining={}",
                    run_id, offset, rdb.offset, rdb.size, remaining);
                let reader = RdbReader::open(&rdb.path, offset, remaining)?;
                return Ok(Box::new(reader));
            }

            // 再查找 AOF
            if let Some(aof) = dataset.find_aof(&run_id, offset) {
                tracing::debug!("找到 AOF Reader: run_id={}, offset={}, aof.offset={}",
                    run_id, offset, aof.offset);
                let reader = AofReader::open(&aof.path, offset, aof.offset)?;
                return Ok(Box::new(reader));
            }
        }

        Err(SyncError::Corrupted(format!("找不到 offset={} 的数据文件", offset)))
    }
    
    async fn get_rdb_writer(
        &self,
        run_id: &str,
        offset: i64,
        size: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>> {
        let run_id_dir = self.run_id_dir(run_id);
        self.ensure_dir_exists(&run_id_dir).await?;

        let rdb_path = run_id_dir.join(Self::rdb_file_name(offset, size));

        // 创建文件
        let file = fs::File::create(&rdb_path).await?;
        let writer = BufWriter::new(file);

        // 更新数据集
        let mut dataset = self.dataset_manager.write().await;
        dataset.add_rdb(run_id, crate::store::DataSetRdb {
            path: rdb_path.clone(),
            offset,
            size,
            created_at: std::time::SystemTime::now(),
        });

        tracing::info!("创建 RDB Writer: {}", rdb_path.display());

        Ok(Box::new(RdbWriter::new(writer, rdb_path, self.notify.clone())))
    }

    async fn get_aof_writer(
        &self,
        run_id: &str,
        offset: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>> {
        let run_id_dir = self.run_id_dir(run_id);
        self.ensure_dir_exists(&run_id_dir).await?;

        let aof_path = run_id_dir.join(Self::aof_file_name(offset));

        // 创建文件
        let file = fs::File::create(&aof_path).await?;
        let writer = BufWriter::new(file);

        // 更新数据集
        let mut dataset = self.dataset_manager.write().await;
        dataset.add_aof(run_id, crate::store::DataSetAof {
            path: aof_path.clone(),
            offset,
            created_at: std::time::SystemTime::now(),
        });

        tracing::info!("创建 AOF Writer: {}", aof_path.display());

        Ok(Box::new(AofWriter::new(writer, aof_path, self.config.clone(), self.notify.clone())))
    }
    
    async fn init_data_set(&self) -> Result<()> {
        self.dataset_manager.init_from_dir(&self.base_dir).await?;
        tracing::info!("数据集初始化完成，扫描目录: {}", self.base_dir.display());
        Ok(())
    }
    
    async fn gc_data_set(&self) -> Result<()> {
        self.do_gc().await?;
        tracing::info!("数据集 GC 完成");
        Ok(())
    }
    
    fn verify_run_id(&self, run_id: &str) -> bool {
        // 使用阻塞读取（仅在验证时）
        // 如果需要异步，应改为 async fn
        // 这里简化处理，直接返回 true（实际应检查文件是否存在）
        std::fs::exists(self.run_id_dir(run_id)).unwrap_or(false)
    }
    
    async fn available_bytes(&self, offset: i64) -> Result<i64> {
        let dataset = self.dataset_manager.read().await;

        for run_id in dataset.run_ids() {
            // 先查找 RDB
            if let Some(rdb) = dataset.find_rdb(&run_id, offset) {
                match fs::metadata(&rdb.path).await {
                    Ok(_m) => {
                        // RDB 的可用字节是从 offset 到 RDB 结束位置
                        let rdb_end = rdb.offset + rdb.size;
                        if offset < rdb_end {
                            let available = rdb_end - offset;
                            tracing::debug!("RDB available_bytes: offset={}, rdb.offset={}, rdb.size={}, rdb_end={}, available={}",
                                offset, rdb.offset, rdb.size, rdb_end, available);
                            return Ok(available);
                        }
                    }
                    Err(_) => {}
                }
            }

            // 再查找 AOF
            if let Some(aof) = dataset.find_aof(&run_id, offset) {
                match fs::metadata(&aof.path).await {
                    Ok(_m) => {
                        let file_len = _m.len() as i64;
                        let file_pos = offset - aof.offset;
                        let available = file_len - file_pos;
                        tracing::debug!("AOF available_bytes: offset={}, aof.offset={}, file_len={}, file_pos={}, available={}",
                            offset, aof.offset, file_len, file_pos, available);
                        if available > 0 {
                            return Ok(available);
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        tracing::debug!("available_bytes: 未找到 offset={} 的数据", offset);
        Ok(0)
    }

    fn data_notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;
    
    /// 测试 FileStorer 创建和初始化
    #[tokio::test]
    async fn test_file_storer_init() {
        let temp_dir = tempdir().unwrap();
        let config = LocalCacheConfig::default();
        
        let storer = FileStorer::new(temp_dir.path().to_path_buf(), config);
        storer.init_data_set().await.unwrap();
    }
    
    /// 测试 RDB Writer 创建和写入
    #[tokio::test]
    async fn test_file_storer_rdb_writer() {
        let temp_dir = tempdir().unwrap();
        let config = LocalCacheConfig::default();
        
        let storer = FileStorer::new(temp_dir.path().to_path_buf(), config);
        storer.init_data_set().await.unwrap();
        
        // 创建 RDB Writer
        let mut writer = storer.get_rdb_writer("run123", 1000, 2048).await.unwrap();
        
        // 写入数据
        writer.write_all(b"test rdb data").await.unwrap();
        writer.flush().await.unwrap();
        
        // 验证文件存在
        let rdb_path = temp_dir.path().join("run123").join("1000_2048.rdb");
        assert!(fs::try_exists(&rdb_path).await.unwrap());
    }
    
    /// 测试 AOF Writer 创建和写入
    #[tokio::test]
    async fn test_file_storer_aof_writer() {
        let temp_dir = tempdir().unwrap();
        let config = LocalCacheConfig::default();
        
        let storer = FileStorer::new(temp_dir.path().to_path_buf(), config);
        storer.init_data_set().await.unwrap();
        
        // 创建 AOF Writer
        let mut writer = storer.get_aof_writer("run123", 500).await.unwrap();
        
        // 写入数据
        writer.write_all(b"test aof data").await.unwrap();
        writer.flush().await.unwrap();
        
        // 验证文件存在
        let aof_path = temp_dir.path().join("run123").join("500.aof");
        assert!(fs::try_exists(&aof_path).await.unwrap());
    }
    
    /// 测试 verify_run_id
    #[tokio::test]
    async fn test_file_storer_verify_run_id() {
        let temp_dir = tempdir().unwrap();
        let config = LocalCacheConfig::default();
        
        let storer = FileStorer::new(temp_dir.path().to_path_buf(), config);
        
        // 创建 runId 目录
        fs::create_dir(temp_dir.path().join("run123")).await.unwrap();
        
        assert!(storer.verify_run_id("run123"));
        assert!(!storer.verify_run_id("nonexistent"));
    }
    
    /// 测试 Reader 获取
    #[tokio::test]
    async fn test_file_storer_get_reader() {
        let temp_dir = tempdir().unwrap();
        let config = LocalCacheConfig::default();
        
        let storer = FileStorer::new(temp_dir.path().to_path_buf(), config);
        storer.init_data_set().await.unwrap();
        
        // 先创建 RDB 文件
        let mut writer = storer.get_rdb_writer("run123", 1000, 2048).await.unwrap();
        writer.write_all(b"test rdb data").await.unwrap();
        writer.flush().await.unwrap();
        
        // 重新初始化数据集（扫描新文件）
        storer.init_data_set().await.unwrap();
        
        // 获取 Reader
        let reader = storer.get_reader(1000).await.unwrap();
        assert_eq!(reader.reader_type(), crate::store::ReaderType::Rdb);
    }
}