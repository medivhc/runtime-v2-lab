#!/bin/bash
# resume-v06.sh — 用正常配置重启 control runner，冲刷暂停留下的待投递结果
set -u
LAB=/root/workspace/runtime-v2-lab
pkill -f '[c]onfigs/control-v06-pause.toml' 2>/dev/null
sleep 1
cd "$LAB"
setsid nohup $LAB/target/debug/runtime runner --config $LAB/configs/control.toml >> $LAB/logs/control-runner.log 2>&1 &
sleep 8
pgrep -af '[r]untime runner' | head
