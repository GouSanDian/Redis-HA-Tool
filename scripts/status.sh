#!/bin/bash
cd "$(dirname "$0")/.."

PID_FILE="logs/redis-ha-tool.pid"

if [ ! -f "$PID_FILE" ]; then
    echo "状态: 未运行 (未找到 PID 文件)"
    exit 1
fi

PID=$(cat "$PID_FILE")

if ps -p $PID > /dev/null 2>&1; then
    echo "状态: 运行中"
    echo "PID: $PID"
    echo "启动时间: $(ps -p $PID -o lstart= 2>/dev/null)"
    echo "内存使用: $(ps -p $PID -o rss= 2>/dev/null | awk '{printf "%.1f MB\n", $1/1024}')"
    echo "CPU 使用: $(ps -p $PID -o %cpu= 2>/dev/null)%"
else
    echo "状态: 未运行 (PID: $PID 不存在)"
    exit 1
fi