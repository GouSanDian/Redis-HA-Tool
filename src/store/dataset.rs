//! store/dataset.rs - 数据集管理
//!
//! 本文件定义并实现数据集（DataSet）结构，用于跟踪 RDB 和 AOF 文件的元数据。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use tokio::sync::RwLock;
use crate::error::{SyncError, Result};
use crate::config::LocalCacheConfig;

/// RDB 文件元数据
#[derive(Debug, Clone)]
pub struct DataSetRdb {
    /// 文件路径
    pub path: PathBuf,
    /// 数据起始偏移量
    pub offset: i64,
    /// 数据大小（字节）
    pub size: i64,
    /// 文件创建时间
    pub created_at: SystemTime,
}

impl DataSetRdb {
    /// 从文件路径解析 RDB 元数据（同步版本）
    ///
    /// 文件名格式: `{offset}_{size}.rdb`
    pub fn from_path_sync(path: &Path) -> Result<Self> {
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| SyncError::Corrupted(format!("无效的 RDB 文件名: {}", path.display())))?;
        
        // 解析文件名: offset_size.rdb
        if !file_name.ends_with(".rdb") {
            return Err(SyncError::Corrupted(format!("RDB 文件名格式错误: {}", file_name)));
        }
        
        let name_without_ext = file_name.trim_end_matches(".rdb");
        let parts: Vec<&str> = name_without_ext.split('_').collect();
        
        if parts.len() != 2 {
            return Err(SyncError::Corrupted(format!("RDB 文件名格式错误: {}", file_name)));
        }
        
        let offset: i64 = parts[0].parse()
            .map_err(|_| SyncError::Corrupted(format!("RDB offset 解析失败: {}", parts[0])))?;
        
        let size: i64 = parts[1].parse()
            .map_err(|_| SyncError::Corrupted(format!("RDB size 解析失败: {}", parts[1])))?;
        
        let metadata = std::fs::metadata(path)
            .map_err(|e| SyncError::Io(e))?;
        
        let created_at = metadata.created()
            .unwrap_or_else(|_| SystemTime::now());
        
        Ok(DataSetRdb {
            path: path.to_path_buf(),
            offset,
            size,
            created_at,
        })
    }
    
    /// 获取文件大小（字节）- 同步版本
    pub fn file_size_sync(&self) -> Result<u64> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len())
    }
    
    /// 获取文件大小（字节）- 异步版本
    pub async fn file_size(&self) -> Result<u64> {
        let metadata = fs::metadata(&self.path).await?;
        Ok(metadata.len())
    }
}

/// AOF 文件段元数据
#[derive(Debug, Clone)]
pub struct DataSetAof {
    /// 文件路径
    pub path: PathBuf,
    /// 数据起始偏移量
    pub offset: i64,
    /// 文件创建时间
    pub created_at: SystemTime,
}

impl DataSetAof {
    /// 从文件路径解析 AOF 元数据（同步版本）
    ///
    /// 文件名格式: `{offset}.aof`
    pub fn from_path_sync(path: &Path) -> Result<Self> {
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| SyncError::Corrupted(format!("无效的 AOF 文件名: {}", path.display())))?;
        
        // 解析文件名: offset.aof
        if !file_name.ends_with(".aof") {
            return Err(SyncError::Corrupted(format!("AOF 文件名格式错误: {}", file_name)));
        }
        
        let name_without_ext = file_name.trim_end_matches(".aof");
        let offset: i64 = name_without_ext.parse()
            .map_err(|_| SyncError::Corrupted(format!("AOF offset 解析失败: {}", name_without_ext)))?;
        
        let metadata = std::fs::metadata(path)
            .map_err(|e| SyncError::Io(e))?;
        
        let created_at = metadata.created()
            .unwrap_or_else(|_| SystemTime::now());
        
        Ok(DataSetAof {
            path: path.to_path_buf(),
            offset,
            created_at,
        })
    }
    
    /// 获取文件大小（字节）- 同步版本
    pub fn file_size_sync(&self) -> Result<u64> {
        let metadata = std::fs::metadata(&self.path)?;
        Ok(metadata.len())
    }
    
    /// 获取文件大小（字节）- 异步版本
    pub async fn file_size(&self) -> Result<u64> {
        let metadata = fs::metadata(&self.path).await?;
        Ok(metadata.len())
    }
}

