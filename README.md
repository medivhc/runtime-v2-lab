# runtime-v2-lab

智能体编排开发工作流的运行时验证实验室：基于 OpenCode V2 的多 Agent 协作系统，包含中心服务、控制节点与执行节点，通过任务分派、租约续租、固定验收测试与证据归档，将自然语言目标转化为可查询、可续接、可验收的工程交付流程。

---

## 仓库结构导览

```
.
├── Cargo.toml                 # Workspace 根配置（Rust 2021 edition，resolver = "2"）
├── configs/                   # 中心与节点 TOML 配置
│   ├── central.toml           # 服务监听、数据库、证据根、节点/仓库/验收档位注册
│   ├── control.toml           # 控制节点 runner 配置（role=review, port=4096）
│   ├── worker-a.toml          # 执行节点 A（capabilities=[fixture-exec], port=4097）
│   ├── worker-b.toml          # 执行节点 B（capabilities=[fixture-exec,fixture-special], port=4098）
│   └── *-v05-pause.toml       # 测试暂停点变体（prompt 后 / result 前）
├── crates/
│   ├── domain/                # 纯逻辑层：ID 生成、角色授权、计划校验、状态机、调度规则
│   ├── store/                 # SQLite 存储：迁移、事务、事件、幂等键、调度查询
│   ├── service/               # 中心 HTTP 服务：鉴权、Spec 7.1 全端点、证据下载、错误语义
│   ├── runtime/               # 单二进制 `runtime`：`serve` / `runner` 子命令 + CLI 动词
│   └── opencode_v2/           # OpenCode V2 协议封装：会话创建、提示去重、历史查询
├── scripts/
│   ├── recover-oc.sh          # 幂等恢复：网关 SSH 隧道 + 三个 OpenCode 节点（control/worker-a/worker-b）
│   ├── restore.sh             # 受控恢复：停中心 → 恢复 DB/证据/refs → 状态核对 → 重新开放领取
│   ├── start-oc.sh            # 启动单个 OpenCode V2 节点（端口/密码/模型网关配置自动化）
│   ├── serve-all.sh           # 常驻拉起：中心 + control/worker-a/worker-b runner（缺失自动补）
│   ├── make-fixture.sh        # 生成试件仓库（counter.py 故意失败 + test_counter.py + bare clone）
│   ├── env.sh                 # 进程共用环境基线（Rust toolchain、ccwrap、gateway、密码）
│   ├── probe-oc.sh            # OpenCode V2 接入探针（版本/目录绑定/FIRST-ACK/去重/隔离）
│   └── ...                    # resume-v06.sh, stop-for-v06.sh, test-restore-healthcheck-stub.sh
├── docs/
│   ├── RUNTIME_OPENCODE_V2_PROJECT_SPEC.md  # 完整项目规格（架构、数据、API、CLI、部署、验收）
│   ├── recursive-task-verification-single.md # 递归任务验证单元规格
│   ├── rtv-task-record.md                    # 任务记录模板
│   └── tasks/                    # 任务书：acceptance-profile-pluggable, evidence-cli-fix, restore-sh-fix
└── evidence/                     # 任务证据归档（里程碑 m0–m3 + 真实任务 rt-*/rt-restore-1）
    ├── m0/                       # M0 三节点 OpenCode V2 接入
    ├── m1/                       # M1 中心服务与 runner 核心闭环
    ├── m2/                       # M2 规划/执行/验收/回传完整流程
    ├── m3/                       # M3 真实任务前置：验收档位参数化、恢复、证据 CLI
    ├── rt-evidence-1/            # RT-EVIDENCE-1 证据 CLI 列表与下载路径修复
    ├── rt-restore-1/             # RT-RESTORE-1 受控恢复交付
    └── rt-profile-1-migration-equivalence.md  # counter-v1 迁移等价性证据
```

### crates/ 各 crate 职责（一句话）

| Crate | 职责 |
|-------|------|
| `domain` | ID、角色、计划校验、状态机、调度与授权规则，纯逻辑无 I/O |
| `store` | SQLite 迁移、事务型状态转换、事件写入、幂等约束、调度查询 |
| `service` | 中心 HTTP 服务：Bearer 认证、全 Spec 7.1 端点、错误码语义、证据索引与下载 |
| `runtime` | 单二进制入口：`serve` 启动中心、`runner` 启动节点、CLI 提交/查询/证据/处置 |
| `opencode_v2` | OpenCode V2 原生协议封装：会话创建、稳定消息 ID 去重、历史轮询与恢复 |

### scripts/ 关键脚本用途

| 脚本 | 用途 |
|------|------|
| `recover-oc.sh` | 幂等恢复网关 SSH 隧道（13425→VPS）与三个 OpenCode 节点（4096/4097/4098） |
| `restore.sh` | 受控恢复：停中心 → 复制 `runtime.sqlite` 与 `evidence/` → 核对 run 状态与消息队列 → 重启中心 |
| `start-oc.sh` | 单节点 OpenCode V2 启动（XDG 目录隔离、模型网关配置、密码注入） |
| `serve-all.sh` | 常驻守护：中心 + 双 runner，进程缺失自动补齐（`setsid nohup` 后台） |
| `make-fixture.sh` | 生成试件仓库（故意失败的 `counter.py` + 单测 + bare clone），输出 baseline SHA |
| `env.sh` | 所有进程 source 的环境基线：Rust stable toolchain、ccwrap/clang、gateway URL、节点密码 |

### docs/ 关键文档指引

- `RUNTIME_OPENCODE_V2_PROJECT_SPEC.md`：完整规格（架构、数据模型、状态机、API 合同、CLI、部署、验收）
- `tasks/*.md`：真实任务书（验收档位参数化、证据 CLI 修复、restore.sh 修复）

