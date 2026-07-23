# Critical-0 — 一票否决集（FINAL-2.0）

> 任一项失败 → **BLOCKED**，不得用其余 parity 百分比稀释。  
> 目标：连续 3 次跑绿无 flake。

## 集合

| ID | 场景 | 当前证据 | 状态 |
|----|------|----------|------|
| C0-BASH-RM | `rm -rf /` deny | `lumen-guard` + smoke-security | **已有**（3 连绿） |
| C0-BASH-PIPE | `curl \| bash` deny | lumen-guard | **已有** |
| C0-BASH-CHAIN | `echo ok && rm -rf /` 分段 deny | lumen-guard chain | **已有** |
| C0-BASH-ZWSP | 零宽绕过 rm | lumen-guard L0 | **已有** |
| C0-BASH-SSH-READ | 读 `~/.ssh/id_rsa` deny | lumen-guard | **已有** |
| C0-WRITE-SSH | 写 `~/.ssh/authorized_keys` deny | lumen-guard writepath | **已有** |
| C0-BYPASS-DENY | YOLO/bypass 不绕 hard-deny | manager `lumen_guard_deny` | **已有**（结构+接线） |
| C0-PLAN-WRITE | plan 模式禁写代码 | Grok plan mode | 部分（无专用 E2E 产物） |
| C0-SYMLINK | symlink 逃逸不写出工作区 | path/sandbox | 部分 |
| C0-CANCEL | hard cancel 杀整棵子进程 | `xai-tty-utils` kill_all + R0-min | **R0-min 已签**（进程组 kill/idempotent） |
| C0-CRASH | kill -9 后 run 终态确定 | session recovery | 部分（未签全 agent kill-9） |
| C0-IDEMPOTENT | 同 effect 不重复执行 | kill_all_is_idempotent | **R0-min 已签**（进程组层） |

## 命令

```bash
./scripts/smoke-security.sh   # C0-BASH-* / WRITE-SSH
./scripts/verify-readiness.sh # 汇总 Critical-0 + readiness blockers
```

## 三连绿（发布前）

```bash
for i in 1 2 3; do ./scripts/smoke-security.sh || exit 1; done
```
