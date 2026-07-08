# 多阶段构建，优化镜像大小

# 阶段 1：编译
FROM rust:1.75 AS builder

WORKDIR /app

# 安装编译依赖
RUN apt update && apt install -y \
    protobuf-compiler \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# 复制源代码
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY proto ./proto
COPY build.rs ./
COPY config ./config

# 编译发布版本
RUN cargo build --release

# 阶段 2：运行镜像
FROM debian:bullseye-slim

WORKDIR /app

# 安装运行依赖
RUN apt update && apt install -y \
    ca-certificates \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

# 从编译阶段复制二进制
COPY --from=builder /app/target/release/redis-ha-tool /usr/local/bin/

# 复制配置文件
COPY config/config.json /app/config/config.json.example

# 创建日志和数据目录
RUN mkdir -p /app/logs /app/data

# 暴露端口
EXPOSE 8080 9090

# 健康检查
HEALTHCHECK --interval=30s --timeout=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# 启动命令
ENTRYPOINT ["redis-ha-tool"]
CMD ["--config", "/app/config/config.json"]