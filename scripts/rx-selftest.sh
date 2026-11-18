#!/usr/bin/env bash
# rx-selftest.sh - Self-test for rx-draft.sh
# Tests three scenarios:
# 1. With findings -> generates 1-2 prescriptions
# 2. No findings -> outputs "无处方建议"
# 3. Invalid action rejected (though our script maps to valid actions)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RX_DRAFT="$SCRIPT_DIR/rx-draft.sh"

if [[ ! -x "$RX_DRAFT" ]]; then
    echo "Error: $RX_DRAFT not found or not executable" >&2
    exit 1
fi

TEST_COUNT=0
PASS_COUNT=0
FAIL_COUNT=0

run_test() {
    local test_name="$1"
    local test_fn="$2"
    
    TEST_COUNT=$((TEST_COUNT + 1))
    echo "=== Test $TEST_COUNT: $test_name ==="
    
    local temp_dir
    temp_dir=$(mktemp -d)
    
    # Run test function with temp_dir as argument
    if "$test_fn" "$temp_dir"; then
        echo "Result: PASS"
        PASS_COUNT=$((PASS_COUNT + 1))
    else
        echo "Result: FAIL"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
    
    # Cleanup
    rm -rf "$temp_dir"
    echo ""
}

# Test 1: With findings -> generates 1-2 prescriptions
test_with_findings() {
    local temp_dir="$1"
    
    local task_book="$temp_dir/task.md"
    local evidence_dir="$temp_dir/evidence"
    local output="$temp_dir/rx-draft.md"
    
    mkdir -p "$evidence_dir"
    
    # Create task book
    cat > "$task_book" <<'EOF'
# Task Book: Test Task

## Objective
Fix the evidence CLI download issue.

## Acceptance Criteria
- CLI downloads evidence correctly
- Hash verification passes
EOF
    
    # Create review verdict with findings
    cat > "$evidence_dir/review-test-verdict.txt" <<'EOF'
VERDICT: APPROVE

Findings:

MAJOR: Hash verification mismatch in DELIVERY report - reported hashes differ from evidence table

MINOR: 
1. Config format inconsistency in central.toml (pre-existing)
2. Missing machine gate check for hash consistency verification
EOF
    
    # Create DELIVERY with rework note
    cat > "$evidence_dir/DELIVERY-test.md" <<'EOF'
# DELIVERY Test

## Summary
Task completed.

## Rework Items
遗留: Need to add automated hash verification gate check in CI pipeline.
EOF
    
    # Run rx-draft.sh
    "$RX_DRAFT" --task "$task_book" --evidence "$evidence_dir" --out "$output"
    
    # Verify output
    if [[ ! -f "$output" ]]; then
        echo "FAIL: Output file not created"
        return 1
    fi
    
    local content
    content=$(cat "$output")
    echo "Output content:"
    echo "$content"
    echo "---"
    
    # Check header
    if ! grep -q "处方草稿，须人工确认后应用，脚本不自动修改任何流程文件" "$output"; then
        echo "FAIL: Missing header disclaimer"
        return 1
    fi
    
    # Check has prescriptions (not "无处方建议")
    if grep -q "无处方建议" "$output"; then
        echo "FAIL: Should have prescriptions but got '无处方建议'"
        return 1
    fi
    
    # Check has action: field with valid action
    if ! grep -q "action:" "$output"; then
        echo "FAIL: Missing action field"
        return 1
    fi
    
    # Check action is one of valid enum
    local actions
    actions=$(grep "action:" "$output" | sed 's/.*action: *//')
    for action in $actions; do
        case "$action" in
            update_task_template|add_gate_check|append_review_prompt)
                ;;
            *)
                echo "FAIL: Invalid action: $action"
                return 1
                ;;
        esac
    done
    
    # Check rationale field exists
    if ! grep -q "rationale:" "$output"; then
        echo "FAIL: Missing rationale field"
        return 1
    fi
    
    # Check status: DRAFT
    if ! grep -q "status: DRAFT" "$output"; then
        echo "FAIL: Missing status: DRAFT"
        return 1
    fi
    
    # Check max 2 prescriptions
    local prescription_count
    prescription_count=$(grep -c "action:" "$output" || true)
    if [[ $prescription_count -gt 2 ]]; then
        echo "FAIL: More than 2 prescriptions ($prescription_count)"
        return 1
    fi
    
    if [[ $prescription_count -lt 1 ]]; then
        echo "FAIL: Less than 1 prescription ($prescription_count)"
        return 1
    fi
    
    echo "PASS: Generated $prescription_count prescription(s) with valid actions"
    return 0
}