/// 数据集结构
///
/// 管理所有 RDB 和 AOF 文件的内存状态。
pub struct DataSet {
    /// 存储配置
    pub config: LocalCacheConfig,
    /// runId → RDB 文件列表
    pub rdb_files: HashMap<String, Vec<DataSetRdb>>,
    /// runId → AOF 文件段列表（按 offset 有序）
    pub aof_segments: HashMap<String, Vec<DataSetAof>>,
}

impl DataSet {
    /// 创建空数据集
    pub fn new(config: LocalCacheConfig) -> Self {
        DataSet {
            config,
            rdb_files: HashMap::new(),
            aof_segments: HashMap::new(),
        }
    }
    
    /// 从存储目录扫描并初始化数据集
    pub async fn scan_from_dir(base_dir: &Path) -> Result<Self> {
        let mut dataset = DataSet::new(LocalCacheConfig::default());
        
        // 遍历 base_dir 下的所有 runId 目录
        let mut entries = fs::read_dir(base_dir).await
            .map_err(|e| SyncError::Io(e))?;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            
            // runId 目录名
            let run_id = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            if run_id.is_empty() {
                continue;
            }
            
            // 扫描该 runId 目录下的 RDB 和 AOF 文件
            let mut rdb_files = Vec::new();
            let mut aof_segments = Vec::new();
            
            let mut sub_entries = fs::read_dir(&path).await?;
            while let Some(sub_entry) = sub_entries.next_entry().await? {
                let file_path = sub_entry.path();
                if !file_path.is_file() {
                    continue;
                }
                
                let file_name = file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                
                if file_name.ends_with(".rdb") {
                    if let Ok(rdb) = DataSetRdb::from_path_sync(&file_path) {
                        rdb_files.push(rdb);
                    }
                } else if file_name.ends_with(".aof") {
                    if let Ok(aof) = DataSetAof::from_path_sync(&file_path) {
                        aof_segments.push(aof);
                    }
                }
            }
            
            // 排序 RDB（按 offset）
            rdb_files.sort_by_key(|r| r.offset);
            
            // 排序 AOF（按 offset）
            aof_segments.sort_by_key(|a| a.offset);
            
            dataset.rdb_files.insert(run_id.to_string(), rdb_files);
            dataset.aof_segments.insert(run_id.to_string(), aof_segments);
        }
        
        Ok(dataset)
    }
    
    /// 添加 RDB 文件
    pub fn add_rdb(&mut self, run_id: &str, rdb: DataSetRdb) {
        self.rdb_files
            .entry(run_id.to_string())
            .or_insert_with(Vec::new)
            .push(rdb);
        
        // 保持有序
        self.rdb_files.get_mut(run_id)
            .unwrap()
            .sort_by_key(|r| r.offset);
    }
    
    /// 添加 AOF 段
    pub fn add_aof(&mut self, run_id: &str, aof: DataSetAof) {
        self.aof_segments
            .entry(run_id.to_string())
            .or_insert_with(Vec::new)
            .push(aof);
        
        // 保持有序
        self.aof_segments.get_mut(run_id)
            .unwrap()
            .sort_by_key(|a| a.offset);
    }
    
    /// 查找包含指定 offset 的 RDB 文件
    ///
    /// 返回包含该 offset 的 RDB 文件（offset 在 RDB 的范围内）
    pub fn find_rdb(&self, run_id: &str, offset: i64) -> Option<&DataSetRdb> {
        self.rdb_files.get(run_id)?.iter().find(|r| {
            r.offset <= offset && offset < r.offset + r.size
        })
    }
    
    /// 查找包含指定 offset 的 AOF 段
    ///
    /// 返回包含该 offset 的 AOF 文件
    pub fn find_aof(&self, run_id: &str, offset: i64) -> Option<&DataSetAof> {
        let segments = self.aof_segments.get(run_id)?;
        
        // 找到第一个 offset >= target 的段
        // 实际应该返回 offset <= target 的最后一个段
        let mut result = None;
        for segment in segments.iter().rev() {
            if segment.offset <= offset {
                result = Some(segment);
                break;
            }
        }
        
        result
    }
    
    /// 获取所有 RDB 文件大小总和
    pub async fn total_rdb_size(&self) -> Result<u64> {
        let mut total = 0u64;
        for rdb_list in self.rdb_files.values() {
            for rdb in rdb_list {
                total += rdb.file_size().await?;
            }
        }
        Ok(total)
    }
    
    /// 获取所有 AOF 文件大小总和
    pub async fn total_aof_size(&self) -> Result<u64> {
        let mut total = 0u64;
        for aof_list in self.aof_segments.values() {
            for aof in aof_list {
                total += aof.file_size().await?;
            }
        }
        Ok(total)
    }
    
    /// 验证 run_id 是否存在
    pub fn verify_run_id(&self, run_id: &str) -> bool {
        self.rdb_files.contains_key(run_id) || self.aof_segments.contains_key(run_id)
    }
    
    /// 获取指定 runId 的最新 RDB 文件
    pub fn latest_rdb(&self, run_id: &str) -> Option<&DataSetRdb> {
        self.rdb_files.get(run_id)?.last()
    }
    
    /// 获取指定 runId 的最新 AOF 段
    pub fn latest_aof(&self, run_id: &str) -> Option<&DataSetAof> {
        self.aof_segments.get(run_id)?.last()
    }
    
    /// 获取所有 runId 列表
    pub fn run_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.rdb_files.keys()
            .chain(self.aof_segments.keys())
            .cloned()
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }
}

