#!/usr/bin/env bash
# env.sh — runtime-v2-lab 全部进程共用的环境基线（source 本文件；兼容 sh/dash）
set -eu
(set -o pipefail) 2>/dev/null && set -o pipefail || true
export LAB=/root/workspace/runtime-v2-lab
export PATH="/root/workspace/.local/ccwrap:/root/workspace/.local/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/root/workspace/.local/opencode:$PATH"
export RUSTUP_HOME=/root/workspace/.local/rustup
export CARGO_HOME=/root/workspace/.cargo
export RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu
# 沙箱无系统 gcc：cc/ar 由 clang 包装器提供（ccwrap/cc→clang，ccwrap/ar→/usr/bin/ar）
export CC=clang
export AR=ar
# 模型网关经 vps 的本地隧道端口
export GATEWAY_URL="http://127.0.0.1:13425/v1"
# 节点服务密码（测试用，落本地密钥文件）
export OC_CONTROL_PASSWORD="$(cat $LAB/secrets/oc-control.pass 2>/dev/null || true)"
export OC_WORKER_A_PASSWORD="$(cat $LAB/secrets/oc-worker-a.pass 2>/dev/null || true)"
export OC_WORKER_B_PASSWORD="$(cat $LAB/secrets/oc-worker-b.pass 2>/dev/null || true)"
export OC_GATEWAY_KEY="$(jq -r .apiKey $LAB/secrets/magpie-provider.json 2>/dev/null || true)"
