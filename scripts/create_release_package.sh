#!/bin/bash
# create_release_package.sh

set -e

VERSION="0.1.0"
PLATFORM="linux-x86_64"
PACKAGE_NAME="redis-ha-tool"

echo "开始编译 release 版本..."
cargo build --release
if [ $? -ne 0 ]; then
    echo "错误: 编译失败!"
    exit 1
fi
echo "编译成功!"

# 创建目录结构
mkdir -p ${PACKAGE_NAME}/bin
mkdir -p ${PACKAGE_NAME}/config
mkdir -p ${PACKAGE_NAME}/cache
mkdir -p ${PACKAGE_NAME}/logs
mkdir -p ${PACKAGE_NAME}/scripts

# 复制二进制文件
cp target/release/redis-ha-tool ${PACKAGE_NAME}/bin/
chmod +x ${PACKAGE_NAME}/bin/redis-ha-tool

# 复制配置文件
cp config/config.json ${PACKAGE_NAME}/config/config.json.example

# 复制脚本文件
cp scripts/start.sh ${PACKAGE_NAME}/scripts/
cp scripts/stop.sh ${PACKAGE_NAME}/scripts/
cp scripts/status.sh ${PACKAGE_NAME}/scripts/
chmod +x ${PACKAGE_NAME}/scripts/*.sh

# 打包
tar -czf ${PACKAGE_NAME}.tar.gz ${PACKAGE_NAME}

# 清理临时目录
rm -rf ${PACKAGE_NAME}

echo "发布包创建成功: ${PACKAGE_NAME}.tar.gz"
ls -lh ${PACKAGE_NAME}.tar.gz