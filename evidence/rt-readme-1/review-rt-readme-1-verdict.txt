VERDICT: APPROVE
- major: []
- minor: 
  - 1: 树形图中 `*-v05-pause.toml` 模式未覆盖 `control-v06-pause.toml`（实为 v06 变体），且遗漏 `worker-a-orig.toml` 与 `evidence/m0-selftest/` 目录。
  - 2: 规格文档 (`docs/RUNTIME_OPENCODE_V2_PROJECT_SPEC.md` 第 44/187/221 行) 记载中心监听 `127.0.0.1:8080`，但实际 `configs/central.toml` 与 README 均为 `18080`；README 以实配为准，无误，但规格与实现存微差。
  - 3: 第 62 行 `recover-oc.sh` 隧道描述“13425→VPS”经核验与脚本实现一致：脚本第 12 行 `ssh -p 12224 ... -L 13425:10.144.144.3:3425` 走 VPS 端口 12224（注释亦写“经 vps”）；任务书预置“现行为经 SH-YF 直连 12225”在本提交 (e0ffd86) 代码中不存在，README 如实记录当前代码行为，不失实。

核验依据文件：
- `/root/workspace/runtime-v2-lab/wt-readme-1/README.md` (e0ffd86 完整内容)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/recover-oc.sh` (第 8-17 行：隧道建立逻辑与注释)
- `/root/workspace/runtime-v2-lab/wt-readme-1/configs/central.toml` (第 2 行：listen=18080)
- `/root/workspace/runtime-v2-lab/wt-readme-1/configs/*.toml` (control/worker-a/worker-b 及变体)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/env.sh` (第 6,9,14 行：Rust 1.99.1、ccwrap、GATEWAY_URL=13425)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/serve-all.sh` (第 16-23 行：中心与 runner 启动)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/start-oc.sh` (第 5-9,21-24,38 行：节点端口/密码/网关配置)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/make-fixture.sh` (第 13-34 行：fixture 生成与 baseline)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/restore.sh` (全文：受控恢复流程)
- `/root/workspace/runtime-v2-lab/wt-readme-1/scripts/probe-oc.sh` (全文：探针逻辑)
- `/root/workspace/runtime-v2-lab/wt-readme-1/crates/runtime/src/main.rs` (第 17-79 行：CLI 子命令定义)
- `/root/workspace/runtime-v2-lab/wt-readme-1/Cargo.toml` (workspace 成员与 edition/resolver)
- `/root/workspace/runtime-v2-lab/wt-readme-1/docs/RUNTIME_OPENCODE_V2_PROJECT_SPEC.md` (第 44,114-131,183,187,221 行)
- `/root/workspace/runtime-v2-lab/wt-readme-1/docs/tasks/*.md` (三个任务书)
- `/root/workspace/runtime-v2-lab/wt-readme-1/evidence/*` (目录结构与文件清单)
- `git diff fccd4e5..e0ffd86` (仅 README.md 变更 178 行)

---

Now I have all the evidence. Let me compile the final verdict.


---

Now let me do a final comprehensive check of the evidence structure and any other potential issues.


---

Let me check the runtime CLI subcommands and verify other claims.


---

Now let me check restore.sh, probe-oc.sh, resume/stop scripts, the Rust version, CLI subcommands, and docs/tasks.

---

Configs check out. Now the remaining scripts and env.sh to verify build commands, versions, gateway URL, ports.

---

Key finding emerging. Line 12 of recover-oc.sh uses `-p 12224` (via VPS). The task book claims historical VPS 12224, current SH-YF direct 12225. Let me read the remaining scripts and configs to fully verify.

---

The README is committed at e0ffd86 (HEAD). Let me now verify the repository structure and key files in parallel.

---

I'll begin by exploring the repository and reading the key files. Let me start with parallel investigation.