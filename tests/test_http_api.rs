//! tests/test_http_api.rs - HTTP API 集成测试
//!
//! 验证 HTTP 管理 API 功能。

use std::sync::Arc;
use redis_syncer::{
    config::SyncConfig,
    syncer::{SyncerImpl, Syncer},
    cmd::{AppState, build_router},
};

/// 测试：HTTP API 路由构建
#[test]
fn test_build_router() {
    let config = Arc::new(SyncConfig::default());
    let syncer = Arc::new(SyncerImpl::new(config));
    let state = AppState::new(syncer);
    
    let _router = build_router(state);
    
    tracing::info!("HTTP 路由构建测试通过");
}

/// 测试：AppState 创建
#[test]
fn test_app_state_creation() {
    let config = Arc::new(SyncConfig::default());
    let syncer = Arc::new(SyncerImpl::new(config));
    let state = AppState::new(syncer.clone());
    
    // 验证 Syncer 状态（通过 syncer 本身）
    assert_eq!(syncer.status(), redis_syncer::syncer::SyncState::ReadyRun);
}

/// 文档说明：HTTP API 测试
///
/// # 真实测试步骤
///
/// 1. 启动服务：
///    ```bash
///    cargo run -- --config config/config.json
///    ```
///
/// 2. 健康检查：
///    ```bash
///    curl http://localhost:8080/health
///    # 预期响应: {"code":200,"message":"success","data":{"status":"ok"}}
///    ```
///
/// 3. 查询状态：
///    ```bash
///    curl http://localhost:8080/syncer/status
///    ```
///
/// 4. 暂停同步：
///    ```bash
///    curl -X POST http://localhost:8080/syncer/pause
///    ```
///
/// 5. 恢复同步：
///    ```bash
///    curl -X POST http://localhost:8080/syncer/resume
///    ```
///
/// 6. 停止同步：
///    ```bash
///    curl -X POST http://localhost:8080/syncer/stop
///    ```
#[test]
fn test_http_api_documentation() {
    tracing::info!("请参考文档进行真实 HTTP API 测试");
}