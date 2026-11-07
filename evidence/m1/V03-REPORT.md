# V03 REPORT — 验收固定检查的三路对抗核验

日期：2026-11-07（北京时间）。结论：**PASS**。

## 验证目标（规格 V03）
验收输入分别使用：基线提交、篡改测试提交、真实修复提交。
前两者被固定检查驳回（非零退出码或差异校验失败），仅真实修复通过。

## 环境
- bare 仓库：`fixture.git`，基线 `265f9fcb997c94d604f1912e050567bdd07b8418`
- 候选：`2207edf0cd29f16ecc93173f3bea713c127f02d2`（refs/runs/run-18e5b60201a46412）
- 固定测试：基线 `test_counter.py`，断言 `increment(value) == value + 1`（value ∈ {-1, 0, 5}）
- 复现目录：`evidence/m1/v03/`（base/ real/ tampered/ 及三个结果文件）

## 三路结果（python3 -m unittest -v test_counter，PYTHONDONTWRITEBYTECODE=1）

| 输入 | counter.py 来源 | test_counter.py | 退出码 | 结果 |
|---|---|---|---|---|
| 基线提交 | 基线 `return value` | 原始固定测试 | **1** | FAILED（`AssertionError: -1 != 0`）→ 正确驳回 |
| 篡改测试 | 真实修复 `return value + 1` | 篡改为 `value + 2` | **1** | FAILED（`AssertionError: 6 != 7`）→ 正确驳回 |
| 真实修复 | 真实修复 | 原始固定测试 | **0** | OK → 通过 |

要点：
- 篡改测试是**双向**必要的：修改断言的期望值后，即使真实修复也无法通过，
  证明固定检查确实以测试输出为准，而非以候选提交来源为准。
- 基线被驳回证明测试初始失败（附录 B 试件要求），未执行修复的任务不可能蒙混过关。
- 差异范围检查（生产代码路径 `review` 流程内）：`changed == ["counter.py"]`，
  候选仅修改目标文件。

## 线上验收一致性
线上评审（run-18e5b607ae8ef613）对该候选的固定检查退出码 `0`、
agent 裁决 `pass`、终审 `pass`，与本目录实测一致。

## 判定
V03 判据满足：前两路被驳回（非零退出码），仅真实修复通过。**PASS**。
