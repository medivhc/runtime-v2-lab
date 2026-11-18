#!/usr/bin/env bash
# gate-selftest.sh — gate.sh 自测脚本
# 用临时目录构造任务书与产物夹具，只调 gate.sh --contract-only 验证退出码与输出
# 场景：契约全满足、缺产物、空产物、glob 未命中
# 最后输出 SELFTEST: PASS/FAIL
set -eu

GATE_SCRIPT="$(cd "$(dirname "$0")" && pwd)/gate.sh"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEST_ROOT="$REPO_ROOT/.gate-selftest-$(date +%s)-$$"
SELFTEST_PASS=true

cleanup() {
  rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

mkdir -p "$TEST_ROOT"

# Helper: get relative path from repo root
relpath() {
  local path="$1"
  echo "${path#$REPO_ROOT/}"
}

assert_exit_code() {
  local expected="$1"
  local actual="$2"
  local desc="$3"
  if [[ "$expected" -ne "$actual" ]]; then
    echo "  ASSERT FAIL: $desc (expected exit $expected, got $actual)"
    SELFTEST_PASS=false
  else
    echo "  ASSERT PASS: $desc (exit $actual)"
  fi
}

assert_output_contains() {
  local output="$1"
  local pattern="$2"
  local desc="$3"
  if echo "$output" | grep -qF "$pattern"; then
    echo "  ASSERT PASS: $desc (output contains '$pattern')"
  else
    echo "  ASSERT FAIL: $desc (output missing '$pattern')"
    SELFTEST_PASS=false
  fi
}

# 场景 1：契约全满足
echo "=== 场景 1：契约全满足 ==="
SCENARIO1="$TEST_ROOT/scenario1"
mkdir -p "$SCENARIO1/artifacts"
echo "content" > "$SCENARIO1/artifacts/file1.txt"
echo "content" > "$SCENARIO1/artifacts/file2.md"
ART1="$(relpath "$SCENARIO1/artifacts/file1.txt")"
ART2="$(relpath "$SCENARIO1/artifacts/file2.md")"
cat > "$SCENARIO1/task.md" <<EOF
# 任务书

## 产物契约
- 必需产物：
  - $ART1
  - $ART2
EOF

# Run gate.sh and capture both output and exit code
run_gate_capture() {
  local task_file="$1"
  local output
  local rc
  output=$(bash "$GATE_SCRIPT" --task "$task_file" --contract-only 2>&1)
  rc=$?
  echo "$output"
  return $rc
}

set +e
run_gate_capture "$SCENARIO1/task.md"
exit_code=$?
set -e
set +e
output=$(bash "$GATE_SCRIPT" --task "$SCENARIO1/task.md" --contract-only 2>&1)
set -e
echo "$output"
assert_exit_code 0 $exit_code "场景1 退出码应为 0"
assert_output_contains "$output" "PASS: $ART1" "场景1 file1.txt PASS"
assert_output_contains "$output" "PASS: $ART2" "场景1 file2.md PASS"
assert_output_contains "$output" "GATE: PASS" "场景1 GATE PASS"

# 场景 2：缺产物
echo ""
echo "=== 场景 2：缺产物 ==="
SCENARIO2="$TEST_ROOT/scenario2"
mkdir -p "$SCENARIO2/artifacts"
echo "content" > "$SCENARIO2/artifacts/file1.txt"
# file2.md 故意不创建
ART1="$(relpath "$SCENARIO2/artifacts/file1.txt")"
ART2="$(relpath "$SCENARIO2/artifacts/file2.md")"
cat > "$SCENARIO2/task.md" <<EOF
# 任务书

## 产物契约
- 必需产物：
  - $ART1
  - $ART2
EOF

set +e
run_gate_capture "$SCENARIO2/task.md"
exit_code=$?
set -e
set +e
output=$(bash "$GATE_SCRIPT" --task "$SCENARIO2/task.md" --contract-only 2>&1)
set -e
echo "$output"
assert_exit_code 1 $exit_code "场景2 退出码应为 1"
assert_output_contains "$output" "PASS: $ART1" "场景2 file1.txt PASS"
assert_output_contains "$output" "FAIL: $ART2" "场景2 file2.md FAIL"
assert_output_contains "$output" "GATE: FAIL" "场景2 GATE FAIL"

# 场景 3：空产物
echo ""
echo "=== 场景 3：空产物 ==="
SCENARIO3="$TEST_ROOT/scenario3"
mkdir -p "$SCENARIO3/artifacts"
echo "content" > "$SCENARIO3/artifacts/file1.txt"
touch "$SCENARIO3/artifacts/empty.txt"  # 空文件
ART1="$(relpath "$SCENARIO3/artifacts/file1.txt")"
ART2="$(relpath "$SCENARIO3/artifacts/empty.txt")"
cat > "$SCENARIO3/task.md" <<EOF
# 任务书

## 产物契约
- 必需产物：
  - $ART1
  - $ART2
EOF

set +e
run_gate_capture "$SCENARIO3/task.md"
exit_code=$?
set -e
set +e
output=$(bash "$GATE_SCRIPT" --task "$SCENARIO3/task.md" --contract-only 2>&1)
set -e
echo "$output"
assert_exit_code 1 $exit_code "场景3 退出码应为 1"
assert_output_contains "$output" "PASS: $ART1" "场景3 file1.txt PASS"
assert_output_contains "$output" "FAIL: $ART2" "场景3 empty.txt FAIL"
assert_output_contains "$output" "GATE: FAIL" "场景3 GATE FAIL"

# 场景 4：glob 未命中
echo ""
echo "=== 场景 4：glob 未命中 ==="
SCENARIO4="$TEST_ROOT/scenario4"
mkdir -p "$SCENARIO4/artifacts"
echo "content" > "$SCENARIO4/artifacts/file1.txt"
# 故意没有 *.log 文件
ART1="$(relpath "$SCENARIO4/artifacts/file1.txt")"
ART_GLOB="$(relpath "$SCENARIO4/artifacts/*.log")"
cat > "$SCENARIO4/task.md" <<EOF
# 任务书

## 产物契约
- 必需产物：
  - $ART1
  - $ART_GLOB
EOF

set +e
run_gate_capture "$SCENARIO4/task.md"
exit_code=$?
set -e
set +e
output=$(bash "$GATE_SCRIPT" --task "$SCENARIO4/task.md" --contract-only 2>&1)
set -e
echo "$output"
assert_exit_code 1 $exit_code "场景4 退出码应为 1"
assert_output_contains "$output" "PASS: $ART1" "场景4 file1.txt PASS"
assert_output_contains "$output" "FAIL: $ART_GLOB" "场景4 *.log FAIL"
assert_output_contains "$output" "GATE: FAIL" "场景4 GATE FAIL"

# 额外场景：glob 匹配多个文件，其中有空文件
echo ""
echo "=== 场景 5：glob 匹配多文件含空文件 ==="
SCENARIO5="$TEST_ROOT/scenario5"
mkdir -p "$SCENARIO5/artifacts"
echo "content" > "$SCENARIO5/artifacts/a.log"
touch "$SCENARIO5/artifacts/b.log"  # 空文件
ART_GLOB="$(relpath "$SCENARIO5/artifacts/*.log")"
cat > "$SCENARIO5/task.md" <<EOF
# 任务书

## 产物契约
- 必需产物：
  - $ART_GLOB
EOF

set +e
run_gate_capture "$SCENARIO5/task.md"
exit_code=$?
set -e
set +e
output=$(bash "$GATE_SCRIPT" --task "$SCENARIO5/task.md" --contract-only 2>&1)
set -e
echo "$output"
assert_exit_code 1 $exit_code "场景5 退出码应为 1"
# glob 展开后会显示具体的空文件名，而非 glob 模式
assert_output_contains "$output" "FAIL: " "场景5 有 FAIL 输出"
assert_output_contains "$output" "b.log" "场景5 空文件 b.log 被检出"
assert_output_contains "$output" "GATE: FAIL" "场景5 GATE FAIL"

# 最终结果
echo ""
echo "=== 自测汇总 ==="
if [[ "$SELFTEST_PASS" == true ]]; then
  echo "SELFTEST: PASS"
  exit 0
else
  echo "SELFTEST: FAIL"
  exit 1
fi