# 任务书 RT-README-1：为公开仓库编写 README

## 背景
runtime-v2-lab 已公开推送至 github.com/medivhc/runtime-v2-lab（main=fccd4e5），但仓库没有 README，外部访客无法理解项目用途与结构。本任务为仓库补一份高质量 README。

## 工作环境
- 工作树：/root/workspace/runtime-v2-lab/wt-readme-1（分支 rt/readme-1，基于 main=fccd4e5）
- 所有文件操作必须在该工作树内进行
- 仓库语言：Rust（crates/service、crates/runtime），附 scripts/、docs/、evidence/、configs/

## 任务要求
在仓库根目录创建 `README.md`（中文），内容依据仓库真实文件撰写，先读再写，不得虚构：
1. **项目一句话定位**：智能体编排开发工作流的运行时验证实验室（以仓库实际代码为准，阅读 crates/ 与 docs/ 后概括）。
2. **仓库结构导览**：crates/（各 crate 职责一句话）、scripts/（关键脚本用途，如 recover-oc.sh、restore.sh、backup.sh）、docs/（关键文档指引）、evidence/（任务证据归档说明）、configs/（中心配置）。
3. **快速上手**：从 Cargo.toml 提取真实的构建与测试命令（如 cargo build / cargo test --workspace），如实标注依赖（Rust 版本等，以仓库实际为准）。
4. **流水线工作法简述**：任务单 → worker 实现 → 机器门（fmt/clippy/test）→ 独立审查 → 合入 main → evidence 归档（依据 docs/ 与 evidence/ 实际结构描述，不夸大）。
5. 徽章或状态区不需要；不要写联系方式、许可协议（仓库未定 License，不虚构）。

## 硬性约束
- 只允许新增/修改：README.md、docs/**（验收档 docs-v1）
- 不得改动任何代码、脚本、配置
- README 长度 80–200 行；所有命令必须来自仓库真实文件，写前先核对
- 完成后运行 `test -s README.md` 自检非空

## 交付
- 在工作树内 git add README.md 并 git commit（信息格式：`docs: add repository README`）
- 回复中列出：README 章节目录、撰写依据的文件清单、自检结果
