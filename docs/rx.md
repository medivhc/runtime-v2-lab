# 处方草稿机制

## 定位与设计理念

本机制借鉴 **LoopForge runeval 三段式** 思想，将归档环节的"事后改进"拆解为三个受控阶段：

```
事实提取 → 受控处方 → 人工应用
    ↓           ↓           ↓
review/DELIVERY  rx-draft.sh  编排方/维护者
```

- **事实提取**：从审查结论的 `FINDINGS` 与 `DELIVERY` 返工记录中自动归纳问题
- **受控处方**：`scripts/rx-draft.sh` 仅输出**草稿**，动作严格限定在枚举集合内，禁止自由文本
- **人工应用**：确认后由编排方将处方写入任务书模板、机器门配置或审查提示词，**脚本永不写入任何流程文件**

## 为何不自动应用？

1. **可审计**：每条处方有明确溯源，人工确认留存决策痕迹
2. **可逆转**：草稿阶段可丢弃、合并、拆分，不污染流程基线
3. **最小权限**：脚本只读归档产物，不持有写入流程文件的权限

## 脚本规格

### `scripts/rx-draft.sh`

**用法**
```bash
rx-draft.sh --task <任务书路径> --evidence <evidence目录> --out <输出路径>
```

**输入**
- 任务书全文
- `evidence/` 目录下的：
  - `review-*-verdict.txt`：含 `FINDINGS` 区块，每行以 `CRITICAL:`/`MAJOR:`/`MINOR:` 开头
  - `DELIVERY*.md`：含返工/遗留记录（关键字：返工、遗留、remaining、rework、follow-up、后续、未解决、待解决）

**输出**：`rx-draft.md`

**处方动作枚举**（三选一，禁止其他动作）

| 动作 | 语义 | 典型触发场景 |
|------|------|-------------|
| `update_task_template` | 修改任务书模板 | 任务书结构缺失、验收档定义模糊、配置格式规范需更新 |
| `add_gate_check` | 新增机器门检查项 | 缺失自动化校验（哈希一致性、fmt/clippy/test、格式校验等） |
| `append_review_prompt` | 追加审查提示词 | 审查关注点遗漏、需增加人工评审检查清单项 |

**输出格式**
```markdown
# 处方草稿

> 处方草稿，须人工确认后应用，脚本不自动修改任何流程文件

- action: add_gate_check
  rationale: "Hash verification mismatch in DELIVERY report - reported hashes differ from evidence table" (source: evidence/rt-evidence-1/review-verdict.txt:10)
  status: DRAFT

- action: append_review_prompt
  rationale: "遗留: Need to add automated hash verification gate check in CI pipeline." (source: evidence/rt-evidence-1/DELIVERY-test.md:8)
  status: DRAFT
```

**无 findings 时输出**：
```markdown
# 处方草稿

> 处方草稿，须人工确认后应用，脚本不自动修改任何流程文件

无处方建议
```

### `scripts/rx-selftest.sh`

自测脚本，覆盖三大场景：
1. **有 findings** → 生成 1–2 条处方，动作合法、带 rationale 与 DRAFT 状态
2. **无 findings** → 输出"无处方建议"
3. **非法动作被拒绝** → 任意输入文本均映射到合法枚举，不产生非法 action

运行方式：
```bash
./scripts/rx-selftest.sh
# 输出 SELFTEST: PASS / FAIL
```

## 人工确认流程

1. **生成草稿**：归档环节执行 `rx-draft.sh`，产出 `rx-draft.md`
2. **评审草稿**：编排方/维护者阅读 `rationale` 与溯源行号，判断是否采纳
3. **执行应用**（三选一，对应动作枚举）：
   - `update_task_template` → 编辑 `docs/tasks/` 下的任务书模板或基线任务书
   - `add_gate_check` → 修改 CI 配置、验收脚本或 `configs/` 下的机器门规则
   - `append_review_prompt` → 追加内容至审查提示词文档（如 `docs/review-prompt.md` 或类似）
4. **记录决策**：在 `rx-draft.md` 旁或归档记录中标注 `status: APPLIED` / `status: REJECTED` 与确认人、日期
5. **归档**：将确认后的 `rx-draft.md` 纳入证据归档目录

## 约束与边界

- **脚本只读**：`rx-draft.sh` 仅读取归档产物，**绝不写入**任务书、审查提示词、CI 配置等流程文件
- **幂等性**：同一输入多次运行产出相同草稿
- **上限 2 条**：每次归档最多产出 2 条处方，避免噪音
- **枚举封闭**：动作集合固定为三值，新增动作需修改脚本与文档同步变更

## 示例：真实证据产出的处方草稿

基于 `evidence/rt-evidence-1/` 的真实审查结论与 DELIVERY：

```markdown
# 处方草稿

> 处方草稿，须人工确认后应用，脚本不自动修改任何流程文件

- action: add_gate_check
  rationale: "Hash verification mismatch in DELIVERY report - reported hashes differ from evidence table" (source: evidence/rt-evidence-1/review-verdict.txt:10)
  status: DRAFT

- action: update_task_template
  rationale: "Config format inconsistency in central.toml (pre-existing)" (source: evidence/rt-evidence-1/review-verdict.txt:12)
  status: DRAFT
```

---

*本文档随 `scripts/rx-draft.sh`、`scripts/rx-selftest.sh` 同步维护，版本锚定为 rx-v1。*