/// 数据集管理器
///
/// 使用 RwLock 保护 DataSet，支持并发访问。
pub struct DataSetManager {
    /// 数据集（受 RwLock 保护）
    data_set: RwLock<DataSet>,
}

impl DataSetManager {
    /// 创建数据集管理器
    pub fn new(config: LocalCacheConfig) -> Self {
        DataSetManager {
            data_set: RwLock::new(DataSet::new(config)),
        }
    }
    
    /// 从目录初始化数据集
    pub async fn init_from_dir(&self, base_dir: &Path) -> Result<()> {
        let dataset = DataSet::scan_from_dir(base_dir).await?;
        
        let mut ds = self.data_set.write().await;
        *ds = dataset;
        
        Ok(())
    }
    
    /// 获取数据集读取锁
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, DataSet> {
        self.data_set.read().await
    }
    
    /// 获取数据集写入锁
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, DataSet> {
        self.data_set.write().await
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;
    
    /// 测试 DataSetRdb 文件名解析
    #[tokio::test]
    async fn test_dataset_rdb_from_path() {
        let temp_dir = tempdir().unwrap();
        let rdb_path = temp_dir.path().join("1000_2048.rdb");
        
        // 创建测试文件
        fs::File::create(&rdb_path).await.unwrap();
        
        let rdb = DataSetRdb::from_path_sync(&rdb_path).unwrap();
        assert_eq!(rdb.offset, 1000);
        assert_eq!(rdb.size, 2048);
    }
    
    /// 测试 DataSetAof 文件名解析
    #[tokio::test]
    async fn test_dataset_aof_from_path() {
        let temp_dir = tempdir().unwrap();
        let aof_path = temp_dir.path().join("500.aof");
        
        // 创建测试文件
        fs::File::create(&aof_path).await.unwrap();
        
        let aof = DataSetAof::from_path_sync(&aof_path).unwrap();
        assert_eq!(aof.offset, 500);
    }
    
    /// 测试 DataSet 扫描目录
    #[tokio::test]
    async fn test_dataset_scan_from_dir() {
        let temp_dir = tempdir().unwrap();
        
        // 创建 runId 目录
        let run_id_dir = temp_dir.path().join("abc123");
        fs::create_dir(&run_id_dir).await.unwrap();
        
        // 创建 RDB 文件
        let rdb_path = run_id_dir.join("1000_2048.rdb");
        let mut rdb_file = fs::File::create(&rdb_path).await.unwrap();
        rdb_file.write_all(b"test rdb data").await.unwrap();
        rdb_file.sync_all().await.unwrap();
        
        // 创建 AOF 文件
        let aof_path = run_id_dir.join("500.aof");
        let mut aof_file = fs::File::create(&aof_path).await.unwrap();
        aof_file.write_all(b"test aof data").await.unwrap();
        aof_file.sync_all().await.unwrap();
        
        // 扫描目录
        let dataset = DataSet::scan_from_dir(temp_dir.path()).await.unwrap();
        
        // 验证
        assert!(dataset.verify_run_id("abc123"));
        assert!(dataset.find_rdb("abc123", 1000).is_some());
        assert!(dataset.find_aof("abc123", 500).is_some());
        
        // 验证文件大小
        let rdb_size = dataset.total_rdb_size().await.unwrap();
        assert!(rdb_size > 0);
        
        let aof_size = dataset.total_aof_size().await.unwrap();
        assert!(aof_size > 0);
    }
    
    /// 测试 DataSetManager
    #[tokio::test]
    async fn test_dataset_manager() {
        let manager = DataSetManager::new(LocalCacheConfig::default());
        
        // 创建测试数据
        let temp_dir = tempdir().unwrap();
        let run_id_dir = temp_dir.path().join("test_run");
        fs::create_dir(&run_id_dir).await.unwrap();
        
        let rdb_path = run_id_dir.join("100_1000.rdb");
        fs::File::create(&rdb_path).await.unwrap();
        
        // 初始化
        manager.init_from_dir(temp_dir.path()).await.unwrap();
        
        // 读取验证
        let ds = manager.read().await;
        assert!(ds.verify_run_id("test_run"));
    }
    
    /// 测试无效文件名解析
    #[tokio::test]
    async fn test_invalid_file_name() {
        let temp_dir = tempdir().unwrap();
        
        // 无效的 RDB 文件名
        let invalid_rdb = temp_dir.path().join("invalid.rdb");
        fs::File::create(&invalid_rdb).await.unwrap();
        
        let result = DataSetRdb::from_path_sync(&invalid_rdb);
        assert!(result.is_err());
        
        // 无效的 AOF 文件名
        let invalid_aof = temp_dir.path().join("invalid.aof");
        fs::File::create(&invalid_aof).await.unwrap();
        
        let result = DataSetAof::from_path_sync(&invalid_aof);
        assert!(result.is_err());
    }
    
    /// 测试 find_rdb 查找包含指定 offset 的 RDB
    ///
    /// 验证修复：find_rdb 应该返回包含该 offset 的 RDB，
    /// 而不仅仅是起始 offset 精确匹配的 RDB
    #[tokio::test]
    async fn test_find_rdb_with_offset_in_range() {
        let temp_dir = tempdir().unwrap();
        
        // 创建 runId 目录
        let run_id_dir = temp_dir.path().join("test_run");
        fs::create_dir(&run_id_dir).await.unwrap();
        
        // 创建 RDB 文件：offset=1000, size=2048
        // 该 RDB 覆盖的 offset 范围是 [1000, 3048)
        let rdb_path = run_id_dir.join("1000_2048.rdb");
        let mut rdb_file = fs::File::create(&rdb_path).await.unwrap();
        rdb_file.write_all(b"test rdb data").await.unwrap();
        rdb_file.sync_all().await.unwrap();
        
        // 扫描目录
        let dataset = DataSet::scan_from_dir(temp_dir.path()).await.unwrap();
        
        // 验证：起始 offset 应该能找到
        assert!(dataset.find_rdb("test_run", 1000).is_some(), "应该找到起始 offset=1000 的 RDB");
        
        // 验证：范围内的 offset 也应该能找到（这是修复的关键）
        assert!(dataset.find_rdb("test_run", 1500).is_some(), "应该找到 offset=1500（在范围内）的 RDB");
        assert!(dataset.find_rdb("test_run", 2000).is_some(), "应该找到 offset=2000（在范围内）的 RDB");
        assert!(dataset.find_rdb("test_run", 3047).is_some(), "应该找到 offset=3047（范围内最后一个字节）的 RDB");
        
        // 验证：范围外的 offset 不应该找到
        assert!(dataset.find_rdb("test_run", 999).is_none(), "不应该找到 offset=999（范围外）的 RDB");
        assert!(dataset.find_rdb("test_run", 3048).is_none(), "不应该找到 offset=3048（刚好等于结束位置）的 RDB");
        assert!(dataset.find_rdb("test_run", 4000).is_none(), "不应该找到 offset=4000（范围外）的 RDB");
        
        // 验证：不存在的 run_id
        assert!(dataset.find_rdb("nonexistent", 1500).is_none(), "不应该找到不存在的 run_id");
    }
}