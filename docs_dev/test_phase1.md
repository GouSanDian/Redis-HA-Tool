# 阶段一功能测试指南

## 前置条件

### 1. 安装 Rust 工具链

```bash
# 安装 rustup（Rust 版本管理器）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 添加 Cargo 到 PATH（重启终端或执行）
source $HOME/.cargo/env

# 验证安装
rustc --version
cargo --version
```

### 2. 系统要求

- Rust 1.75+ (edition 2021)
- Linux / macOS / Windows
- 无需 Redis 环境（阶段一仅测试基础功能）

## 编译验证

### 1. 编译项目

```bash
cd /home/haotong/code/redis-ha-tool-rust

# 编译（首次编译会下载依赖）
cargo build

# 编译 Release 版本（优化编译）
cargo build --release
```

### 2. 运行单元测试

```bash
# 运行所有单元测试
cargo test

# 运行特定模块测试
cargo test --lib

# 显示测试输出
cargo test -- --nocapture

# 运行特定测试
cargo test test_encode_simple_string
cargo test test_decode_simple_string
cargo test test_config_from_yaml
```

### 3. 运行程序验证

```bash
# 使用示例配置运行（JSON 格式）
cargo run -- --config config/config.json

# 使用示例配置运行（YAML 格式）
cargo run -- --config config/config.yaml

# 预期输出：
# - 版本信息：redis-ha-tool 0.1.0
# - 配置文件加载成功日志
# - 源 Redis 和目标 Redis 配置信息
# - "阶段一验证完成，程序正常退出"
```

## 功能测试验证清单

### TC-1.1 配置系统测试

| 测试项 | 命令 | 预期结果 |
|--------|------|----------|
| 加载完整配置文件（JSON） | `cargo test test_load_config_from_json` | PASS |
| 加载完整配置文件（YAML） | `cargo test test_load_config_from_yaml` | PASS |
| 加载最小配置文件（JSON） | `cargo test test_load_minimal_json_config` | PASS |
| 加载最小配置文件（YAML） | `cargo test test_load_minimal_config` | PASS |
| 加载不存在的配置文件 | `cargo test test_load_nonexistent_config` | PASS（返回错误） |
| Redis 类型解析 | `cargo test test_redis_type_default` | PASS |
| 过滤配置解析 | 查看 `test_load_config_from_json` 输出 | PASS |
| Replay 配置默认值 | `cargo test test_replay_config_default` | PASS |
| 常量值验证 | `cargo test test_constants_not_empty` | PASS |

### TC-1.2 RESP 编码器测试

| 测试项 | 命令 | 预期结果 |
|--------|------|----------|
| 编码简单字符串 | `cargo test test_encode_simple_string` | PASS（`+OK\r\n`） |
| 编码错误 | `cargo test test_encode_error` | PASS（`-ERR...\r\n`） |
| 编码整数 | `cargo test test_encode_integer_positive` | PASS（`:42\r\n`） |
| 编码负整数 | `cargo test test_encode_integer_negative` | PASS（`:-1\r\n`） |
| 编码批量字符串 | `cargo test test_encode_bulk_string` | PASS（`$5\r\nhello\r\n`） |
| 编码空批量字符串 | `cargo test test_encode_bulk_string_empty` | PASS（`$0\r\n\r\n`） |
| 编码 Null | `cargo test test_encode_null` | PASS（`$-1\r\n`） |
| 编码空数组 | `cargo test test_encode_array_empty` | PASS（`*0\r\n`） |
| 编码嵌套数组 | `cargo test test_nested_array` | PASS |
| 编码嵌套 Null 数组 | `cargo test test_encode_array_with_null` | PASS |

### TC-1.3 RESP 解码器测试

| 测试项 | 命令 | 预期结果 |
|--------|------|----------|
| 解码简单字符串 | `cargo test test_decode_simple_string` | PASS |
| 解码错误 | `cargo test test_decode_error` | PASS |
| 解码整数 | `cargo test test_decode_integer_positive` | PASS |
| 解码批量字符串 | `cargo test test_decode_bulk_string` | PASS |
| 解码 Null 批量字符串 | `cargo test test_decode_null_bulk_string` | PASS |
| 解码数组 | `cargo test test_decode_array_multiple_elements` | PASS |
| 解码空数组 | `cargo test test_decode_array_empty` | PASS |
| 解码 Null 数组 | `cargo test test_decode_null_array` | PASS |
| 解码嵌套数组 | `cargo test test_decode_nested_array` | PASS |
| 不完整数据 | `cargo test test_decode_incomplete_data` | PASS（返回 None） |
| 多条消息连续 | `cargo test test_decode_multiple_messages` | PASS |
| offset 跟踪 | `cargo test test_offset_tracking` | PASS |
| 非法类型标记 | `cargo test test_invalid_type_marker` | PASS（返回错误） |
| 非法长度 | `cargo test test_invalid_length` | PASS（返回错误） |

