#!/bin/bash
# stop-for-v06.sh — 停掉 serve-all 循环、看门狗和普通 control runner（保留 central 与 worker）
set -u
pkill -f '[s]erve-all.sh' 2>/dev/null
pkill -f '[r]ecover-watchdog' 2>/dev/null
pkill -f 'config /root/workspace/runtime-v2-lab/configs/control[.]toml' 2>/dev/null
sleep 1
pgrep -af '[r]untime serve|[r]untime runner' | head
