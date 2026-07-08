#!/bin/bash
cd "$(dirname "$0")/.."

PID_FILE="logs/redis-ha-tool.pid"
LOG_FILE="logs/redis-ha-tool.log"

if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if ps -p $PID > /dev/null 2>&1; then
        echo "redis-ha-tool 已经在运行中 (PID: $PID)"
        exit 1
    else
        echo "发现过期的 PID 文件，清理中..."
        rm -f "$PID_FILE"
    fi
fi

echo "启动 redis-ha-tool..."
nohup ./bin/redis-ha-tool --config config/config.json > "$LOG_FILE" 2>&1 &
PID=$!
echo $PID > "$PID_FILE"

sleep 1

if ps -p $PID > /dev/null 2>&1; then
    echo "redis-ha-tool 启动成功 (PID: $PID)"
    echo "日志文件: $LOG_FILE"
else
    echo "redis-ha-tool 启动失败，请查看日志: $LOG_FILE"
    rm -f "$PID_FILE"
    exit 1
fi