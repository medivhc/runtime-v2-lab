#!/usr/bin/env bash
# rx-draft.sh - Generate prescription draft from task book and evidence
# Usage: rx-draft.sh --task <任务书路径> --evidence <evidence目录> --out <输出路径>
# 输出：rx-draft.md（处方草稿，须人工确认后应用，脚本不自动修改任何流程文件）

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: rx-draft.sh --task <task_book_path> --evidence <evidence_dir> --out <output_path>

Generate prescription draft from task book and evidence archive.
Extracts 0-2 improvement prescriptions from review findings and DELIVERY rework records.

Arguments:
  --task      Path to the task book (markdown file)
  --evidence  Directory containing review-*-verdict.txt and DELIVERY*.md files
  --out       Output path for rx-draft.md

Prescription actions (enum, exactly one per prescription):
  update_task_template  - Modify task book template
  add_gate_check        - Add machine gate check item
  append_review_prompt  - Append to review prompt

Output format (rx-draft.md):
  - Header: "处方草稿，须人工确认后应用，脚本不自动修改任何流程文件"
  - Each prescription: action:, rationale: (with source line references), status: DRAFT
  - If no findings and no rework: "无处方建议"
EOF
    exit 1
}

TASK_BOOK=""
EVIDENCE_DIR=""
OUTPUT_PATH=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --task)
            TASK_BOOK="${2:-}"
            shift 2
            ;;
        --evidence)
            EVIDENCE_DIR="${2:-}"
            shift 2
            ;;
        --out)
            OUTPUT_PATH="${2:-}"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Error: Unknown argument: $1" >&2
            usage
            ;;
    esac
done

if [[ -z "$TASK_BOOK" || -z "$EVIDENCE_DIR" || -z "$OUTPUT_PATH" ]]; then
    echo "Error: Missing required arguments" >&2
    usage
fi

if [[ ! -f "$TASK_BOOK" ]]; then
    echo "Error: Task book not found: $TASK_BOOK" >&2
    exit 1
fi

if [[ ! -d "$EVIDENCE_DIR" ]]; then
    echo "Error: Evidence directory not found: $EVIDENCE_DIR" >&2
    exit 1
fi

# Valid prescription actions
VALID_ACTIONS=("update_task_template" "add_gate_check" "append_review_prompt")

# Extract findings from review verdict files
extract_findings() {
    local evidence_dir="$1"
    local findings=()
    
    # Find review verdict files
    local verdict_files=()
    while IFS= read -r -d '' file; do
        verdict_files+=("$file")
    done < <(find "$evidence_dir" -maxdepth 1 \( -name 'review-*-verdict.txt' -o -name 'review-verdict.txt' \) -print0 2>/dev/null)
    
    for verdict_file in "${verdict_files[@]}"; do
        # Extract lines with CRITICAL/MAJOR/MINOR findings
        # Two formats supported:
        # 1. Inline: "MAJOR: finding text" or "MINOR: finding text"
        # 2. Header + numbered list: "MINOR:" followed by numbered items "1. **Title**: Description"
        local in_numbered_section=false
        local current_severity=""
        local line_num=0
        
        while IFS= read -r line; do
            line_num=$((line_num + 1))
            
            # Check for inline format: "SEVERITY: text" (with text on same line)
            if [[ "$line" =~ ^(CRITICAL|MAJOR|MINOR):[[:space:]]*(.+)$ ]]; then
                local severity="${BASH_REMATCH[1]}"
                local text="${BASH_REMATCH[2]}"
                # Skip findings with empty/whitespace-only text or "None" findings
                if [[ -n "${text//[[:space:]]/}" ]] && [[ ! "$text" =~ ^None[[:space:]]*[—-]? ]]; then
                    findings+=("$verdict_file:$line_num:$severity:$text")
                fi
                # Reset section tracking
                in_numbered_section=false
                current_severity=""
            # Check for section headers (SEVERITY: with no text on same line)
            elif [[ "$line" =~ ^(MINOR|MAJOR):[[:space:]]*$ ]]; then
                in_numbered_section=true
                current_severity="${BASH_REMATCH[1]}"
            elif [[ "$line" =~ ^CRITICAL:[[:space:]]*$ ]]; then
                in_numbered_section=true
                current_severity="CRITICAL"
            # Check for numbered items in MINOR/MAJOR/CRITICAL section
            elif [[ "$in_numbered_section" == "true" && "$line" =~ ^[[:space:]]*[0-9]+\.[[:space:]]+\*\*(.+)\*\*:[[:space:]]*(.+)$ ]]; then
                local title="${BASH_REMATCH[1]}"
                local desc="${BASH_REMATCH[2]}"
                local text="$title: $desc"
                if [[ -n "${text//[[:space:]]/}" ]]; then
                    findings+=("$verdict_file:$line_num:$current_severity:$text")
                fi
            # Exit section on new section header or non-numbered, non-empty content
            elif [[ "$line" =~ ^(CRITICAL|MAJOR|MINOR): ]]; then
                # Will be handled by the first condition above
                in_numbered_section=false
                current_severity=""
            elif [[ -n "$line" && "$in_numbered_section" == "true" && ! "$line" =~ ^[[:space:]]*[0-9]+\. ]]; then
                # Non-empty, non-numbered line in a numbered section -> end section
                in_numbered_section=false
                current_severity=""
            fi
            # Empty lines do NOT end the numbered section
        done < "$verdict_file"
    done
    
    printf '%s\n' "${findings[@]}"
}

