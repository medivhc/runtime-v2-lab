#!/usr/bin/env bash
# gate.sh — 客观门禁脚本（机制一落地·仓库侧）
# 用法：gate.sh --task <任务书路径> [--contract-only]
#   在仓库根目录运行。解析任务书中的产物契约并校验；非 --contract-only 时再跑三段检查。
# 退出码：0=GATE PASS，1=GATE FAIL
set -euo pipefail

usage() {
  cat <<'EOF'
用法: gate.sh --task <任务书路径> [--contract-only]

参数:
  --task <path>       任务书 Markdown 文件路径（必需）
  --contract-only     仅做产物契约校验，跳过 fmt/clippy/test 三段检查
  -h, --help          显示此帮助

行为:
  1) 产物契约校验：解析任务书中「## 产物契约」区块下「- 必需产物：」之后的
     每个 "- " 列表项（相对路径，支持 glob）；逐项校验存在且非空
     (glob 须至少展开到一个非空文件)。缺失或空 → 记 FAIL。
  2) 三段检查（非 --contract-only 时）：
     cargo fmt --check
     cargo clippy --all-targets -- -D warnings
     cargo test --workspace
     任一失败 → 记 FAIL。
  3) 输出：逐项 "PASS/FAIL: <检查项>"，最后一行 "GATE: PASS" 或 "GATE: FAIL"。

退出码: 0=PASS, 1=FAIL
EOF
}

# 参数解析
TASK_FILE=""
CONTRACT_ONLY=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --task)
      TASK_FILE="${2:-}"
      shift 2
      ;;
    --contract-only)
      CONTRACT_ONLY=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "未知参数: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$TASK_FILE" ]]; then
  echo "错误: 必需参数 --task <任务书路径>" >&2
  usage
  exit 2
fi

# 解析任务书绝对路径（在 cd 之前）
TASK_FILE_ABS="$(cd "$(dirname "$TASK_FILE")" && pwd)/$(basename "$TASK_FILE")"
if [[ ! -f "$TASK_FILE_ABS" ]]; then
  echo "错误: 任务书不存在: $TASK_FILE" >&2
  exit 1
fi

# 确保在仓库根目录
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# 解析任务书中的产物契约
parse_artifacts() {
  local task_file="$1"
  local in_contract=false
  local in_required=false

  while IFS= read -r line; do
    # 检测 "## 产物契约" 区块开始
    if [[ "$line" =~ ^##[[:space:]]+产物契约 ]]; then
      in_contract=true
      continue
    fi

    # 离开二级标题区块
    if [[ "$in_contract" == true && "$line" =~ ^##[[:space:]]+ ]]; then
      break
    fi

    # 检测 "- 必需产物："
    if [[ "$in_contract" == true && "$line" =~ ^-[[:space:]]+必需产物[：:] ]]; then
      in_required=true
      continue
    fi

    # 解析列表项 "- 路径"
    if [[ "$in_required" == true && "$line" =~ ^[[:space:]]*-[[:space:]]+(.+) ]]; then
      local item="${BASH_REMATCH[1]}"
      # 去除行内注释
      item="${item%%#*}"
      # 去除首尾空白
      item="$(echo "$item" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
      if [[ -n "$item" ]]; then
        echo "$item"
      fi
      continue
    fi

    # 遇到非列表项且已在必需产物区，结束
    if [[ "$in_required" == true && ! "$line" =~ ^[[:space:]]*- ]]; then
      break
    fi
  done < "$task_file"
}

# 校验单个产物路径（支持 glob）
check_artifact() {
  local pattern="$1"
  local all_nonempty=true

  # 判断是否包含 glob 元字符
  if [[ "$pattern" == *[\[\*\?]* ]]; then
    # glob 模式：使用 nullglob 展开
    local saved_nullglob
    saved_nullglob=$(shopt -p nullglob 2>/dev/null || echo "unset")
    shopt -s nullglob

    local matches=()
    for f in $pattern; do
      matches+=("$f")
    done

    # 恢复 nullglob 状态
    if [[ "$saved_nullglob" == "unset" ]]; then
      shopt -u nullglob
    else
      eval "$saved_nullglob"
    fi

    if [[ ${#matches[@]} -eq 0 ]]; then
      echo "FAIL: $pattern (glob 未命中任何文件)"
      return 1
    fi

    for f in "${matches[@]}"; do
      if [[ ! -s "$f" ]]; then
        echo "FAIL: $f (存在但为空)"
        all_nonempty=false
      fi
    done
  else
    # 字面路径：直接检查文件是否存在且非空
    if [[ ! -e "$pattern" ]]; then
      echo "FAIL: $pattern (文件不存在)"
      return 1
    fi
    if [[ ! -s "$pattern" ]]; then
      echo "FAIL: $pattern (存在但为空)"
      return 1
    fi
  fi

  if [[ "$all_nonempty" == true ]]; then
    echo "PASS: $pattern"
    return 0
  else
    return 1
  fi
}

# 执行三段检查
run_checks() {
  local checks=(
    "cargo fmt --check:fmt"
    "cargo clippy --all-targets -- -D warnings:clippy"
    "cargo test --workspace:test"
  )

  local all_pass=true
  for check in "${checks[@]}"; do
    local cmd="${check%%:*}"
    local name="${check##*:}"
    if eval "$cmd" >/dev/null 2>&1; then
      echo "PASS: $name"
    else
      echo "FAIL: $name"
      all_pass=false
    fi
  done

  if [[ "$all_pass" == true ]]; then
    return 0
  else
    return 1
  fi
}

# 主流程
main() {
  local artifacts
  mapfile -t artifacts < <(parse_artifacts "$TASK_FILE_ABS")

  if [[ ${#artifacts[@]} -eq 0 ]]; then
    echo "FAIL: 任务书中未找到产物契约（## 产物契约 -> - 必需产物：列表）" >&2
    exit 1
  fi

  local overall_pass=true

  # 1. 产物契约校验
  for artifact in "${artifacts[@]}"; do
    if ! check_artifact "$artifact"; then
      overall_pass=false
    fi
  done

  # 2. 三段检查
  if [[ "$CONTRACT_ONLY" == false ]]; then
    if ! run_checks; then
      overall_pass=false
    fi
  fi

  # 最终结果
  if [[ "$overall_pass" == true ]]; then
    echo "GATE: PASS"
    exit 0
  else
    echo "GATE: FAIL"
    exit 1
  fi
}

main "$@"