#!/bin/bash
# restart-services.sh — 停掉旧进程并用最新二进制重启全部服务
set -u
LAB=/root/workspace/runtime-v2-lab
pkill -f '[r]untime serve'
pkill -f '[r]untime runner'
sleep 2
cd "$LAB"
setsid nohup bash scripts/serve-all.sh >> logs/serve-all.log 2>&1 &
sleep 6
pgrep -af 'runtime serve|runtime runner' | head