# Test 2: No findings -> outputs "无处方建议"
test_no_findings() {
    local temp_dir="$1"
    
    local task_book="$temp_dir/task.md"
    local evidence_dir="$temp_dir/evidence"
    local output="$temp_dir/rx-draft.md"
    
    mkdir -p "$evidence_dir"
    
    # Create task book
    cat > "$task_book" <<'EOF'
# Task Book: Clean Task

## Objective
Simple task with no issues.

## Acceptance Criteria
- All checks pass
EOF
    
    # Create review verdict with NO findings
    cat > "$evidence_dir/review-clean-verdict.txt" <<'EOF'
VERDICT: APPROVE

Findings:

MAJOR: None — all acceptance criteria satisfied.

MINOR: None
EOF
    
    # Create DELIVERY with no rework
    cat > "$evidence_dir/DELIVERY-clean.md" <<'EOF'
# DELIVERY Clean

## Summary
Task completed successfully with no issues.
All items addressed.
EOF
    
    # Run rx-draft.sh
    "$RX_DRAFT" --task "$task_book" --evidence "$evidence_dir" --out "$output"
    
    # Verify output
    if [[ ! -f "$output" ]]; then
        echo "FAIL: Output file not created"
        return 1
    fi
    
    local content
    content=$(cat "$output")
    echo "Output content:"
    echo "$content"
    echo "---"
    
    # Check header
    if ! grep -q "处方草稿，须人工确认后应用，脚本不自动修改任何流程文件" "$output"; then
        echo "FAIL: Missing header disclaimer"
        return 1
    fi
    
    # Check outputs "无处方建议"
    if ! grep -q "无处方建议" "$output"; then
        echo "FAIL: Expected '无处方建议' but not found"
        return 1
    fi
    
    # Should NOT have action fields
    if grep -q "action:" "$output"; then
        echo "FAIL: Should not have action fields when no findings"
        return 1
    fi
    
    echo "PASS: Correctly outputs '无处方建议' for clean case"
    return 0
}

# Test 3: Invalid action rejected (our script maps to valid actions, but test the enum validation)
test_invalid_action_rejected() {
    local temp_dir="$1"
    
    local task_book="$temp_dir/task.md"
    local evidence_dir="$temp_dir/evidence"
    local output="$temp_dir/rx-draft.md"
    
    mkdir -p "$evidence_dir"
    
    # Create task book
    cat > "$task_book" <<'EOF'
# Task Book: Edge Case Task

## Objective
Test action mapping.
EOF
    
    # Create review verdict with text that doesn't match any keyword
    # This should map to default action (append_review_prompt)
    cat > "$evidence_dir/review-edge-verdict.txt" <<'EOF'
VERDICT: APPROVE

Findings:

MINOR: Some completely unrelated text that matches no keywords xyzabc123
EOF
    
    # No DELIVERY file
    
    # Run rx-draft.sh
    "$RX_DRAFT" --task "$task_book" --evidence "$evidence_dir" --out "$output"
    
    # Verify output
    if [[ ! -f "$output" ]]; then
        echo "FAIL: Output file not created"
        return 1
    fi
    
    local content
    content=$(cat "$output")
    echo "Output content:"
    echo "$content"
    echo "---"
    
    # Check header
    if ! grep -q "处方草稿，须人工确认后应用，脚本不自动修改任何流程文件" "$output"; then
        echo "FAIL: Missing header disclaimer"
        return 1
    fi
    
    # Check action is valid (should be append_review_prompt as default)
    local action
    action=$(grep "action:" "$output" | sed 's/.*action: *//' | head -1)
    
    case "$action" in
        update_task_template|add_gate_check|append_review_prompt)
            echo "PASS: Invalid input text correctly mapped to valid action: $action"
            return 0
            ;;
        *)
            echo "FAIL: Action '$action' is not a valid enum value"
            return 1
            ;;
    esac
}

