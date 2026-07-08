#!/bin/bash
cd "$(dirname "$0")/.."

PID_FILE="logs/redis-ha-tool.pid"

if [ ! -f "$PID_FILE" ]; then
    echo "redis-ha-tool 未运行 (未找到 PID 文件)"
    exit 1
fi

PID=$(cat "$PID_FILE")

if ! ps -p $PID > /dev/null 2>&1; then
    echo "redis-ha-tool 未运行 (PID: $PID 不存在)"
    rm -f "$PID_FILE"
    exit 1
fi

echo "停止 redis-ha-tool (PID: $PID)..."
kill $PID

for i in {1..10}; do
    if ! ps -p $PID > /dev/null 2>&1; then
        echo "redis-ha-tool 已停止"
        rm -f "$PID_FILE"
        exit 0
    fi
    sleep 1
done

echo "redis-ha-tool 未能正常停止，强制终止..."
kill -9 $PID 2>/dev/null
rm -f "$PID_FILE"
echo "redis-ha-tool 已强制停止"