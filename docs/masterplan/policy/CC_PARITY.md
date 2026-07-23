# CC_PARITY — Claude Code / Claw 行为对照（开工模板）

> **用途：** M3 启动即用；≥40 条目标，本文先填 **前 10 条**（可勾选扩展）。  
> **底盘：** Grok Build（`agent/`），不是 claw runtime。  
> **规格源：** `~/Desktop/claw-code-main`（bash_validation / permission / file_ops / mock_parity_scenarios.json）  
> **状态：** `已有` = Grok 已覆盖 · `部分` = 有但弱 · `缺失` = 需补 · `不做` = 明确放弃  

**填写约定：** 实现后把「状态」改为已有/部分，并填「测试」路径。

---

## 总进度

| 指标 | 值 |
|------|-----|
| 已填条目 | 10 / 40+ |
| 目标完成率 | ≥ 80%「已有或已补」 |
| 配套 harness | 见文末 12 场景（来自 claw mock parity，**不是** coding eval） |

---

## 第一批 10 条（已填模板）

| ID | 行为 | Claw/CC 参考 | Grok 落点（预期） | 状态 | 测试 |
|----|------|--------------|-------------------|------|------|
| F01 | 工作区外 write 拒绝 | file_ops workspace boundary | permission + file tools | 部分 | 待：写 `/tmp` 外敏感路径 |
| F02 | 二进制/超大 read 有界 | file_ops MAX_READ | read_file 截断/拒 | 部分 | 待 |
| F03 | symlink 逃逸不写出工作区 | file_ops canonical | path 规范化 | 部分 | 待 |
| B01 | `&&` `\|\|` `;` 分段鉴权 | bash_validation + decompose | `permission/bash_command_splitting.rs` | 部分 | 待：`echo ok && rm -rf /` |
| B02 | 破坏性 `rm -rf /` 类 deny | destructiveWarning | bash + GUARD L4 | **缺失→M1补** | `scripts/smoke-security.sh` |
| B03 | `curl \| bash` 下载执行 deny | commandSemantics | GUARD L6 | **缺失→M1补** | smoke-security |
| B04 | 只读命令可自动过（ls/cat/git status） | readOnlyValidation | Grok 只读 shell 列表 | 已有 | 文档对照 |
| P01 | plan 模式禁止写代码文件 | permission plan | session plan mode | 已有 | 手工：plan 下 search_replace 失败 |
| P02 | bypass 仍尊重 deny 规则 | enforcer deny wins | permission pipeline | 已有/待确认 | 待：bypass + rm -rf / |
| P03 | 危险命令不吃「记住允许」前缀 | dangerous list | remembered grants | 已有/待确认 | 待 |

---

## 扩展槽位（11–40，M3 填）

复制行追加：

| ID | 行为 | Claw/CC 参考 | Grok 落点 | 状态 | 测试 |
|----|------|--------------|-----------|------|------|
| B05 | 外泄 curl -d @.env | Lumen GUARD L1 | bash guard | 缺失 | |
| B06 | 读 `~/.ssh/id_rsa` | Lumen GUARD L2 | bash guard | 缺失 | |
| B07 | base64\|sh | Lumen GUARD L5 | bash guard | 缺失 | |
| B08 | 零宽绕过 rm | Lumen ZWSP | L0 sanitizer | 缺失 | |
| B09 | bash 超时 | claw bash timeout | bash tool | 部分 | |
| B10 | 输出截断 | claw/truncate | bash tool | 部分 | |
| F04 | search_replace 锚点失败明确错误 | edit tool | search_replace | 部分 | |
| F05 | grep 分块不丢 | grep_chunk_assembly | grep tool | 部分 | |
| S01 | 流式无 tool | streaming_text | sampler/pager | 已有 | |
| S02 | 单轮多 tool | multi_tool_turn | agent loop | 已有 | |
| S03 | tool 失败可继续 | — | agent loop | 部分 | |
| S04 | compact 可观测 | auto_compact_triggered | compaction | 部分 | |
| S05 | session resume | — | session | 已有 | |
| S06 | MCP 一轮 | plugin_tool_roundtrip | mcp | 部分 | |
| P04 | acceptEdits：编辑自动 bash 问 | permission modes | modes | 已有 | |
| P05 | dontAsk 无规则则拒 | dontAsk | modes | 已有 | |
| … | 补到 ≥40 | | | | |

---

## 与 coding eval 的边界（勿混）

| 体系 | 目的 | 来源 | 是否算「eval 20 任务」 |
|------|------|------|------------------------|
| **CC_PARITY + mock parity 12** | 工具/权限/会话**行为**回归 | claw `mock_parity_scenarios.json` | **否**（属 M3 harness） |
| **Coding eval ≥20** | 模型+agent **修 bug 质量** | 见 `policy/EVAL_BASELINE.md` | **是**（属 M4） |

---

## Mock parity 12 场景清单（M3 `scripts/parity-run.sh`）

直接改编自 claw（名称保持可追溯）：

1. streaming_text  
2. read_file_roundtrip  
3. grep_chunk_assembly  
4. write_file_allowed  
5. write_file_denied  
6. multi_tool_turn_roundtrip  
7. bash_stdout_roundtrip  
8. bash_permission_prompt_approved  
9. bash_permission_prompt_denied  
10. plugin_tool_roundtrip  
11. auto_compact_triggered  
12. token_cost_reporting  

实现：优先 **Grok headless + fixture 工作区**；不必跑 claw 二进制。  
