//! syncer/transaction.rs - 事务状态跟踪
//!
//! 本文件实现事务状态机，跟踪 MULTI/EXEC/DISCARD 命令。

/// 事务状态枚举
///
/// 定义事务的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    /// 无事务
    No,
    
    /// 遇到 Barrier（等待事务开始）
    Barrier,
    
    /// 事务已开始（MULTI）
    Begin,
    
    /// 事务进行中
    In,
    
    /// 事务提交（EXEC）
    Commit,
}

impl Default for TransactionStatus {
    fn default() -> Self {
        TransactionStatus::No
    }
}

/// 事务状态跟踪器
///
/// 跟踪当前的事务状态，处理 MULTI/EXEC/DISCARD 命令。
pub struct Transaction {
    /// 当前状态
    status: TransactionStatus,
}

impl Transaction {
    /// 创建新的事务跟踪器
    pub fn new() -> Self {
        Transaction {
            status: TransactionStatus::No,
        }
    }
    
    /// 获取当前状态
    pub fn status(&self) -> TransactionStatus {
        self.status
    }
    
    /// 处理命令
    ///
    /// 根据命令更新事务状态。
    ///
    /// # 参数
    /// - cmd: Redis 命令（大写）
    ///
    /// # 返回
    /// 是否在事务中
    pub fn process_command(&mut self, cmd: &str) -> bool {
        match cmd {
            "MULTI" => {
                self.status = TransactionStatus::Begin;
                tracing::debug!("事务开始: MULTI");
                false
            }
            
            "EXEC" => {
                if self.status == TransactionStatus::In {
                    self.status = TransactionStatus::Commit;
                    tracing::debug!("事务提交: EXEC");
                    false
                } else {
                    tracing::warn!("EXEC 命令不在事务中");
                    false
                }
            }
            
            "DISCARD" => {
                if self.status == TransactionStatus::In || self.status == TransactionStatus::Begin {
                    self.status = TransactionStatus::No;
                    tracing::debug!("事务丢弃: DISCARD");
                    false
                } else {
                    tracing::warn!("DISCARD 命令不在事务中");
                    false
                }
            }
            
            _ => {
                // 普通命令
                if self.status == TransactionStatus::Begin {
                    // 第一个命令进入事务
                    self.status = TransactionStatus::In;
                    tracing::debug!("事务进入: {}", cmd);
                    true
                } else if self.status == TransactionStatus::In {
                    // 事务中的命令
                    true
                } else {
                    // 非事务命令
                    if self.status == TransactionStatus::Commit {
                        // EXEC 后，恢复到无事务
                        self.status = TransactionStatus::No;
                    }
                    false
                }
            }
        }
    }
    
    /// 是否在事务中
    pub fn is_in_transaction(&self) -> bool {
        matches!(self.status, TransactionStatus::Begin | TransactionStatus::In)
    }
    
    /// 是否需要提交
    pub fn need_commit(&self) -> bool {
        self.status == TransactionStatus::Commit
    }
    
    /// 重置状态
    pub fn reset(&mut self) {
        self.status = TransactionStatus::No;
    }
    
    /// 完成提交
    ///
    /// EXEC 命令处理完成后调用。
    pub fn finish_commit(&mut self) {
        self.status = TransactionStatus::No;
        tracing::debug!("事务完成");
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new()
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试事务基本流程
    #[test]
    fn test_transaction_flow() {
        let mut tx = Transaction::new();
        
        // 初始状态
        assert_eq!(tx.status(), TransactionStatus::No);
        assert!(!tx.is_in_transaction());
        
        // MULTI 命令
        tx.process_command("MULTI");
        assert_eq!(tx.status(), TransactionStatus::Begin);
        assert!(tx.is_in_transaction());
        
        // 第一个命令
        let in_tx = tx.process_command("SET");
        assert!(in_tx);
        assert_eq!(tx.status(), TransactionStatus::In);
        
        // 第二个命令
        let in_tx = tx.process_command("GET");
        assert!(in_tx);
        assert_eq!(tx.status(), TransactionStatus::In);
        
        // EXEC 命令
        tx.process_command("EXEC");
        assert_eq!(tx.status(), TransactionStatus::Commit);
        
        // 完成提交
        tx.finish_commit();
        assert_eq!(tx.status(), TransactionStatus::No);
        assert!(!tx.is_in_transaction());
    }
    
    /// 测试事务丢弃
    #[test]
    fn test_transaction_discard() {
        let mut tx = Transaction::new();
        
        // MULTI
        tx.process_command("MULTI");
        assert_eq!(tx.status(), TransactionStatus::Begin);
        
        // 命令
        tx.process_command("SET");
        assert_eq!(tx.status(), TransactionStatus::In);
        
        // DISCARD
        tx.process_command("DISCARD");
        assert_eq!(tx.status(), TransactionStatus::No);
        assert!(!tx.is_in_transaction());
    }
    
    /// 测试非事务命令
    #[test]
    fn test_non_transaction_commands() {
        let mut tx = Transaction::new();
        
        // 非 MULTI 状态下的命令
        let in_tx = tx.process_command("SET");
        assert!(!in_tx);
        assert_eq!(tx.status(), TransactionStatus::No);
        
        let in_tx = tx.process_command("GET");
        assert!(!in_tx);
        assert_eq!(tx.status(), TransactionStatus::No);
    }
    
    /// 测试 EXEC/DISCARD 在非事务中
    #[test]
    fn test_exec_discard_outside_transaction() {
        let mut tx = Transaction::new();
        
        // EXEC 在非事务中
        tx.process_command("EXEC");
        assert_eq!(tx.status(), TransactionStatus::No);
        
        // DISCARD 在非事务中
        tx.process_command("DISCARD");
        assert_eq!(tx.status(), TransactionStatus::No);
    }
    
    /// 测试重置
    #[test]
    fn test_transaction_reset() {
        let mut tx = Transaction::new();
        
        // 进入事务
        tx.process_command("MULTI");
        tx.process_command("SET");
        assert_eq!(tx.status(), TransactionStatus::In);
        
        // 重置
        tx.reset();
        assert_eq!(tx.status(), TransactionStatus::No);
    }
    
    /// 测试完整事务周期
    #[test]
    fn test_full_transaction_cycle() {
        let mut tx = Transaction::new();
        
        // 开始事务
        assert!(!tx.is_in_transaction());
        
        // MULTI
        tx.process_command("MULTI");
        assert!(tx.is_in_transaction());
        
        // 多个命令
        for cmd in ["SET", "GET", "DEL"] {
            let in_tx = tx.process_command(cmd);
            assert!(in_tx);
        }
        
        // EXEC
        tx.process_command("EXEC");
        assert!(tx.need_commit());
        
        // 完成提交后的命令
        tx.finish_commit();
        let in_tx = tx.process_command("GET");
        assert!(!in_tx);
        assert!(!tx.is_in_transaction());
    }
}