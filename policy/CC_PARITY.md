# CC_PARITY — Claude Code / Claw 行为对照

> **底盘：** Grok Build（`agent/`）+ Lumen 注入（`lumen-guard` / `lumen-discipline`）  
> **规格源：** `~/Desktop/claw-code-main`（bash_validation / mock_parity_scenarios.json）  
> **状态：** `已有` · `部分` · `缺失` · `不做`  
> **验收：** `./scripts/parity-run.sh`（目标完成率 ≥80% 已有或已补）

---

## 总进度

| 指标 | 值 |
|------|-----|
| 已填条目 | **41** / 40+ |
| 已有+部分（计入完成） | 见 harness 输出 |
| 配套 harness | `scripts/parity-run.sh` + `policy/parity_scenarios.json`（12 场景） |

---

## A. 文件 / 路径（F）

| ID | 行为 | Claw/CC 参考 | Grok/Lumen 落点 | 状态 | 测试 |
|----|------|--------------|-----------------|------|------|
| F01 | 工作区外敏感写拒绝 | file_ops boundary | permission + **lumen-guard L3** | **已有** | smoke-security / lumen-guard writepath |
| F02 | 二进制/超大 read 有界 | MAX_READ | read_file 截断 | 部分 | tools read_file |
| F03 | symlink 逃逸限制 | canonical path | path 规范化 + sandbox | 部分 | sandbox + path utils |
| F04 | search_replace 锚点失败明确错误 | edit tool | search_replace | 部分 | tools search_replace |
| F05 | grep 分块不丢 | grep_chunk_assembly | grep tool | 部分 | parity scenario |
| F06 | 写 `~/.ssh/*` deny 全模式 | writepath | **lumen-guard L3** | **已有** | lumen-guard writepath tests |
| F07 | 写 git hooks deny | writepath | **lumen-guard L3** | **已有** | lumen-guard |
| F08 | 写 `/etc` 等系统路径 deny | writepath | **lumen-guard L3** | **已有** | lumen-guard |
| F09 | 写 home shell rc deny | writepath | **lumen-guard L3** | **已有** | lumen-guard |
| F10 | 正常项目路径可写 | file_ops | tools + allow | **已有** | writepath allows_project |

---

## B. Bash / 安全（B）

| ID | 行为 | Claw/CC 参考 | Grok/Lumen 落点 | 状态 | 测试 |
|----|------|--------------|-----------------|------|------|
| B01 | `&&` `\|\|` `;` 分段鉴权 | bash decompose | bash_command_splitting + **lumen-guard chain** | **已有** | lumen-guard blocks_segment_chain |
| B02 | `rm -rf /` deny | destructive | **lumen-guard L2** | **已有** | smoke-security |
| B03 | `curl \| bash` deny | download-exec | **lumen-guard L2** | **已有** | smoke-security |
| B04 | 只读命令可自动过 | readOnly | is_safe_command_words | **已有** | manager tests |
| B05 | 外泄 `curl -d @.env` deny | L1 exfil | **lumen-guard** | **已有** | lumen-guard |
| B06 | 读 `~/.ssh/id_rsa` deny | L2 sensitive | **lumen-guard** | **已有** | smoke-security |
| B07 | `base64 \| sh` deny | L5 encoded | **lumen-guard** | **已有** | lumen-guard |
| B08 | 零宽绕过 rm 仍 deny | ZWSP | **lumen-guard L0** | **已有** | smoke-security |
| B09 | bash 超时 | timeout | bash tool timeout | 部分 | tools |
| B10 | bash 输出截断 | truncate | bash tool | 部分 | tools |
| B11 | YOLO/bypass 不绕过 hard-deny | deny wins | manager **before YOLO** | **已有** | manager lumen_guard_deny |
| B12 | home 数据目录整盘 wipe deny | destructive rm | **lumen-guard** | **已有** | blocks_home_data_wipe |
| B13 | 安全 pipe（curl\|jq）允许 | — | pipe-to-shell 白名单外 | **已有** | safe_commands tests |

---

## C. 权限模式（P）

| ID | 行为 | Claw/CC 参考 | Grok/Lumen 落点 | 状态 | 测试 |
|----|------|--------------|-----------------|------|------|
| P01 | plan 模式禁止写代码 | plan mode | session plan | **已有** | shell plan mode |
| P02 | bypass 仍尊重 deny | enforcer | lumen-guard + policy deny | **已有** | B11 + policy_deny |
| P03 | 危险命令不吃「记住允许」 | dangerous | is_dangerous_command_words | **已有** | manager evaluate_bash |
| P04 | acceptEdits：编辑自动 bash 问 | modes | permission modes | **已有** | config modes |
| P05 | dontAsk 无规则则拒 | dontAsk | modes | **已有** | modes |
| P06 | always-approve 仍拦 hard-deny | YOLO | lumen_guard_deny first | **已有** | manager.rs |

---

## D. 会话 / 流式 / 工具循环（S）

| ID | 行为 | Claw/CC 参考 | Grok/Lumen 落点 | 状态 | 测试 |
|----|------|--------------|-----------------|------|------|
| S01 | 流式无 tool | streaming_text | sampler/pager | **已有** | parity scenario |
| S02 | 单轮多 tool | multi_tool | agent loop | **已有** | parity scenario |
| S03 | tool 失败可继续 | — | agent loop | 部分 | — |
| S04 | compact 可观测 | auto_compact | compaction | 部分 | parity scenario |
| S05 | session resume | — | session | **已有** | shell resume |
| S06 | MCP 一轮 | plugin_tool | mcp | 部分 | parity scenario |
| S07 | Storm 同错 ≥3 nudge | Reasonix/Lumen | **lumen-discipline** | **已有** | smoke-m2 |
| S08 | 重复成功 thrash 提醒 | Lumen | **lumen-discipline** | **已有** | smoke-m2 |
| S09 | 假完成 SoftOnce 闸 | Delivery/Goal | update_goal gate | **已有** | smoke-m2 |
| S10 | headless usage 含 cache | token_cost | cache_line | **已有** | notification.rs |

---

## E. 模型 / 配置（M）

| ID | 行为 | 参考 | 落点 | 状态 | 测试 |
|----|------|------|------|------|------|
| M01 | 默认 DeepSeek chat | Lumen M0 | default_models.json | **已有** | assert-defaults |
| M02 | reasoner preset | M2 | deepseek-reasoner | **已有** | default_models.json |
| M03 | local OpenAI-compatible preset | M2 | local-openai hidden | **已有** | default_models.json |
| M04 | BYOK base_url/env_key | M0 | config default_models() | **已有** | smoke-deepseek |
| M05 | auto_update 默认 off | M0 | registry/update | **已有** | assert-defaults |
| M06 | 遥测 Mixpanel 默认 off | M0 | TelemetryConfig | **已有** | M0 tests |

---

## 与 coding eval 的边界

| 体系 | 目的 | 是否算 eval 20 |
|------|------|----------------|
| **CC_PARITY + 12 场景** | 行为回归 | **否**（M3） |
| **Coding eval ≥20** | 修 bug 质量 | **是**（M4） |

---

## Mock parity 12 场景

见 `policy/parity_scenarios.json`，由 `scripts/parity-run.sh` 驱动。  
实现优先 **单元/结构证明**，不必跑 claw 二进制。