### TC-1.4 日志系统测试

| 测试项 | 命令 | 预期结果 |
|--------|------|----------|
| 初始化 stdout 日志 | `cargo test test_init_logging_stdout` | PASS |
| 初始化文件日志 | `cargo test test_init_logging_file` | PASS |
| 动态调整日志级别 | `cargo test test_set_log_level` | PASS |
| 无效日志级别 | `cargo test test_set_invalid_log_level` | PASS（返回错误） |

### TC-1.5 工具函数测试

| 测试项 | 命令 | 预期结果 |
|--------|------|----------|
| FNV hash 空数据 | `cargo test test_fnv_hash_empty` | PASS |
| FNV hash 已知值 | `cargo test test_fnv_hash_known` | PASS |
| FNV hash 一致性 | `cargo test test_fnv_hash_consistency` | PASS |
| 哈希范围映射 | `cargo test test_fnv_hash_range` | PASS |

## 测试输出示例

### 成功输出示例

```
running 85 tests
test error::tests::test_config_error_messages ... ok
test error::tests::test_resp_error_messages ... ok
test error::tests::test_sync_error_messages ... ok
test error::tests::test_io_error_conversion ... ok
test config::tests::test_redis_type_default ... ok
test config::tests::test_slot_range_contains ... ok
test config::tests::test_replay_config_default ... ok
test config::tests::test_log_config_valid_level ... ok
test config::tests::test_log_config_invalid_level ... ok
test config::tests::test_load_config_from_yaml ... ok
test config::tests::test_load_minimal_config ... ok
test config::tests::test_load_nonexistent_config ... ok
test config::tests::test_redis_config_empty_addresses ... ok
test config::tests::test_input_config_invalid_rdb_parallel ... ok
test config::constants::tests::test_constants_not_empty ... ok
test config::constants::tests::test_prefix_format ... ok
test config::constants::tests::test_default_values_reasonable ... ok
test config::constants::tests::test_no_route_commands ... ok
test protocol::resp::value::tests::test_simple_string ... ok
test protocol::resp::value::tests::test_error ... ok
test protocol::resp::value::tests::test_integer ... ok
test protocol::resp::value::tests::test_bulk_string ... ok
test protocol::resp::value::tests::test_array ... ok
test protocol::resp::value::tests::test_null ... ok
test protocol::resp::value::tests::test_nested_array ... ok
test protocol::resp::value::tests::test_display ... ok
test protocol::resp::value::tests::test_as_command ... ok
test protocol::resp::value::tests::test_as_command_invalid_type ... ok
test protocol::resp::value::tests::test_as_command_invalid_first_element ... ok
test protocol::resp::value::tests::test_bulk_string_from_bytes ... ok
test protocol::resp::value::tests::test_equality ... ok
test protocol::resp::encoder::tests::test_encode_simple_string ... ok
test protocol::resp::encoder::tests::test_encode_error ... ok
test protocol::resp::encoder::tests::test_encode_integer_positive ... ok
test protocol::resp::encoder::tests::test_encode_integer_negative ... ok
test protocol::resp::encoder::tests::test_encode_bulk_string ... ok
test protocol::resp::encoder::tests::test_encode_bulk_string_empty ... ok
test protocol::resp::encoder::tests::test_encode_null ... ok
test protocol::resp::encoder::tests::test_encode_array_empty ... ok
test protocol::resp::encoder::tests::test_encode_array_single_element ... ok
test protocol::resp::encoder::tests::test_encode_array_multiple_elements ... ok
test protocol::resp::encoder::tests::test_encode_nested_array ... ok
test protocol::resp::encoder::tests::test_encode_array_with_null ... ok
test protocol::resp::encoder::tests::test_encode_mixed_array ... ok
test protocol::resp::encoder::tests::test_encode_batch ... ok
test protocol::resp::encoder::tests::test_encode_to_vec ... ok
test protocol::resp::encoder::tests::test_encode_command_bytes ... ok
test protocol::resp::encoder::tests::test_encode_command_str ... ok
test protocol::resp::encoder::tests::test_encode_command_no_args ... ok
test protocol::resp::encoder::tests::test_encode_special_characters ... ok
test protocol::resp::encoder::tests::test_encode_binary_data ... ok
test protocol::resp::decoder::tests::test_decode_simple_string ... ok
test protocol::resp::decoder::tests::test_decode_error ... ok
test protocol::resp::decoder::tests::test_decode_integer_positive ... ok
test protocol::resp::decoder::tests::test_decode_integer_negative ... ok
test protocol::resp::decoder::tests::test_decode_bulk_string ... ok
test protocol::resp::decoder::tests::test_decode_bulk_string_empty ... ok
test protocol::resp::decoder::tests::test_decode_null_bulk_string ... ok
test protocol::resp::decoder::tests::test_decode_array_empty ... ok
test protocol::resp::decoder::tests::test_decode_array_single_element ... ok
test protocol::resp::decoder::tests::test_decode_array_multiple_elements ... ok
test protocol::resp::decoder::tests::test_decode_null_array ... ok
test protocol::resp::decoder::tests::test_decode_nested_array ... ok
test protocol::resp::decoder::tests::test_decode_incomplete_data ... ok
test protocol::resp::decoder::tests::test_decode_multiple_messages ... ok
test protocol::resp::decoder::tests::test_offset_tracking ... ok
test protocol::resp::decoder::tests::test_invalid_type_marker ... ok
test protocol::resp::decoder::tests::test_invalid_integer_format ... ok
test protocol::resp::decoder::decoder::tests::test_invalid_length ... ok
test protocol::resp::decoder::tests::test_buffer_overflow ... ok
test protocol::resp::decoder::tests::test_decoder_reset ... ok
test protocol::resp::decoder::tests::test_decoder_default ... ok
test protocol::resp::decoder::tests::test_decode_array_with_null ... ok
test protocol::resp::decoder::tests::test_decode_mixed_array ... ok
test protocol::resp::decoder::tests::test_buffer_size ... ok
test protocol::resp::decoder::tests::test_incremental_feed ... ok
test utils::log::tests::test_init_logging_stdout ... ok
test utils::log::tests::test_set_log_level ... ok
test utils::log::tests::test_set_invalid_log_level ... ok
test utils::log::tests::test_init_logging_file ... ok
test utils::log::tests::test_init_logging_default ... ok
test utils::hash::tests::test_fnv_hash_empty ... ok
test utils::hash::tests::test_fnv_hash_known ... ok
test utils::hash::tests::test_fnv_hash_consistency ... ok
test utils::hash::tests::test_fnv_hash_range ... ok
test utils::hash::tests::test_fnv_hash_range_zero ... ok
test utils::hash::tests::test_distribution ... ok
test utils::hash::tests::test_fnv_hash_str ... ok
test utils::hash::tests::test_fnv_hash_str_range ... ok
test utils::hash::tests::test_prefix_keys_different_workers ... ok
test utils::hash::tests::test_large_scale_distribution ... ok
test utils::hash::tests::test_special_characters ... ok

test result: ok. 85 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## 预期测试统计

| 模块 | 测试数量 | 覆盖内容 |
|------|----------|----------|
| error | 4 | 错误消息、类型转换 |
| config | 14 | 配置加载、验证、默认值、常量 |
| protocol::resp::value | 11 | RESP 值类型、转换、判断 |
| protocol::resp::encoder | 17 | RESP 编码（所有类型） |
| protocol::resp::decoder | 20 | RESP 解码、流式解析、错误处理 |
| utils::log | 5 | 日志初始化、动态调整 |
| utils::hash | 11 | FNV 哈希、分布均匀性 |
| **总计** | **82+** | 阶段一全部功能 |

## 验证通过标准

阶段一功能验证通过的判定标准：

1. ✅ **编译成功**：`cargo build` 无错误
2. ✅ **测试全部通过**：`cargo test` 显示 `test result: ok`
3. ✅ **程序运行正常**：`cargo run -- --config config/config.yaml` 正常退出
4. ✅ **配置加载成功**：程序日志显示配置文件加载成功
5. ✅ **RESP 编解码验证**：编解码测试全部通过，覆盖所有类型
6. ✅ **日志系统正常**：日志初始化和动态调整测试通过
7. ✅ **工具函数正常**：FNV 哈希测试通过

## 下一步

阶段一完成后，可以继续进行阶段二开发：

- 实现本地文件存储引擎（Storer/Reader/Writer）
- 实现过滤系统（Trie/RangeList/RedisKeyFilter）
- 实现 Checkpoint 管理器

详见 `dev_plan.md` 阶段二部分。