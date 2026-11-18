# DELIVERY-RT-README-1

## 任务概要
- 任务编号：RT-README-1（公开仓库治理·README）
- 工作树：wt-readme-1（branch rt/readme-1，基线 main=fccd4e5）
- 提交号：e0ffd86（docs: add repository README，+178 行仅 README.md）

## 交付内容
为公开仓库 github.com/medivhc/runtime-v2-lab 补齐中文 README：项目定位、仓库结构导览（crates/scripts/docs/evidence/configs）、快速上手（真实构建/测试命令与依赖）、完整栈运行指引、流水线工作法简述。

## 验收结果
- 机器门 docs-v1：test -s README.md PASS；改动边界核验仅 README.md。
- 独立审查（worker-b，ses_e1314093fffex22zXRBWwFisVQ）：APPROVE，0 major，3 minor：
  1. 树形图未覆盖 control-v06-pause.toml/worker-a-orig.toml/evidence/m0-selftest（表述不全，不阻断）
  2. 规格文档记载中心端口 8080 与实配 18080 存微差（既有问题，README 以实配为准无误）
  3. 隧道描述"13425→VPS"与提交版 recover-oc.sh 一致（编排方预置疑点实为 live 工作区脚本与仓库版的差异，README 不失实）
- 合入：--no-ff merge → main=fadf340。

## 遗留
- GitHub 推送需用户再次提供凭据（origin 为无凭据 URL）。
- live 工作区 scripts/recover-oc.sh 的 SH-YF 改线（12225）未提交进仓库，与提交版（12224 VPS）分叉，建议后续以流水线工单收口。
- LICENSE/description/topics 缺失，待用户定 License 类型。