# Extract rework/legacy items from DELIVERY files
extract_delivery_rework() {
    local evidence_dir="$1"
    local rework_items=()
    
    local delivery_files=()
    while IFS= read -r -d '' file; do
        delivery_files+=("$file")
    done < <(find "$evidence_dir" -maxdepth 1 -name 'DELIVERY*.md' -print0 2>/dev/null)
    
    for delivery_file in "${delivery_files[@]}"; do
        # Look for rework/legacy/remaining issues sections
        # Patterns: "返工", "遗留", "remaining", "rework", "follow-up", "后续"
        while IFS= read -r line; do
            if [[ "$line" =~ (返工|遗留|remaining|rework|follow-up|后续|未解决|待解决) ]]; then
                local line_num
                line_num=$(grep -n -F "$line" "$delivery_file" | head -1 | cut -d: -f1)
                rework_items+=("$delivery_file:$line_num:$line")
            fi
        done < "$delivery_file"
    done
    
    printf '%s\n' "${rework_items[@]}"
}

# Map finding/rework to prescription action
# Heuristic mapping based on content keywords
map_to_action() {
    local text="$1"
    local text_lower="${text,,}"
    
    # Keywords for each action
    if [[ "$text_lower" =~ (模板|template|任务书格式|task.book.format|任务书结构) ]]; then
        echo "update_task_template"
    elif [[ "$text_lower" =~ (机器门|gate.check|自动检查|ci.check|检查项|lint|clippy|fmt|测试覆盖|test.coverage) ]]; then
        echo "add_gate_check"
    elif [[ "$text_lower" =~ (审查提示|review.prompt|评审提示|reviewer指引|review指引|审查关注点|review.focus) ]]; then
        echo "append_review_prompt"
    elif [[ "$text_lower" =~ (哈希|hash|校验|verify|checksum|一致性|consistency) ]]; then
        echo "add_gate_check"
    elif [[ "$text_lower" =~ (配置|config|格式|format|结构|structure) ]]; then
        echo "update_task_template"
    else
        # Default: append_review_prompt for general process improvements
        echo "append_review_prompt"
    fi
}

# Generate prescription from finding/rework
generate_prescription() {
    local source_file="$1"
    local line_num="$2"
    local severity="$3"
    local text="$4"
    local action
    
    action=$(map_to_action "$text")
    
    # Validate action
    local valid=false
    for a in "${VALID_ACTIONS[@]}"; do
        if [[ "$a" == "$action" ]]; then
            valid=true
            break
        fi
    done
    
    if [[ "$valid" != "true" ]]; then
        action="append_review_prompt"  # fallback
    fi
    
    cat <<EOF
- action: $action
  rationale: "$text" (source: $source_file:$line_num)
  status: DRAFT
EOF
}

# Main logic
main() {
    local all_findings=()
    local all_rework=()
    
    # Extract findings
    while IFS= read -r line; do
        [[ -n "$line" ]] && all_findings+=("$line")
    done < <(extract_findings "$EVIDENCE_DIR")
    
    # Extract DELIVERY rework items
    while IFS= read -r line; do
        [[ -n "$line" ]] && all_rework+=("$line")
    done < <(extract_delivery_rework "$EVIDENCE_DIR")
    
    # Generate prescriptions (max 2)
    local prescriptions=()
    local count=0
    
    # Process findings first (higher priority)
    for finding in "${all_findings[@]}"; do
        if [[ $count -ge 2 ]]; then break; fi
        IFS=':' read -r source_file line_num severity text <<< "$finding"
        prescriptions+=("$(generate_prescription "$source_file" "$line_num" "$severity" "$text")")
        count=$((count + 1))
    done
    
    # Process rework items if slots remain
    for rework in "${all_rework[@]}"; do
        if [[ $count -ge 2 ]]; then break; fi
        IFS=':' read -r source_file line_num text <<< "$rework"
        prescriptions+=("$(generate_prescription "$source_file" "$line_num" "REWORK" "$text")")
        count=$((count + 1))
    done
    
    # Write output
    {
        echo "# 处方草稿"
        echo ""
        echo "> 处方草稿，须人工确认后应用，脚本不自动修改任何流程文件"
        echo ""
        
        if [[ ${#prescriptions[@]} -eq 0 ]]; then
            echo "无处方建议"
        else
            for p in "${prescriptions[@]}"; do
                echo "$p"
                echo ""
            done
        fi
    } > "$OUTPUT_PATH"
    
    echo "Prescription draft written to: $OUTPUT_PATH"
}

main "$@"