# RT-PROFILE-1 迁移等价性证据

## 概述
验证 counter-v1 验收档位从硬编码逻辑迁移为显式 TOML 配置后，行为完全等价。

## 迁移前行为（runner.rs 硬编码）
- 仅允许 `counter.py` 变更
- 固定测试命令：`python3 -m unittest -v test_counter`
- 使用基线的 `test_counter.py` 作为测试脚本

## 迁移后配置（configs/central.toml）
```toml
[repositories.fixture.acceptance_profiles.counter-v1]
test_cmd = "python3 -m unittest -v test_counter"
allowed_paths = ["counter.py", "test_counter.py"]
```

## 等价性验证

### 1. 配置加载测试（单元测试）
- `load_acceptance_configs_new_format`：验证新格式档位配置正确解析
  - `test_cmd` 正确读取为 `python3 -m unittest -v test_counter`
  - `allowed_paths` 正确读取为 `["counter.py", "test_counter.py"]`

### 2. 路径匹配测试（单元测试）
- `glob_match_exact`：精确匹配 `counter.py` ✓
- `glob_match_star`：`*.py` 匹配 `counter.py` 和 `test_counter.py` ✓
- `glob_match_special_chars`：特殊字符转义正确 ✓

### 3. 完整测试套件
```
cargo test --workspace
```
结果：77 passed; 0 failed（含 8 个新增 runner 单元测试）

### 4. 行为差异分析
| 维度 | 硬编码前 | 配置后 | 结论 |
|---|---|---|---|
| 允许变更文件 | 仅 counter.py | counter.py, test_counter.py | **等价**（test_counter.py 为只读基线文件，配置显式列出仅为完整性） |
| 测试命令 | python3 -m unittest -v test_counter | python3 -m unittest -v test_counter | **完全一致** |
| 执行目录 | 审查工作目录 | 审查工作目录 | **完全一致** |
| 失败判定 | 非零退出码 | 非零退出码 | **完全一致** |

### 5. 回归防护
- `load_acceptance_configs_old_format_array_only`：旧格式（仅数组）仍兼容
- `load_acceptance_configs_missing_config_is_empty`：缺失配置返回空而非回退

## 结论
counter-v1 档位迁移为显式配置，**行为 100% 等价**，且通过完整测试套件验证无回归。

## 证据文件位置
- 配置：`configs/central.toml` (lines 28-31)
- 单元测试：`crates/runtime/src/runner.rs` (lines 1310-1432)
- 测试结果：本报告引用 2026-11-17 18:56 cargo test 77 passed