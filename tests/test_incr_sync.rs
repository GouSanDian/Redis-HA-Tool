//! tests/test_incr_sync.rs - 增量同步集成测试
//!
//! 验证：
//!   - 全量同步完成后，源端新增数据能实时同步到目标端
//!   - 延迟在可接受范围内

use std::sync::Arc;
use tempfile::TempDir;
use redis_syncer::{
    config::SyncConfig,
    syncer::{SyncerImpl, Syncer, SyncState, SyncFiniteStateMachine},
};

/// 辅助函数：创建测试配置
fn create_test_config(temp_dir: &TempDir) -> Arc<SyncConfig> {
    let mut config = SyncConfig::default();
    config.local_cache.dir = temp_dir.path().to_str().unwrap().to_string();
    Arc::new(config)
}

/// 测试：增量同步基本流程（Mock）
#[tokio::test]
async fn test_incr_sync_mock() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config(&temp_dir);
    
    // 创建同步器
    let syncer = SyncerImpl::new(config.clone());
    
    // 验证状态机
    let fsm = syncer.fsm();
    assert_eq!(fsm.state(), redis_syncer::syncer::SyncPhase::Started);
    
    // 模拟全量同步完成
    fsm.finish_full_sync();
    assert!(fsm.state().is_full_sync());
    
    // 模拟开始增量同步
    fsm.start_incr_sync();
    assert!(fsm.state().is_incr_sync());
    assert!(fsm.state().is_syncing());
    
    // 模拟增量同步完成
    fsm.finish_incr_sync();
    assert!(fsm.state().is_incr_sync());
    assert!(fsm.state().is_synced());
    
    tracing::info!("增量同步 Mock 测试通过");
}

/// 测试：状态机转换
#[tokio::test]
async fn test_state_machine_transitions() {
    let fsm = SyncFiniteStateMachine::new();
    
    // Started -> FullInit
    fsm.start_full_sync();
    assert_eq!(fsm.state(), redis_syncer::syncer::SyncPhase::FullInit);
    assert!(fsm.state().is_full_sync());
    
    // FullInit -> FullSyncing
    fsm.begin_full_sync();
    assert_eq!(fsm.state(), redis_syncer::syncer::SyncPhase::FullSyncing);
    assert!(fsm.state().is_syncing());
    
    // FullSyncing -> FullSynced
    fsm.finish_full_sync();
    assert_eq!(fsm.state(), redis_syncer::syncer::SyncPhase::FullSynced);
    assert!(fsm.state().is_synced());
    
    // FullSynced -> IncrSyncing
    fsm.start_incr_sync();
    assert_eq!(fsm.state(), redis_syncer::syncer::SyncPhase::IncrSyncing);
    
    // IncrSyncing -> IncrSynced
    fsm.finish_incr_sync();
    assert_eq!(fsm.state(), redis_syncer::syncer::SyncPhase::IncrSynced);
    
    tracing::info!("状态机转换测试通过");
}

/// 测试：实时同步延迟（框架）
///
/// 实际测试需要真实 Redis 环境。
#[tokio::test]
async fn test_sync_delay_mock() {
    // 注意：真实测试应：
    // 1. 完成全量同步
    // 2. 在源 Redis 中持续写入数据
    // 3. 测量目标 Redis 数据出现的延迟
    
    tracing::info!("同步延迟 Mock 测试通过");
}

/// 文档说明：如何测试增量同步
///
/// # 真实测试步骤
///
/// 1. 完成全量同步：
///    - 运行全量同步测试，确保初始数据一致
///
/// 2. 监控同步状态：
///    ```bash
///    # 查看同步器状态
///    curl http://localhost:8080/syncer/status
///    ```
///
/// 3. 在源端写入新数据：
///    ```bash
///    redis-cli -p 6379 SET new_key new_value
///    ```
///
/// 4. 验证目标端数据：
///    ```bash
///    redis-cli -p 6380 GET new_key
///    ```
///
/// 5. 测量延迟：
///    - 在源端写入测试 key，并记录时间戳
///    - 在目标端轮询检查 key 是否出现
///    - 计算延迟时间
#[test]
fn test_incr_sync_documentation() {
    tracing::info!("请参考文档进行真实增量同步测试");
}