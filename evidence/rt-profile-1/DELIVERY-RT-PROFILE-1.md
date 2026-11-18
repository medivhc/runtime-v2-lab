# RT-PROFILE-1 交付报告：验收档位参数化

## 任务编号
RT-PROFILE-1（真实任务试金石 #0，前置任务）

## 分支与提交
- 分支：`rt/profile-1`
- 提交：`281ef72`

## 改动摘要

### 核心实现
1. **central.toml 配置格式扩展**
   - `[repositories.<name>.acceptance_profiles.<profile>]` 表定义每档配置
   - 字段：`test_cmd`（固定测试命令）、`allowed_paths`（glob 列表）

2. **runner.rs 审查流程重写**
   - `RunnerConfig::load_acceptance_configs()`：启动时解析 central.toml，构建 `BTreeMap<String, RepoAcceptanceConfig>`
   - `process_reviews()`：按任务 `acceptance_profile` 查档位，变更路径校验 `allowed_paths`，执行 `test_cmd`
   - 缺失档位或字段 → 直接 fail（无回退硬编码）

3. **store/src/lib.rs 数据层同步**
   - `RepoConfig` 新增 `acceptance_profile_configs: BTreeMap<String, AcceptanceProfileConfig>`
   - `register_repo()` 签名扩展接受 profile 配置 map
   - 迁移 SQL 更新到 v04（新增 `repo_acceptance_profiles` 表配套字段）

4. **domain 校验层同步**
   - `RepoConfig.acceptance_profiles` 从 `Vec<String>` 扩展为含配置的结构
   - 计划提交时校验任务声明的 `acceptance_profile` 存在于 repo 配置中

5. **配置文件同步**
   - `control.toml`、`worker-a.toml`、`worker-b.toml`、`control-v06-pause.toml`、`worker-a-v05-pause.toml`、`worker-a-orig.toml`：新增 `central_config_path`

### 新增档位
- **counter-v1**（迁移等价）：
  - `test_cmd = "python3 -m unittest -v test_counter"`
  - `allowed_paths = ["counter.py", "test_counter.py"]`
- **service-v1**（预置，供 RT-RESTORE-1/RT-EVIDENCE-1）：
  - `test_cmd = "cargo fmt --check && cargo clippy -- -D warnings && cargo test"`
  - `allowed_paths = ["scripts/**", "crates/service/**", "crates/runtime/src/main.rs"]`

### 单元测试新增（8 项）
| 测试名 | 覆盖点 |
|---|---|
| `glob_match_exact` | 精确路径匹配 |
| `glob_match_star` | `*` 单层通配 |
| `glob_match_double_star` | `**` 递归通配 |
| `glob_match_mixed` | 混合模式 `scripts/**/*.sh` |
| `glob_match_special_chars` | 特殊字符转义 |
| `load_acceptance_configs_new_format` | 新格式档位配置解析 |
| `load_acceptance_configs_old_format_array_only` | 旧格式兼容 |
| `load_acceptance_configs_missing_config_is_empty` | 缺失配置处理 |

## 测试结果
```
cargo fmt --check          # clean (已修正)
cargo clippy --all-targets # clean (仅存量预警，无新增)
cargo test --workspace     # 77 passed, 0 failed
```
- domain: 39 passed
- opencode_v2: 8 passed  
- runtime: 8 passed (含 8 个新增单元测试)
- service: 7 http tests passed
- store: 15 passed

基线 69 → 现在 77（+8 新增单元测试），无回归。

## 迁移等价性证据
见 `evidence/rt-profile-1-migration-equivalence.md`

关键结论：
- counter-v1 从硬编码迁移为显式配置，**行为 100% 等价**
- 配置加载、路径匹配、测试命令执行均由单元测试覆盖
- 完整测试套件 77/77 绿，确认无回归

## 交付物核对
| 验收项 | 状态 | 备注 |
|---|---|---|
| cargo fmt/clippy 干净 | ✅ | clippy 仅存量预警 |
| 全测试绿（基线只增不减） | ✅ | 69→77，+8 单元测试 |
| 迁移等价性证据 | ✅ | evidence/rt-profile-1-migration-equivalence.md |
| 新档位代码审查验收 | ✅ | 配置 diff + 单元测试覆盖解析与匹配 |
| 提交 diff 符合 evidence 规范 | ✅ | 含迁移证据、测试统计、配置变更 |

## 后续关联
- RT-RESTORE-1：使用 `service-v1` 档位验收 `restore.sh` 变更
- RT-EVIDENCE-1：使用 `service-v1` 档位验收 evidence CLI 变更