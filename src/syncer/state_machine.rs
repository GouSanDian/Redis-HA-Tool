//! syncer/state_machine.rs - 同步状态机实现
//!
//! 本文件实现同步状态机，跟踪同步过程的状态变化。

use std::sync::atomic::{AtomicU8, Ordering};
use tokio::sync::watch;

/// 同步阶段枚举
///
/// 定义同步过程中的具体阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncPhase {
    /// 已启动
    Started = 0,
    /// 全量同步初始化
    FullInit = 1,
    /// 全量同步进行中
    FullSyncing = 2,
    /// 全量同步完成
    FullSynced = 3,
    /// 增量同步进行中
    IncrSyncing = 4,
    /// 增量同步完成（稳定状态）
    IncrSynced = 5,
}

impl Default for SyncPhase {
    fn default() -> Self {
        SyncPhase::Started
    }
}

impl From<u8> for SyncPhase {
    fn from(value: u8) -> Self {
        match value {
            0 => SyncPhase::Started,
            1 => SyncPhase::FullInit,
            2 => SyncPhase::FullSyncing,
            3 => SyncPhase::FullSynced,
            4 => SyncPhase::IncrSyncing,
            5 => SyncPhase::IncrSynced,
            _ => SyncPhase::Started,
        }
    }
}

impl SyncPhase {
    /// 转换为 u8
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
    
    /// 是否为全量同步阶段
    pub fn is_full_sync(&self) -> bool {
        matches!(self, SyncPhase::FullInit | SyncPhase::FullSyncing | SyncPhase::FullSynced)
    }
    
    /// 是否为增量同步阶段
    pub fn is_incr_sync(&self) -> bool {
        matches!(self, SyncPhase::IncrSyncing | SyncPhase::IncrSynced)
    }
    
    /// 是否正在同步
    pub fn is_syncing(&self) -> bool {
        matches!(self, SyncPhase::FullSyncing | SyncPhase::IncrSyncing)
    }
    
    /// 是否已同步完成
    pub fn is_synced(&self) -> bool {
        matches!(self, SyncPhase::FullSynced | SyncPhase::IncrSynced)
    }
}

/// 同步状态机
///
/// 使用原子变量和 watch channel 管理状态变化。
pub struct SyncFiniteStateMachine {
    /// 当前状态（原子存储）
    state: AtomicU8,
    
    /// 状态变化通知发送器
    tx: watch::Sender<SyncPhase>,
    
    /// 状态变化通知接收器（用于外部观察）
    rx: watch::Receiver<SyncPhase>,
}

impl SyncFiniteStateMachine {
    /// 创建新的状态机
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(SyncPhase::Started);
        
        SyncFiniteStateMachine {
            state: AtomicU8::new(SyncPhase::Started.as_u8()),
            tx,
            rx,
        }
    }
    
    /// 设置状态
    ///
    /// # 参数
    /// - phase: 新的同步阶段
    pub fn set_state(&self, phase: SyncPhase) {
        self.state.store(phase.as_u8(), Ordering::SeqCst);
        
        // 发送状态变化通知
        if self.tx.send(phase).is_err() {
            tracing::warn!("状态变化通知发送失败");
        }
        
        tracing::info!("同步状态变化: {:?}", phase);
    }
    
    /// 获取当前状态
    pub fn state(&self) -> SyncPhase {
        SyncPhase::from(self.state.load(Ordering::SeqCst))
    }
    
    /// 获取状态变化通知接收器
    ///
    /// 用于外部观察状态变化。
    pub fn state_notify(&self) -> watch::Receiver<SyncPhase> {
        self.rx.clone()
    }
    
    /// 等待状态变化
    ///
    /// 异步等待状态从当前值变化。
    pub async fn wait_for_change(&self) {
        let mut rx = self.rx.clone();
        rx.changed().await.ok();
    }
    
    /// 从 Started → FullInit
    pub fn start_full_sync(&self) {
        self.set_state(SyncPhase::FullInit);
    }
    
    /// 从 FullInit → FullSyncing
    pub fn begin_full_sync(&self) {
        self.set_state(SyncPhase::FullSyncing);
    }
    
    /// 从 FullSyncing → FullSynced
    pub fn finish_full_sync(&self) {
        self.set_state(SyncPhase::FullSynced);
    }
    
    /// 从 FullSynced → IncrSyncing
    pub fn start_incr_sync(&self) {
        self.set_state(SyncPhase::IncrSyncing);
    }
    
    /// 从 IncrSyncing → IncrSynced
    pub fn finish_incr_sync(&self) {
        self.set_state(SyncPhase::IncrSynced);
    }
    
    /// 重置状态到 Started
    pub fn reset(&self) {
        self.set_state(SyncPhase::Started);
    }
}