# Test 4: Exit code verification (success cases)
test_exit_codes() {
    local temp_dir="$1"
    
    local task_book="$temp_dir/task.md"
    local evidence_dir="$temp_dir/evidence"
    local output="$temp_dir/rx-draft.md"
    
    mkdir -p "$evidence_dir"
    
    cat > "$task_book" <<'EOF'
# Task Book
EOF
    
    cat > "$evidence_dir/review-verdict.txt" <<'EOF'
VERDICT: APPROVE
Findings:
MINOR: Test finding
EOF
    
    # Test successful execution returns 0
    if "$RX_DRAFT" --task "$task_book" --evidence "$evidence_dir" --out "$output"; then
        echo "PASS: Successful execution returns exit code 0"
        return 0
    else
        echo "FAIL: Successful execution returned non-zero exit code"
        return 1
    fi
}

# Test 5: Error handling - missing arguments
test_missing_args() {
    local temp_dir="$1"
    
    # Test missing --task
    if "$RX_DRAFT" --evidence "$temp_dir" --out "$temp_dir/out.md" 2>/dev/null; then
        echo "FAIL: Should fail with missing --task"
        return 1
    fi
    
    # Test missing --evidence
    if "$RX_DRAFT" --task "$temp_dir/task.md" --out "$temp_dir/out.md" 2>/dev/null; then
        echo "FAIL: Should fail with missing --evidence"
        return 1
    fi
    
    # Test missing --out
    if "$RX_DRAFT" --task "$temp_dir/task.md" --evidence "$temp_dir" 2>/dev/null; then
        echo "FAIL: Should fail with missing --out"
        return 1
    fi
    
    echo "PASS: Correctly rejects missing arguments"
    return 0
}

# Test 6: Error handling - non-existent paths
test_nonexistent_paths() {
    local temp_dir="$1"
    
    local output="$temp_dir/out.md"
    
    # Test non-existent task book
    if "$RX_DRAFT" --task "$temp_dir/nonexistent.md" --evidence "$temp_dir" --out "$output" 2>/dev/null; then
        echo "FAIL: Should fail with non-existent task book"
        return 1
    fi
    
    # Test non-existent evidence dir
    mkdir -p "$temp_dir/task.md"
    if "$RX_DRAFT" --task "$temp_dir/task.md" --evidence "$temp_dir/nonexistent" --out "$output" 2>/dev/null; then
        echo "FAIL: Should fail with non-existent evidence dir"
        return 1
    fi
    
    echo "PASS: Correctly rejects non-existent paths"
    return 0
}

# Run all tests
echo "Starting rx-draft.sh self-tests..."
echo ""

run_test "With findings generates 1-2 prescriptions" test_with_findings
run_test "No findings outputs 无处方建议" test_no_findings
run_test "Invalid action text maps to valid enum" test_invalid_action_rejected
run_test "Exit code 0 on success" test_exit_codes
run_test "Missing arguments rejected" test_missing_args
run_test "Non-existent paths rejected" test_nonexistent_paths

# Summary
echo "=== SELFTEST SUMMARY ==="
echo "Total tests: $TEST_COUNT"
echo "Passed: $PASS_COUNT"
echo "Failed: $FAIL_COUNT"

if [[ $FAIL_COUNT -eq 0 ]]; then
    echo "SELFTEST: PASS"
    exit 0
else
    echo "SELFTEST: FAIL"
    exit 1
fi