### evidence/ 任务证据归档说明

按里程碑与真实任务分目录归档：每目录含 `*-REPORT.md`（判定/逐项证据/文件清单）、`*-VERIFY.md`（核对单）、原始 JSON/日志/哈希文件。真实任务证据位于 `rt-evidence-1/`、`rt-restore-1/`、`rt-profile-1-migration-equivalence.md`。

### configs/ 中心配置

`central.toml` 为单一事实来源：服务监听/数据库/证据根/操作者 token/规划节点/离线判定/租约秒数；`[nodes.*]` 定义角色/能力/token_env；`[repositories.*]` 定义 bare 路径与验收档位（`test_cmd` + `allowed_paths` glob）。节点配置（`control.toml`、`worker-a.toml` 等）引用 `central.toml` 路径以加载档位细节。

---

## 快速上手

### 依赖

- Rust **1.99.1 (stable-x86_64-unknown-linux-gnu)** — 由 `scripts/env.sh` 通过 `rustup` 提供
- `clang` + `ccwrap`（沙箱无系统 gcc，`scripts/env.sh` 已导出 `CC=clang` `AR=ar`）
- OpenCode V2 二进制（`/root/workspace/.local/opencode/opencode`）
- 模型网关：Magpie Gateway 经 SSH 隧道 `127.0.0.1:13425`（见 `scripts/recover-oc.sh`）

### 构建与测试

```bash
# 进入工作树并加载环境
cd /root/workspace/runtime-v2-lab
source scripts/env.sh

# 全 workspace 构建
cargo build --workspace

# 全 workspace 测试（含 domain/store/service/runtime/store 集成测试）
cargo test --workspace
```

### 运行完整栈（开发/验证用）

```bash
# 1) 生成试件仓库与 baseline
bash scripts/make-fixture.sh
# 输出: fixture=/root/workspace/runtime-v2-lab/fixture bare=/root/workspace/runtime-v2-lab/fixture.git baseline=<SHA>

# 2) 恢复/启动 OpenCode 三节点 + 网关隧道
bash scripts/recover-oc.sh

# 3) 启动中心 + 双 runner（常驻守护）
bash scripts/serve-all.sh &
# 中心监听 127.0.0.1:18080（central.toml），日志见 logs/central.log

# 4) 提交目标（需导出 operator token）
export RUNTIME_OPERATOR_TOKEN=op-token-e2e
export RUNTIME_URL=http://127.0.0.1:18080
runtime submit --repo fixture --baseline <SHA> --require fixture-exec "修复 counter.py 让测试通过"

# 5) 查询状态与事件
runtime status --request <REQ_ID> --json
runtime events --request <REQ_ID> --after 0

# 6) 导出证据（run 结束后）
runtime evidence --run <RUN_ID> --out ./evidence-export
```

### CLI 子命令速查（`runtime --help` / `runtime <sub> --help`）

| 子命令 | 说明 |
|--------|------|
| `serve --config <central.toml>` | 启动中心服务（迁移 DB、绑定规划会话、HTTP API） |
| `runner --config <node.toml>` | 启动节点 runner（heartbeat→claim→checkout→opencode→commit→evidence→finish） |
| `submit --repo <ref> --baseline <SHA> --require <cap> "目标"` | 操作者提交原始意图，返回 `request_id` |
| `status --request <ID> [--json]` | 查询请求/任务/Run 状态与回传状态 |
| `events --request <ID> --after <N>` | 读取有序事件流（增量游标） |
| `evidence --run <ID> --out <DIR>` | 导出该 run 的补丁、日志、报告、哈希 |
| `resolve --run <ID> --decision <close|continue> --reason "..."` | 操作者处置待核对 run（关闭旧尝试或继续核对） |
| `retry --task <ID>` | 旧 run 终态后重新开放任务领取 |

---

## 流水线工作法简述

本仓库实践**任务单 → worker 实现 → 机器门 → 独立审查 → 合入 main → evidence 归档**的闭环：

1. **任务单**（`docs/tasks/*.md`）  
   明确编号、基线、目标、验收档、约束。例如 `RT-PROFILE-1` 将硬编码验收参数化为档位配置。

2. **worker 实现**（`rt/<task-id>` 分支）  
   在工作树内按任务书修改代码/配置/脚本，**不改动无关文件**，完成后 `git commit` 说明改动。

3. **机器门**（本地 CI）  
   `cargo fmt --check && cargo clippy -- -D warnings && cargo test --workspace` 全绿才可提交 PR。

4. **独立审查**  
   由独立审查者（非实现者）对照任务书与验收档位，检查 diff、运行固定测试、给出 `pass/fail` 判定，产出 `DELIVERY-<TASK>.md` 与 `VERDICT-<TASK>.md`。

5. **合入 main**  
   审查通过后，以 `--no-ff` 合入 `main`，保留分支历史与审查记录。

6. **evidence 归档**（`evidence/<task-id>/`）  
   归档：交付报告、审查裁决、patch diff、git log、配置变更、迁移等价性证据。目录结构与 `evidence/m0/` 等里程碑一致，便于后续追溯与审计。

> 该流程在 `docs/RUNTIME_OPENCODE_V2_PROJECT_SPEC.md` 第 10 节与 `evidence/` 实际结构中已验证多轮（M0–M3、RT-RESTORE-1、RT-EVIDENCE-1、RT-PROFILE-1）。

---

## 备注

- 仓库未定 License，不在 README 中声明许可协议。
- 所有命令、路径、版本号均取自仓库真实文件（`Cargo.toml`、`central.toml`、`scripts/*.sh`、`docs/*.md`、`crates/*/src/*.rs`），阅读后撰写，未虚构。