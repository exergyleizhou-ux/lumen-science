# EVAL 基线 ≥20 任务 — 来源与组装（写死）

> **问题：** 20 任务从哪来？  
> **答案：分层组装，不是单一来源。**

---

## 1. 三类评测勿混用

| 类型 | 数量目标 | 来源 | 作用 | 阶段 |
|------|----------|------|------|------|
| **A. Coding agent 任务** | **≥20**（M4 门禁） | 见 §2 | 修真实小项目 / pass-rate | M4 |
| **B. 行为 parity** | 12 | claw `mock_parity_scenarios.json` | 工具/权限/会话 | M3（**不计入** 20） |
| **C. 安全 smoke** | ≥6 | `02-安全规格` smoke-security | deny 规则 | M1（**不计入** 20） |

**v1.0 说的「eval ≥20」= 仅类型 A。**

---

## 2. Coding 20 任务来源拆分（合计 ≥20）

### Tier 1 — 迁移 Lumen 现成（**8**）

路径：`~/lumen/evals/tasks/`（及 `baseline6/` 同源）

| ID | 目录 | 类型 |
|----|------|------|
| T01 | 01-average-empty | Go 空切片 / 除零 |
| T02 | 02-stack-lifo | Go 栈 |
| T03 | 03-reverse-runes | Go 字符串 |
| T04 | 04-binary-search | Go 算法 |
| T05 | 05-counter-race | Go 并发 |
| T06 | 06-stringer-impl | Go 接口 |
| T07 | 07-nilmap-write | Go nil map |
| T08 | 08-multifile-shapes | Go 多文件 |

格式：`prompt.txt` + `workspace/` + 确定性测试（现有 harness）。  
**动作：** `cp -R` → `~/code/lumen/evals/tasks/`，跑通 `lumen eval` 或等价 runner。

### Tier 2 — 新建同格式任务（**+8 = 16**）

仍放 `evals/tasks/`，**新建**（不计 claw）：

| ID | 建议 slug | 语言/主题 |
|----|-----------|-----------|
| T09 | 09-py-divzero | Python 边界 |
| T10 | 10-py-json-merge | Python |
| T11 | 11-ts-optional-chain | TS/JS |
| T12 | 12-ts-async-race | TS |
| T13 | 13-go-context-cancel | Go |
| T14 | 14-go-error-wrap | Go |
| T15 | 15-py-path-traversal-fix | Python 安全修复（应用层） |
| T16 | 16-go-http-timeout | Go |

每条必须：broken workspace + `prompt.txt` + **可自动判分**（测试命令 exit 0）。

### Tier 3 — 扩展到 ≥20（**+4 = 20**）

| ID | 建议 | 说明 |
|----|------|------|
| T17 | 17-multi-pkg-go | 多 package |
| T18 | 18-fix-only-regression | 修 A 不破 B |
| T19 | 19-readme-driven | 按 README 补实现 |
| T20 | 20-flaky-to-stable | 去掉 flaky sleep |

可再加 T21+ 不设上限；门禁是 **≥20 可跑且有基线表**。

---

## 3. 明确 **不是** coding eval 来源

| 来源 | 为何不算进 20 |
|------|----------------|
| claw mock parity 12 | 测 harness 行为，不测「修 bug 对不对」 |
| Reasonix e2ebench | 可选后期，v1 不依赖 |
| 手工聊天截图 | 不可复现 |

claw 场景 → 只进 **M3 parity-run**，见 `policy/CC_PARITY.md`。

---

## 4. 基线怎么记

文件：`evals/BASELINE.md`（发布时）

```markdown
| Model | Date | Pass | Median steps | Notes |
| deepseek-v4-pro | YYYY-MM-DD | k/20 | n | |
```

命令（目标形态）：

```bash
# 从 monorepo 根
./scripts/eval-coding.sh --model deepseek-v4-pro --tasks evals/tasks
```

实现前期可用旧 `~/lumen` 的 `lumen eval` 指到迁出后的 tasks 目录。

---

## 5. M4 清单

- [ ] Tier1 8 任务迁入且可跑  
- [ ] Tier2 8 任务新建且可跑  
- [ ] Tier3 4 任务新建且可跑  
- [ ] DeepSeek 至少跑一轮 → `evals/BASELINE.md`  
- [ ] 与 parity 12、smoke-security **分目录/分脚本**，CI 不混报  
