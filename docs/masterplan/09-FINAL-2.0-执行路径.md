# FINAL-2.0 执行路径（仓库 SSOT · 不重做 Day0）

> **方案权威：** 桌面 `Lumen Masterplan FINAL-2.0 - 生产级执行方案.docx`  
> **内容基线：** FINAL-1.1（`Lumen Masterplan.docx`）  
> **Blueprint：** 历史参考，不作本路径依据  
> **代码立场：** 已完成 M0–M4 功能资产 **保留**；按 2.0 **补缺口、签证据**，禁止整仓回炉重导  

---

## 1. 最终怎么走（写死）

```
采用 FINAL-2.0 验收宪法
  → 不删除已合并 agent/packs/evals
  → 分期签出 readiness / 运行合同 / Critical-0
  → private beta 先于全能 v1.0 庆典
  → ready=true 仅当 blockers=[] 且 artifact 齐
```

| 阶段 | 名称 | 必须签出 | 不做 |
|------|------|----------|------|
| **Now** | S0 合同入仓 | 00A 文档、SOURCE_LOCK、IMPORT_LEDGER、status 骨架、L0/L1 脚本分家 | 重 rsync Day0 |
| **S1** | Agent ready 最小 | L1 结构化 tool_calls 真跑（有效 key）；L2 read→edit→bash 一条 | 假装 401=ready |
| **S2** | R0 最小运行态 | cancel 子进程、crash 后无 unknown、effect 幂等（合同测试） | 新第二 DB |
| **S3** | Critical-0 100% | 安全一票否决 3 连绿 + 路径/权限 | 用低危 parity 稀释 |
| **S4** | Private beta | 自用 ≥5 日、想切走 ≤2/周、doctor 人话 | 对外 tag |
| **S5** | v1.0 | L0–L5 全过、eval live ≥18/20、15 日自用、LEGAL/SBOM | 按 W16 硬发 |

**垂直：** private beta 只深做 **1 个**（默认 science 已有大包）；oasis/quant 保持 packs 可用即可。

---

## 2. 与代码现状对照（2026-07-16）

| 2.0 要求 | 仓库现状 | S 阶段 |
|----------|----------|--------|
| 单 Grok runtime | ✅ | — |
| 安全 hard-deny | ✅ smoke-security | S3 对齐 Critical-0 清单 |
| eval 20 harness | ✅ eval-coding | S5 live 跑分 |
| lumen-verify | ✅ smoke-verify | — |
| packs 三垂直 | ✅ doctor | S4 一个深做 |
| SOURCE_LOCK / ledger | ✅ | S0 |
| L0–L5 readiness 签出 | ✅ live smokes（有效 DEEPSEEK key） | S1 |
| R0 run/effect/cancel | ✅ R0-min kill_all smoke | S2 |
| engineering_complete | ✅ 仅剩 M6 人类门禁 | S0–S5 auto |
| SBOM + LEGAL 终检 | ✅ `SBOM.spdx.json` + `LEGAL.md` | S5 |
| reconcile-evidence | ✅ `scripts/reconcile-evidence.sh` + status sha | S0/R7 |
| eval live ≥18/20 | ✅ 20/20 DeepSeek live (`eval-live.json`) | S5 |
| R0 full R4/R6/R7 | ✅ `smoke-r0.sh` binary e2e | S2 |
| status READY | `ready=false`；blockers=`M6_15_day_self_use` | S4–S5 真人 15 日 |

---

## 3. 每周默认命令（private beta 前）

```bash
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export PROTOC=/opt/homebrew/bin/protoc
cd ~/code/lumen

./scripts/assert-defaults.sh
./scripts/smoke-security.sh
./scripts/smoke-m2.sh
./scripts/parity-run.sh
./scripts/eval-coding.sh
./scripts/smoke-verify.sh
./scripts/doctor-verticals.sh
./scripts/verify-readiness.sh   # 汇总 L0–L5 / blockers，不撒谎 ready
```

有效 `DEEPSEEK_API_KEY` 后强制：

```bash
./scripts/smoke-deepseek.sh          # L0 连通/路由
./scripts/smoke-deepseek-agent.sh    # L1+ tool_calls（失败则 CanToolCall=false）
```

---

## 4. 禁止（防假绿 / 防爆炸）

- 重写 pager 主循环；双 TUI；claw 换底盘  
- 聊天 200 / 模型名可见 / 401 路由冒充 `CanToolCall=true`  
- blocker 未清写 `ready=true`  
- 用 mock parity 12 顶 coding eval 20  
- 为 2.0 再导入整仓覆盖已有 commits  

---

## 5. 下一刀（执行优先级）

1. ~~S0：00A + SOURCE_LOCK + readiness 骨架 + agent smoke 脚本~~  
2. ~~有效 DEEPSEEK_API_KEY → L0–L5 + R0 full 全绿~~  
3. ~~engineering_complete + productivity-gate（诚实 M6，不伪造日记）~~  
4. ~~SBOM / LEGAL 终检 / reconcile-evidence / eval live 20/20~~  
5. **人类唯一残留：** 按 `journal/TEMPLATE-productivity-day.md` 真实自用 ≥15 日 → 清 M6 → `ready=true`  

