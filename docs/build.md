# Redis Tool 编译和打包指南

## 1. 编译步骤
### 1.1 开发版编译
**快速编译（开发调试）**：

```bash
# 编译项目（快速编译，包含调试信息）
cargo build

# 编译并运行测试
cargo test

# 编译并运行
cargo run -- --config config/config.json
```

编译产物位置：
- 二进制文件：`target/debug/redis-ha-tool`
- 库文件：`target/debug/libredis_syncer.rlib`

### 1.2 发布版编译

**优化编译（生产环境）**：

```bash
# 编译发布版本（优化性能，移除调试信息）
cargo build --release

# 运行发布版本
cargo run --release -- --config config/config.json
```

编译产物位置：
- 二进制文件：`target/release/redis-ha-tool`
- 库文件：`target/release/libredis_syncer.rlib`


### 1.3 特性编译

**启用可选特性**：

```bash
# 启用 etcd 支持（可选）
cargo build --release --features etcd

# 启用所有特性
cargo build --release --all-features
```

**可用特性**：
- `etcd`：启用 etcd 客户端支持（用于集群选举）

### 1.4 跨平台编译

**编译目标平台二进制**：

```bash
# 添加目标平台
rustup target add x86_64-unknown-linux-gnu    # Linux x64
rustup target add x86_64-apple-darwin         # macOS x64
rustup target add aarch64-apple-darwin        # macOS ARM
rustup target add x86_64-pc-windows-gnu       # Windows x64 (MinGW)

# 编译特定平台
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target x86_64-apple-darwin
cargo build --release --target x86_64-pc-windows-gnu
```

**注意**：跨平台编译可能需要对应平台的交叉编译工具链。


## 2. 打包发布
**执行打包**：

```bash
chmod +x create_release_package.sh
./create_release_package.sh
```


## 3. Docker 构建

### 3.2 构建镜像

```bash
# 构建镜像
docker build -t redis-ha-tool:0.1.0 .

# 构建带标签
docker build -t redis-ha-tool:latest -t redis-ha-tool:0.1.0 .

# 查看镜像大小
docker images redis-ha-tool
```

### 3.3 运行容器

```bash
# 创建配置文件（本地）
mkdir -p ./docker-config
cp config/config.json ./docker-config/config.json

# 运行容器
docker run -d \
    --name redis-ha-tool \
    -p 8080:8080 \
    -v $(pwd)/docker-config:/app/config \
    -v $(pwd)/docker-data:/app/data \
    -v $(pwd)/docker-logs:/app/logs \
    redis-ha-tool:0.1.0

# 查看日志
docker logs redis-ha-tool

# 健康检查
curl http://localhost:8080/health
```

### 3.4 Docker Compose

**启动服务**：

```bash
# 启动所有服务
docker-compose up -d

# 查看服务状态
docker-compose ps

# 查看日志
docker-compose logs -f redis-ha-tool

# 停止服务
docker-compose down
```

