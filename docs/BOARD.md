# Lumen 看板（A / B / C）

> **更新规则：** 只在里程碑更新。无心跳 monitor。  
> 上次刷新：**2026-07-20T09:15Z**（UTC）— 路线图收尾（除 Windows）

---

## 总览

| 线 | 名称 | 完成度 | 状态 |
|----|------|--------|------|
| **A** | Expert prove/harden 交付 | **~95%** | 主路径收口；Windows 仍后置 |
| **B** | grok-build cherry | **P0+P1 100%** | #127 + #128 已合 main |
| **C** | 路线图收尾 | **~90%** | v0.1.222 已 bump；Windows 跳过 |

---

## A — Expert 交付

| 项 | 状态 |
|----|------|
| Dual / tools / evidence / GitHub | ✅ main |
| macOS v0.1.221-macos | ✅ |
| Windows 真 binary | ⏸ **本轮跳过**（用户决策） |

---

## B — 上游 cherry

| 项 | 状态 |
|----|------|
| P0 dispatch_locks + OSC52 | ✅ `f29bd2e` / #127 |
| P1 `/summarize` alias | ✅ #128 |
| P1 marketplace `require_sha` | ✅ #128 |
| P1 auth recovery | SKIP（与 pin 一致） |
| Expert / defaults 未污染 | ✅ |

---

## C — 发版 / 运维

| 项 | 状态 |
|----|------|
| VERSION | **0.1.222** |
| Windows 包 | ⏸ 跳过 |
| 看板 | 本文件收口 |

---

## 一句话

**P0+P1 已上 main；0.1.222 版本已推进；Windows 按用户要求后置。**
