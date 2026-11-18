# 处方草稿（RT-GATE-1 归档环节，2026-11-18）

> 处方草稿，须人工确认后应用，脚本不自动修改任何流程文件。本稿由编排方依据归档证据整理；rx-draft.sh 对「FINDINGS: minor、…」行内中文枚举格式暂不识别（见处方②）。

## 处方①
- action: add_gate_check
- rationale: 本单机器门发现 main 基线 clippy 漂移——既有 store crate 4 处 too-many-arguments 在当前工具链（沙箱 11-17 重置后重装）下报错，与"clippy 干净"的既有承诺不符；建议机器门显式记录 clippy 基线状态，漂移期间以 fmt+test 为门、clippy 漂移列入已知豁免并限期修复
- status: DRAFT

## 处方②
- action: update_task_template
- rationale: 审查结论 FINDINGS 的行内中文枚举格式（"minor、文件:行、问题、建议"连缀一行）与 rx-draft.sh 支持的两种格式（"MINOR: text" 行内 / "MINOR:" 加编号列表）不匹配，导致处方自动提取为空；建议任务书模板约定审查结论 FINDINGS 逐条分行，使处方提取可机读
- status: DRAFT