impl Default for SyncFiniteStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 SyncPhase 转换
    #[test]
    fn test_sync_phase_conversion() {
        assert_eq!(SyncPhase::from(0), SyncPhase::Started);
        assert_eq!(SyncPhase::from(1), SyncPhase::FullInit);
        assert_eq!(SyncPhase::from(2), SyncPhase::FullSyncing);
        assert_eq!(SyncPhase::from(3), SyncPhase::FullSynced);
        assert_eq!(SyncPhase::from(4), SyncPhase::IncrSyncing);
        assert_eq!(SyncPhase::from(5), SyncPhase::IncrSynced);
        assert_eq!(SyncPhase::from(255), SyncPhase::Started);  // 默认值
    }
    
    /// 测试 SyncPhase 类型判断
    #[test]
    fn test_sync_phase_checks() {
        let full_init = SyncPhase::FullInit;
        assert!(full_init.is_full_sync());
        assert!(!full_init.is_incr_sync());
        
        let incr_syncing = SyncPhase::IncrSyncing;
        assert!(incr_syncing.is_incr_sync());
        assert!(incr_syncing.is_syncing());
        
        let incr_synced = SyncPhase::IncrSynced;
        assert!(incr_synced.is_synced());
        assert!(!incr_synced.is_syncing());
    }
    
    /// 测试状态机基本操作
    #[test]
    fn test_state_machine_basic() {
        let machine = SyncFiniteStateMachine::new();
        
        assert_eq!(machine.state(), SyncPhase::Started);
        
        machine.start_full_sync();
        assert_eq!(machine.state(), SyncPhase::FullInit);
        
        machine.begin_full_sync();
        assert_eq!(machine.state(), SyncPhase::FullSyncing);
        
        machine.finish_full_sync();
        assert_eq!(machine.state(), SyncPhase::FullSynced);
        
        machine.start_incr_sync();
        assert_eq!(machine.state(), SyncPhase::IncrSyncing);
        
        machine.finish_incr_sync();
        assert_eq!(machine.state(), SyncPhase::IncrSynced);
        
        machine.reset();
        assert_eq!(machine.state(), SyncPhase::Started);
    }
    
    /// 测试状态变化通知
    #[tokio::test]
    async fn test_state_notify() {
        let machine = SyncFiniteStateMachine::new();
        let mut rx = machine.state_notify();
        
        // 初始状态
        assert_eq!(*rx.borrow(), SyncPhase::Started);
        
        // 修改状态
        machine.start_full_sync();
        
        // 等待变化
        rx.changed().await.ok();
        
        // 验证新状态
        assert_eq!(*rx.borrow(), SyncPhase::FullInit);
    }
    
    /// 测试原子性
    #[test]
    fn test_state_atomic() {
        let machine = SyncFiniteStateMachine::new();
        
        // 并发设置状态（模拟多线程）
        machine.set_state(SyncPhase::FullSyncing);
        machine.set_state(SyncPhase::IncrSyncing);
        
        // 最终状态应该是最后一次设置的值
        assert_eq!(machine.state(), SyncPhase::IncrSyncing);
    }
}