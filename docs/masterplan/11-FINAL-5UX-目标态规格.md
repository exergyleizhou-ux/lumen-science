# Lumen TUI FINAL-5UX

状态：目标态 UX 规格，可直接进入工程拆解  
日期：2026-07-17  
审查基线：`/Users/lei/code/lumen` @ `21ef0796e9d4a384ba545a9983d93e35a2c74a76`  
设计依据：`DESIGN-IS-2026-07-17`，得分 `9/30`，结论 `REDESIGN`  
版本策略：直接从当前体验切到 FINAL-5UX，不发布 FINAL-3/4 的中间壳层，不长期维护双 TUI

## 1. 最终决定

FINAL-5UX 不是给当前 Grok 界面换 Lumen 文案，也不是另起一套 TUI。它保留当前 Ratatui 主循环、prompt/editor、picker、permission、diff、dashboard、low-color quantization 和 session 能力，重做三件承重的事：

1. 用户始终知道当前产品是 **Lumen**，provider/model 是独立事实，上游归属只出现在 About/Legal。
2. 用户在首次启动三分钟内知道当前模型是否真的 **tool-ready**，而不是把能聊天误认成能完成任务。
3. 每次任务都能看到真实的权限、安全、缓存和验证状态，任何 `Ready`、`Verified`、`Cache hit` 都有运行时证据。

一句话记忆点：

> Lumen 第一眼就告诉你它是谁、现在能不能真正动手、改完是否已经验证。

## 2. 不可妥协原则

### 2.1 身份单一

- 产品名只显示 `Lumen`。
- `Grok` / `Grok Build` 只允许出现在 About、Legal、upstream diagnostics 和用户明确选择的 xAI provider 名称里。
- release channel 来自真实版本数据，例如 `0.1.220-alpha.4`，禁止硬编码 `Beta`。
- terminal title 默认格式：`Lumen · {repo} · {phase}`，无 session 时为 `Lumen`。

### 2.2 状态必须可证明

- `Model connected` 只表示凭据和 endpoint 可用。
- `Tool-ready` 只表示当前 provider + model + capability fingerprint 已通过真实 tool-call probe。
- `Verified` 只表示当前 working tree 内容在最后一次变更之后通过了明确命令。
- `Cache hit` 只在 provider 返回可核验 usage 数据时显示；无数据时显示 `Cache metrics unavailable`，不能显示 `0%`。
- `Safe` 不作为模糊总称。界面显示准确模式：`Ask`、`Accept edits`、`Don't ask`、`Bypass`，并单独显示 hard-deny 是否生效。

### 2.3 不假装发布就绪

- Session 级 `Tool-ready` 与产品级 release readiness 是两套概念。
- 用户界面可以显示 `Tool-ready`，但在 M5/M6 未通过时不得显示 `Production ready`、`Best-in-class` 或类似承诺。
- dev/private-alpha build 的 About/diagnostics 必须显示当前 readiness 状态和 blocker；普通任务界面不持续打扰用户。

### 2.4 一个底盘

- fullscreen 与 minimal 共用同一状态模型、文案和动作语义。
- 不复制业务逻辑到第二套 renderer。
- 不用永久 feature flag 保留旧壳层。
- 内部 crate 名可以继续是 `xai-grok-*`，它不是本次 UX 切换的阻塞项。

## 3. 用户与核心任务

### 3.1 核心用户

- 终端内工作的开发者。
- 可能第一次使用 Lumen，不理解 Lumen、provider、model、tool capability、permission mode 的关系。
- 需要把一次真实代码任务交给 agent，并能判断它是否安全、是否真的完成。

### 3.2 核心任务

1. 在 repo 中启动 Lumen。
2. 选择或确认 provider/model。
3. 证明当前 model 可以真实调用工具。
4. 提出修改请求。
5. 理解 agent 当前阶段。
6. 审批风险动作和 diff。
7. 看见新鲜、可追溯的验证结果。
8. 出错时恢复，而不是重开 session 猜问题。

### 3.3 成功标准

| 指标 | FINAL-5UX 目标 |
|---|---|
| 首次可输入 | cached 本地配置下 p95 ≤ 500 ms；冷启动 p95 ≤ 1.5 s，网络不得阻塞输入 |
| 首次 tool-ready | 已有 key 时目标 ≤ 30 s；无 key 的人工路径目标 ≤ 3 min |
| M5 陌生用户 | install → key → real read/edit/verify → security explanation ≤ 10 min |
| 身份正确率 | 新用户能准确说出产品、provider、model、config source，5 项全对 |
| 状态真实性 | 任何 `Tool-ready` / `Verified` / `Cache hit` 都可追到证据对象 |
| 键盘可达性 | 每个 click hit area 都有可发现的 keyboard 等价动作 |
| idle 开销 | 无活跃任务、无提示过期、无动画时不调度持续 redraw |
| 屏幕覆盖 | 80×24、120×40、180×50；fullscreen/minimal；truecolor/256/16-color |

## 4. 设计方向

### 4.1 视觉论点

**Quiet Instrument，安静的仪器。** Lumen 像一台可信的开发仪器，不像品牌海报。内容和证据是主体，产品 chrome 退到背景。唯一可记忆的品牌信号是暖光色和清晰的 `Lumen` 名称。

### 4.2 SAFE 与 RISK

SAFE，用户预期且应保留：

- 终端原生单栏 transcript + 底部 prompt。
- plan、diff、permission、dashboard 的既有交互语法。
- keyboard-first，mouse 是同等入口，不是唯一入口。
- 低色终端与 `NO_COLOR` 降级。

RISK，Lumen 的独立面孔：

- 不再用大型 logo 证明品牌，改用持续、真实的 truth bar 证明产品价值。
- 不把 provider logo 或模型品牌染成彩虹；provider 是数据，状态颜色只表达语义。
- 把 `Tool-ready` 和 `Verified` 做成必须有证据的状态，而不是乐观文案。

成本：首屏更克制，少了表演性。收益：可信、快、可扫描，且不会继续像换皮 Grok。

## 5. 终端设计系统

### 5.1 字体

Lumen 不控制宿主终端字体。文档与 preview 推荐顺序：

1. JetBrains Mono
2. Berkeley Mono
3. IBM Plex Mono
4. 任意用户终端 monospace

界面层级只使用：`normal`、`bold`、`dim`、`underline`。禁止用 dim 承载必读信息。数字使用 tabular glyph 时由宿主字体提供。

### 5.2 Dark palette

| Token | Hex | 对 `#111315` 对比度 | 用途 |
|---|---:|---:|---|
| `bg.base` | `#111315` | - | 主背景 |
| `bg.surface` | `#181B1F` | - | modal、展开块 |
| `bg.raised` | `#22272D` | - | 选中行、diff header |
| `text.primary` | `#E8EDF2` | 15.81:1 | 主文本 |
| `text.secondary` | `#C5CDD5` | 11.59:1 | 次级文本 |
| `text.muted` | `#9AA6B2` | 7.51:1 | 辅助信息，仍可读 |
| `text.dim` | `#7F8A95` | 5.30:1 | 非关键时间戳、快捷键 |
| `accent.lumen` | `#E8B84A` | 10.10:1 | Lumen、焦点、当前选择 |
| `state.info` | `#7DB4FF` | 8.73:1 | reading、连接信息 |
| `state.success` | `#80C995` | 9.49:1 | tool-ready、verified pass |
| `state.error` | `#FF7B86` | 7.47:1 | blocked、failed |
| `state.verify` | `#C4A7FF` | 9.15:1 | verifying，仅用于验证阶段 |

规则：

- 默认主题实际引用色上限 12 个 RGB，不含动态 diff 混色。
- `accent.lumen` 不等于 warning。warning 必须同时有 `!`、明确文字和边框，不依赖颜色。
- provider/model 不使用专属颜色；统一用 `text.secondary`。
- focus 至少同时使用颜色 + 光标/反转/下划线之一。

### 5.3 Light palette

| Token | Hex | 对 `#F5F2EA` 对比度 |
|---|---:|---:|
| `bg.base` | `#F5F2EA` | - |
| `bg.surface` | `#FFFFFF` | - |
| `text.primary` | `#1B2026` | 14.65:1 |
| `text.secondary` | `#38434D` | 9.03:1 |
| `text.muted` | `#53606B` | 5.77:1 |
| `accent.lumen` | `#775900` | 5.84:1 |
| `state.info` | `#005EA8` | 5.93:1 |
| `state.success` | `#26734D` | 5.15:1 |
| `state.error` | `#A0434D` | 5.51:1 |
| `state.verify` | `#6647A8` | 6.19:1 |

### 5.4 ANSI 降级

| 语义 | 256-color | 16-color | NO_COLOR |
|---|---|---|---|
| Lumen/focus | yellow | bright yellow | bold + `>` |
| info | blue | bright blue | `i` 前缀 |
| success | green | bright green | `✓` 前缀 |
| error | red | bright red | `✗` 前缀 |
| verifying | magenta | bright magenta | `◇` 前缀 |
| muted | gray | bright black | 普通文本 + 括号 |

颜色消失后，状态仍必须通过 glyph、label、顺序完整表达。

### 5.5 间距

单位是 terminal cell：

- `0`：紧邻，适用于 status label 内部。
- `1`：默认行间距和紧凑左右 padding。
- `2`：标准 panel 左右 padding。
- `3`：wide layout 的组间距。

不引入更多离散 spacing token。80×24 禁止纯装饰空行。

### 5.6 Motion

- 默认 `motion = off`。
- spinner 只在后台操作真实进行时显示，最大 8 FPS。
- 进入/退出 panel 不做逐帧动画，直接切换并保持焦点。
- `reduced_motion = true` 或等价系统信号时强制关闭全部非必要动画。
- idle welcome 不调度持续 tick。
- logo 若保留，只是静态 1 行 wordmark，不出现 shimmer/breathing。

## 6. 信息架构

```text
Lumen
├── Startup
│   ├── Legacy config migration
│   ├── Provider + credential
│   ├── Model selection
│   ├── Capability probe
│   └── Workspace trust
├── Welcome
│   ├── Identity header
│   ├── Truth bar
│   ├── Prompt
│   ├── Resume / Worktree / Dashboard
│   └── Diagnostics / About
├── Session
│   ├── Transcript
│   ├── Work phase groups
│   ├── Permission / Question
│   ├── Plan / Diff
│   ├── Verification result
│   └── Prompt
├── Dashboard
│   ├── Action required
│   ├── Working
│   ├── Verifying
│   ├── Idle / Complete
│   └── Dispatch / Peek / Takeover
└── Details
    ├── /status
    ├── /doctor
    ├── /model
    ├── /permissions
    ├── /config
    └── /about
```

## 7. 全局框架

### 7.1 固定区域

1. Header：产品、repo、branch、真实版本 channel。
2. Truth bar：provider/model、capability、permission、cache、verification。
3. Content：welcome/transcript/dashboard/modal。
4. Prompt：用户输入与当前 prompt mode。
5. Context hints：只显示当前焦点可用动作。

### 7.2 Truth bar

标准宽度：

```text
DeepSeek · deepseek-chat  |  ✓ Tool-ready  |  Ask + hard-deny  |  Cache 82% hit  |  ✓ Verified 12s ago
```

80 列：

```text
DeepSeek/deepseek-chat · ✓ tools · Ask · ✓ verify
```

未知或失败必须显式：

```text
DeepSeek/deepseek-chat · ? capability unknown · Ask · - not verified
DeepSeek/deepseek-chat · ✗ chat-only · Ask · ✗ verify failed
```

Truth bar 本身可点击；keyboard 入口统一为 `/status`。不新增未经冲突审查的全局 Ctrl shortcut。

### 7.3 状态优先级

显示优先级从高到低：

1. Hard block：凭据失效、hard-deny、工作区不可信、verify failed 且任务宣称完成。
2. User action required：permission、question、plan approval、migration choice。
3. Recovery：provider degraded、tool capability lost、cache/verification unavailable。
4. Active work：Understanding、Reading、Editing、Verifying。
5. Fresh success：Verified、Tool-ready。
6. Passive info：cache、token、elapsed time、version。

低优先级状态不能覆盖高优先级状态，也不能用成功色掩盖 blocker。

## 8. 响应式尺寸

### 8.1 80×24，Compact

- 无 logo、无 hero box、无右栏。
- Header 1 行，truth bar 1 行。
- 只保留当前任务所需的 2 到 4 个动作。
- modal 占满 content 区，Esc 返回原焦点。
- 长 provider/model 使用中间省略，详情可在 `/status` 查看。

```text
Lumen 0.1.220-alpha.4 · repo/main
DeepSeek/deepseek-chat · ✓ tools · Ask · - not verified
────────────────────────────────────────────────────────────────────────────────
What do you want to change?

❯ _

Ctrl+S resume   Ctrl+W worktree   Ctrl+. shortcuts
```

### 8.2 120×40，Standard

- 单栏主内容，truth bar 使用完整 labels。
- Welcome actions 与最近 session 分成两个块。
- Transcript tool groups 默认折叠，失败和验证结果默认展开。

```text
Lumen 0.1.220-alpha.4                                      repo/main
DeepSeek · deepseek-chat | ✓ Tool-ready | Ask + hard-deny | - Not verified
────────────────────────────────────────────────────────────────────────────────────────────────────────
Ready for a real task
Tool calls were proven for this model 18m ago. Lumen will ask before risky commands.

❯ Describe a change, or @ a file

Resume recent
  checkout timeout · main · verified yesterday
  add rate limit    · feat/rate-limit · verification failed

Ctrl+S resume   Ctrl+W worktree   /status details   Ctrl+. shortcuts
```

### 8.3 180×50，Wide

- Content 仍是主角，不默认常驻复杂 sidebar。
- Session 可打开 36 列 context rail，显示 plan、changed files、verification；默认关闭。
- Dashboard 可使用更宽列展示 repo/model/phase，但排序语义不变。

## 9. 屏幕规格

### S0. Legacy config migration

触发：检测到 `~/.grok` 或 `GROK_HOME`，且 Lumen home 尚未初始化。

```text
Lumen found an existing Grok configuration

Source: ~/.grok/config.toml
Will import: models 4 · MCP servers 2 · UI preferences 6
Will not import: login tokens · update channel · product telemetry identity

> Import to ~/.lumen and continue       recommended
  Use the legacy config for this run
  Start clean

Nothing changes until you confirm.  v preview  Enter choose  Esc quit
```

要求：

- 默认不写盘，先展示 diff/summary。
- token、cookie、session 不隐式复制。
- 冲突逐项标出来源和最终值。
- Import 成功后写 migration receipt，包含时间、source hash、target、版本，不含 secrets。
- Import 失败不修改原配置，提供 retry、start clean、copy diagnostics。

### S1. Provider and credential

```text
Connect a model provider

> DeepSeek       DEEPSEEK_API_KEY found · value hidden
  OpenAI         OPENAI_API_KEY not found
  Anthropic      ANTHROPIC_API_KEY not found
  xAI            Sign in or use XAI_API_KEY
  Local/OpenAI   Configure endpoint

Enter continue   e edit source   /doctor details
```

要求：

- DeepSeek 是推荐默认，不是强制默认。
- 凭据来源只显示 env/config/command/system store，不显示值。
- xAI browser auth 只在选择 xAI provider 后出现。
- provider 错误必须属于 provider，不得写成 Lumen 整体不可用。

### S2. Capability probe

```text
Checking agent capability

✓ Endpoint reachable                 412 ms
✓ Model resolved                     deepseek-chat
✓ Structured tool call               read_file
✓ Tool result accepted               fixture.txt
✓ Follow-up completed                1 turn

Tool-ready · fingerprint ds-chat:8f31 · checked now
Enter start using Lumen   d details   r run again
```

Probe 合同：

- 使用隔离临时 fixture，不读取/修改用户 repo。
- 必须收到结构化 tool call，散文或代码块算 FAIL。
- 结果绑定 provider、model id、base URL identity、tool schema version、binary version。
- 任一绑定字段变化，状态变成 `Capability unknown`，后台可重测。
- local model 调用现有 `probe-local` 语义，不另造宽松路径。

### S3. Ready welcome

- 去除 Grok Build hero、Beta、subscription 级产品阻塞。
- 静态 `Lumen` wordmark 最多 1 行，仅在 120×40 以上显示。
- 主 CTA 永远是 prompt。
- 次要动作：resume、worktree、dashboard、model/status。
- announcement 默认不抢首屏；release/security blocker 才进入高优先级消息槽。

### S4. Active session

工作输出按用户可理解的阶段聚合：

```text
Understanding
  Read AGENTS.md and mapped the checkout flow

Reading 6 files                                            1.2s
  src/cart.ts, src/checkout.ts, tests/checkout.test.ts ...     v expand

Editing 2 files
  M src/checkout.ts   +18 -7
  M tests/checkout.test.ts +24 -0

Verifying
  ✓ npm test -- checkout.test.ts                           4.8s

Done
  Fixed timeout handling and added regression coverage.
  ✓ Verified against the current files 3s ago
```

规则：

- 默认展示结果和用户意义，不默认展示原始 RPC/tool JSON。
- `v`/click 展开 raw detail，展开状态可单独保存。
- Reading/Editing/Verifying 由真实事件产生，不由模型文本猜测。
- 一个阶段只显示一个主状态；并行 subagent 在 dashboard/child block 表达。

### S5. Permission

```text
Permission required · shell

npm install fast-check

Why: add property-based coverage for the parser
Risk: writes package.json, lockfile, and node_modules; network access
Scope: this command in /repo

> Allow once
  Allow matching `npm install fast-check` in this session
  Reject and tell Lumen why

1-3 choose   ←/→ scope   Enter confirm   Esc reject
```

规则：

- 先显示 action、why、risk、scope，再显示选项。
- `Always allow` 必须带明确 tool/server/command scope，禁止无边界的 always。
- hard-deny 直接解释被拦原因和 policy source，不提供虚假 override。
- mouse 与 keyboard 选项完全一致。

### S6. Plan and diff

- 复用当前 plan/diff chassis。
- Plan 每一步显示 outcome，不显示纯实现动作堆积。
- Diff header 显示文件、行数、是否已验证。
- 用户修改 plan 后，所有旧 approval 失效。
- diff 变化后，旧 verification 立即变 `Stale`。

### S7. Verification

Passed：

```text
✓ Verified
Command: cargo test -p checkout
Exit: 0   Duration: 18.4s   Finished: 14:32:09
Covers: 42 tests · changed files newer than start: no
Evidence: run 01K0... · copy
```

Failed：

```text
✗ Verification failed
Command: cargo test -p checkout
Exit: 101   Failed: 2/42   Duration: 11.8s
First failure: checkout::timeout_retries

> Let Lumen repair within budget 1/2
  Show full output
  Stop and keep changes
```

状态：

- `Not run`
- `Running`
- `Passed(fresh)`
- `Failed`
- `Stale(changes_after_run)`
- `Unavailable(reason)`

只有 `Passed(fresh)` 渲染 `Verified`。

### S8. Dashboard

排序固定：

1. Permission/question/plan approval required
2. Verification failed/blocked
3. Working
4. Verifying
5. Idle/complete

Row 最小字段：

```text
◆ checkout timeout · repo/main · DeepSeek · permission required · now
✗ rate limit       · api/feat  · OpenAI   · verify failed       · 2m
⋅ docs cleanup     · docs/main · Local    · editing 2 files     · 18s
✓ parser fix       · core/main · Claude   · verified            · 4m
```

要求：

- 保留 dispatch、peek/reply、takeover、group、pin、new agent。
- 状态来源与 session truth snapshot 相同，不在 dashboard 重新推断。
- 行内 quick action 只在需要用户输入时出现。
- 关闭 dashboard 不停止 session。

### S9. `/status`

```text
Lumen status

Product      Lumen 0.1.220-alpha.4 (21ef079)
Config       ~/.lumen/config.toml · imported from ~/.grok on 2026-07-17
Provider     DeepSeek · env DEEPSEEK_API_KEY · connected 38 ms
Model        deepseek-chat · context 128k
Capability   Tool-ready · checked 18m ago · fingerprint 8f31
Permission   Ask · hard-deny active · source managed policy
Cache        82% hit · provider reported · saved 18.2k input tokens
Verification Passed · fresh · cargo test -p checkout · run 01K0...
Workspace    /repo · trusted · git dirty 2 files

d diagnostics   c copy redacted report   Esc close
```

所有字段都有 `value + source + freshness`。未知必须写 unknown 和原因。

### S10. Recovery

错误结构固定：

1. What failed
2. What remains safe/intact
3. Likely cause
4. Best next action
5. Alternatives
6. Redacted diagnostics

```text
Provider connection expired

Your files and session are intact. The last verified state is still available.
Cause: DeepSeek returned 401 while refreshing the current request.

> Re-check DEEPSEEK_API_KEY and retry
  Switch provider/model
  Continue in read-only mode
  Copy redacted diagnostics
```

禁止只有 `Something went wrong`、无限 spinner 或只给 Quit。

## 10. 核心流程

### F1. 已有 DeepSeek key 的首次启动

`launch → detect key → confirm provider/model → capability probe → trust repo → ready welcome`

目标：30 秒内到可输入，probe 失败仍可进入 read-only explanation mode，但不能显示 Tool-ready。

### F2. 无 key 首次启动

`launch → provider picker → exact credential instruction → re-detect → capability probe → trust → ready`

Credential instruction 必须可复制，并明确“value stays hidden”。

### F3. Local model

`choose local → endpoint test → model list → real tool probe → tool-ready/chat-only → ready or recovery`

Chat-only model 可以用于解释/问答，但在用户要求编辑时必须先解释限制并建议切换，不得假装 agent-ready。

### F4. Real edit

`prompt → understanding → reading → permission if needed → editing → diff → verifying → fresh result`

任何用户/agent 后续 edit 都使 verification stale。

### F5. Permission denied

`permission → reject + optional feedback → agent replans → no repeated identical prompt unless context changed`

### F6. Provider failure mid-run

`failure → preserve session/run id → show recovery → refresh/switch → replay from last durable event → continue`

不丢 transcript，不重复已确认的危险动作。

### F7. Resume

`resume → load durable events → reconstruct truth snapshot → mark stale external facts → recheck provider/capability as needed`

旧 session 的 `Tool-ready` 和 cache 数据不能无条件继承。

## 11. Truth data contract

建议新增一个只负责派生显示事实的 contract，不把 renderer 变成第二套业务逻辑：

```rust
struct TruthSnapshot {
    product: ProductIdentity,
    config: ConfigSourceSummary,
    provider: ProviderState,
    model: ModelState,
    capability: CapabilityState,
    permission: PermissionSummary,
    cache: CacheSummary,
    verification: VerificationSummary,
    workspace: WorkspaceSummary,
    phase: WorkPhase,
    attention: Option<AttentionItem>,
    captured_at: SystemTime,
}

enum CapabilityState {
    Unknown { reason: String },
    Checking,
    ChatOnly { evidence_id: String },
    ToolReady { fingerprint: String, checked_at: SystemTime, evidence_id: String },
    Failed { reason: String, evidence_id: Option<String> },
}

enum VerificationSummary {
    NotRun,
    Running { command: String, run_id: String },
    Passed { command: String, run_id: String, finished_at: SystemTime, source_seq: u64 },
    Failed { command: String, run_id: String, exit_code: i32 },
    Stale { prior_run_id: String, changed_at_seq: u64 },
    Unavailable { reason: String },
}
```

### 11.1 派生不变量

- `ToolReady` 必须有 fingerprint、checked_at、evidence_id。
- `Verified` 必须满足 `verification.source_seq >= current_last_change_seq`。
- provider/model/base URL/tool schema/binary 任何一项变化都使 capability 变 Unknown。
- cache `hit_ratio` 必须带 provider-reported source；estimated 数据单独标 estimated。
- UI 不允许直接从字符串或 spinner 状态推断 truth。
- Dashboard、fullscreen、minimal、terminal title 都消费同一 `TruthSnapshot`。

### 11.2 Phase

```text
Idle → Understanding → Reading → Editing → Verifying → Complete
                       ↘ WaitingForUser
                       ↘ Blocked
任何 phase → Recovering → 前一安全 phase
```

Phase 由 tool/run events 派生。模型说“我正在验证”不等于 Verifying。

## 12. Keyboard 与 focus

### 12.1 全局

| 动作 | 键盘 |
|---|---|
| shortcuts | `Ctrl+.` |
| model | 现有 `Ctrl+M` / `/model` |
| permission mode | 现有 `Ctrl+O` / `/permissions` |
| status details | `/status` |
| readiness diagnostics | `/doctor` |
| close modal/back | `Esc` |
| move focus zone | `Tab` / `Shift+Tab` |
| activate selected | `Enter` |

### 12.2 Welcome 修复

- Prompt 是初始 focus。
- `Esc` 从 prompt 到 action list。
- Up/Down 选择 action。
- Enter 必须激活选择项，不得被 `NewSession` 分支提前吞掉。
- Tab/Shift+Tab 在 prompt、actions、recent sessions 间正反循环。
- Changelog、refresh、auth URL copy、announcement expand 均必须有 keyboard action。

### 12.3 Focus 可见性

- 选中行使用 `>` + bold + background/underline 中至少两种。
- mouse hover 不能覆盖 keyboard focus。
- modal 打开记住 origin，关闭后恢复 focus。
- disabled control 仍可读，并解释为何 disabled；不可获得 activation focus。

## 13. 文案系统

### 13.1 名词

| 概念 | 标准文案 | 禁止文案 |
|---|---|---|
| 产品 | Lumen | Grok Build 作为产品名 |
| provider | DeepSeek / OpenAI / Anthropic / xAI / Local | Login to Grok 作为通用认证 |
| model | provider 返回或 config 定义的显示名 | “AI” 作为唯一身份 |
| capability | Tool-ready / Chat-only / Capability unknown | Ready，未说明 ready 什么 |
| permission | Ask / Accept edits / Don't ask / Bypass | Safe / YOLO 单独出现 |
| verification | Verified / Verification failed / Stale / Not run | Done 代替验证 |
| cache | 82% hit / unavailable / estimated | High cache 无数值和来源 |

### 13.2 状态句式

- 动作进行中：`Reading 6 files`，不是 `Working...`。
- 等待：`Waiting for permission`，不是 `Waiting`。
- 失败：`Verification failed: 2 tests`，不是 `Error`。
- 恢复：`Your files and session are intact`，先说明安全边界。
- 成功：`Verified against current files`，不是 `Looks good`。

### 13.3 版本与归属

- Header：`Lumen 0.1.220-alpha.4`。
- About：`Lumen is derived from the open-source Grok Build codebase. See NOTICE and LEGAL.md.`
- xAI provider：`xAI · grok-4.5`，这时 Grok 是 model/provider 事实，不是产品名。

## 14. Cache UX

- Truth bar 只显示最近有效窗口：`Cache 82% hit`。
- `/status` 显示 source、window、tokens、cost estimate 和 freshness。
- provider 没有 metrics：`Cache metrics unavailable from this provider`。
- 首次请求：`Cache warming`，不是 `0% hit`。
- 发生 context compaction 时单独记录，不把 compaction 当 cache hit。
- 任何“节省金额”必须标币种、provider rate source、estimated。

## 15. Security 与 privacy

- credential 只显示来源，不显示内容、前后四位或长度。
- copy diagnostics 默认脱敏：keys、cookies、auth URLs、home path、repo secrets。
- trust prompt 主体写 `Lumen`，并列出可能读取/修改/执行的能力。
- hard-deny 显示命中的 rule 和 rule source。
- `Bypass` 仍受 hard-deny，UI 必须直接写明。
- migration receipt、probe evidence、verification evidence 不包含 secret。
- telemetry 继续默认关闭；UX 不能用 confirmshaming 诱导打开。

## 16. Fullscreen 与 minimal 一致性

| 能力 | Fullscreen | Minimal |
|---|---|---|
| identity | 固定 header | session 起始一行 + title |
| truth snapshot | 固定 truth bar | 值变化时打印 delta，`/status` 查看全量 |
| permission | overlay | inline block，选项相同 |
| phase groups | 折叠 panel | scrollback summary + expand command |
| verification | result card | 完整 inline evidence block |
| recovery | modal/panel | inline block，不丢选项 |
| dashboard | fullscreen | 启动 dashboard 仍进入同一 fullscreen view |

Minimal 不是“少功能版”，只改变布局与 scrollback 所有权。

## 17. Config home 与迁移

### 17.1 新合同

默认：

- `LUMEN_HOME`，若设置则使用。
- 否则 `~/.lumen`。
- project config 使用 `.lumen/`。

兼容读取：

- `GROK_HOME` / `~/.grok` 只作为 legacy source。
- 不静默写回 legacy source，除非用户选择“Use legacy for this run”且界面明确显示。

### 17.2 优先级

```text
enforced system/managed requirement
> CLI session override
> project .lumen
> user ~/.lumen
> explicitly imported legacy snapshot
> built-in defaults
```

被 system/managed requirement 锁定的值不可被 CLI、project 或 user config 覆盖。`/status` 必须显示 `locked` 和 policy source；UX 文案不得承诺用户可以修改被锁定的值。

### 17.3 切换策略

- FINAL-5UX 首次 launch 执行一次 migration gate。
- 兼容 bridge 保留 90 天或两个 release channel，以先到者为准。
- bridge 期间每次使用 legacy source 都显示非阻塞提醒和迁移入口。
- bridge 结束后只读 legacy diagnostics，不自动读取业务配置。
- 不保留旧 UI；兼容的是数据，不是产品壳层。

## 18. 工程落点

### 18.1 现有文件

| 文件 | FINAL-5UX 责任 |
|---|---|
| `xai-grok-pager/src/views/welcome/mod.rs` | 新 startup/welcome layout，删除 Grok shell identity |
| `views/welcome/hero_box.rs` | 改为轻量 ready summary，或在 compact/default 下不渲染 |
| `views/welcome/logo.rs` | 静态 wordmark；删除 idle shimmer/pulse |
| `xai-grok-pager-minimal/src/auth.rs` | provider-scoped auth 文案和动作 |
| `xai-grok-pager-minimal/src/panel.rs` | truth delta、recovery、state parity |
| `xai-grok-pager/src/app/cli.rs` | Lumen 命令文案、LUMEN_HOME help、legacy migration flags |
| `xai-grok-pager/src/notifications/title.rs` | `Lumen · repo · phase` title contract |
| `xai-grok-pager/src/app/app_view.rs` | truth snapshot 注入、focus 修复、统一 actions |
| `xai-grok-pager/src/app/event_loop.rs` | 首屏不等网络、truth 更新、idle tick gating |
| `views/agent_status.rs` | phase 与 verification 的单一显示语义 |
| `views/permission_view.rs` | why/risk/scope、keyboard parity |
| `views/dashboard/state.rs` | 消费同一 truth snapshot、排序优先级 |
| `views/settings_modal.rs` | provider/config/motion settings 与来源 |
| `views/picker.rs` | 对比度、focus、empty/loading copy |
| `xai-grok-config/src/paths.rs` | LUMEN_HOME / ~/.lumen / legacy bridge |
| `xai-grok-models/default_models.json` | provider/model 文案，不含无证据“high” claim |

### 18.2 建议新增的小模块

| 新文件 | 责任 |
|---|---|
| `xai-grok-pager/src/ui_contract.rs` | ProductIdentity、TruthSnapshot、display invariants |
| `xai-grok-pager/src/views/truth_bar.rs` | fullscreen/compact truth bar renderer |
| `xai-grok-pager/src/views/readiness.rs` | migration/provider/probe/trust wizard renderer |
| `xai-grok-pager/src/views/status_detail.rs` | `/status` 与 redacted report |
| `xai-grok-config/src/migration.rs` | dry-run、diff、receipt、rollback-safe import |

限制：模块只收敛状态和渲染，不新造第二套 agent state，不跨 crate 复制模型/provider 真相。

## 19. 实现顺序，单次切换

这些是内部工程阶段，不是对用户发布的中间 UX：

### Gate A. Contract first

- `ui_contract.rs` 和 invariants。
- Product identity 单元测试。
- TruthSnapshot property tests。
- 当前 renderer 暂时适配 contract，但不发布。

### Gate B. Identity and migration

- LUMEN_HOME、legacy dry-run、receipt。
- CLI/title/auth/welcome 统一身份。
- `rg` allowlist 确认 Grok 只存在于 legal/upstream/xAI provider 场景。

### Gate C. Readiness journey

- provider/credential/model/capability/trust。
- 隔离 tool probe。
- read-only/chat-only recovery。

### Gate D. Truthful session

- Truth bar、phase groups、verification freshness、cache source。
- fullscreen/minimal/dashboard 共用 snapshot。

### Gate E. Interaction and accessibility

- welcome Enter/focus 修复。
- 所有 hit area 的 keyboard equivalence。
- 对比度、motion off、low-color snapshots。

### Gate F. Cutover

- 删除旧 product shell 和临时 flag。
- 完整 PTY matrix。
- M5 真人测试。
- 只在所有 P0/P1 验收通过后合并单一 FINAL-5UX。

## 20. P0 / P1 / P2

### P0，任何一个未完成都不能切换

- 单一 Lumen identity、真实 release channel、terminal title。
- LUMEN_HOME 与无损 legacy migration。
- provider-scoped auth，DeepSeek 默认路径不要求 Grok login。
- capability probe 与 Tool-ready 证据合同。
- TruthSnapshot、truth bar、verification freshness。
- permission why/risk/scope 和 hard-deny truth。
- welcome keyboard parity。
- M5 路径可执行，不能只有模板。

### P1，和 FINAL-5UX 同批交付

- cache source/freshness。
- phase grouping 与 raw detail progressive disclosure。
- dashboard truth/sorting。
- minimal parity。
- 80/120/180 + truecolor/256/16-color snapshots。
- reduced motion 与零 idle redraw。
- recovery 和 redacted diagnostics。

### P2，允许切换后继续优化，但规格已在 FINAL-5UX 定义

- 文案微调、更多 provider-specific diagnosis。
- context rail 的高级 plan/file view。
- 用户自定义 token，但必须继续满足 contrast/invariant。
- screen-reader 与更多终端组合实测。

P2 不得被用来推迟 P0/P1 的真相、可达性或恢复能力。

## 21. 测试矩阵

### 21.1 Snapshot

每个屏幕状态至少覆盖：

| 维度 | 值 |
|---|---|
| size | 80×24、120×40、180×50 |
| mode | fullscreen、minimal |
| color | truecolor dark、truecolor light、256、16、NO_COLOR |
| input | keyboard、mouse |
| motion | default off、reduced motion |

必测状态：migration、provider missing、probe checking/pass/chat-only/fail、trust、welcome ready、permission、editing、verify pass/fail/stale、dashboard action-required、recovery。

### 21.2 Contract tests

- `Verified` 不可在 stale source seq 出现。
- `Tool-ready` 不可缺 fingerprint/evidence。
- provider 改变后 capability 自动 Unknown。
- cache 无 source 时不可显示 hit ratio。
- blocker 必须压过 success。
- minimal/fullscreen/dashboard 同 snapshot 产生语义一致的 copy。

### 21.3 Keyboard tests

- 每个非空 hit rect 必须映射至少一个 keyboard action。
- Enter 激活当前 menu selection。
- Tab/Shift+Tab 可逆。
- Esc 恢复原 focus。
- hover 不改变 keyboard activation target。

### 21.4 Migration tests

- no legacy、valid legacy、partial legacy、conflict、read-only source、corrupt TOML、disk full、interrupted import。
- 失败后 source/target 均保持可恢复。
- receipt 不含 secrets。
- precedence 与 `/status` source 完全一致。

### 21.5 Runtime and PTY

- first paint 不等待 catalog/settings 网络完成。
- catalog prefetch 与 shell fallback 不重复请求。
- terminal title 不包含产品级 `grok` fallback。
- idle welcome 无持续 tick/redraw。
- resize 不丢 focus，不越界，不隐藏 blocker。
- resume 后 verification/capability freshness 正确。

### 21.6 Human gates

- M5：陌生用户真实完成 ≤10 分钟路径，有录像/时间/real tool call/security explanation。
- M6：15 天真实自用证据，不可用内部开发聊天记录代替。
- 两者失败时 readiness 继续 BLOCKED；UX 完成不能改写该事实。

## 22. 性能预算

| 项 | 预算 |
|---|---|
| local first input | cached p95 ≤ 500 ms |
| cold first input | p95 ≤ 1.5 s，网络异步 |
| startup duplicate request | 0；同一 catalog/settings key 只能有一个 in-flight owner |
| idle redraw | 0 FPS，无活动 TTL/timer 时不调度 |
| active spinner | ≤ 8 FPS |
| UI binary growth | FINAL-5UX 相关增量 ≤ 2 MiB，超出需解释 |
| truth update | event 到可见 p95 ≤ 100 ms |
| resize | 120→80 列单次布局，无 crash/overlap |

## 23. Definition of Done

FINAL-5UX 只有在以下全部成立时才完成：

- [ ] Primary chrome 中产品名 100% 为 Lumen。
- [ ] `Grok` 只在 allowlisted upstream/legal/xAI model/provider 场景。
- [ ] 实际版本 channel 正确，无 hard-coded Beta。
- [ ] 默认配置写入 `~/.lumen` 或 `LUMEN_HOME`。
- [ ] legacy migration 可 preview、可拒绝、可恢复、无 secret copy。
- [ ] DeepSeek first-run 不经过 xAI/Grok auth gate。
- [ ] Tool-ready 有真实 probe evidence 和 fingerprint。
- [ ] Chat-only 不能执行 edit flow，也不能冒充 agent-ready。
- [ ] Truth bar 五类事实都显示 source/freshness 或明确 unknown。
- [ ] Verified 只对 current files 生效，edit 后立即 stale。
- [ ] Permission 显示 why/risk/scope，hard-deny 不可伪 override。
- [ ] 每个 clickable action 有 keyboard equivalent。
- [ ] 80×24 无裁剪、无纯装饰 hero、主 prompt 可用。
- [ ] fullscreen/minimal/dashboard 语义一致。
- [ ] truecolor/256/16/NO_COLOR 下状态不依赖颜色。
- [ ] reduced-motion 生效，idle redraw 为 0。
- [ ] provider failure 可恢复且 session/run continuity 保留。
- [ ] `/status` 可复制脱敏 evidence report。
- [ ] startup network dedupe、性能预算和 PTY matrix 通过。
- [ ] M5 真人 gate 有真实证据。
- [ ] M6 不被伪造或弱化；未通过时继续如实 BLOCKED。

## 24. 明确不做

- 不新建 Web/Desktop GUI。
- 不做第二套 TUI。
- 不为视觉统一重写百万行 agent loop。
- 不做全 crate rename。
- 不用 provider logo 增加装饰。
- 不保留永久 old/new shell toggle。
- 不把 screenshot、mock 或自动测试当 M5/M6 真人证据。
- 不因为 FINAL-5UX 文档完成就声称产品已实现或 release-ready。

## 25. 验收命令与人工检查入口

实现阶段的最低验证集合：

```bash
rg -n 'Grok Build|Sign in to Grok|Run Grok|push_str\("grok"\)' agent/crates/codegen
cargo test -p xai-grok-pager -p xai-grok-pager-minimal -p xai-grok-config
./scripts/assert-defaults.sh
./scripts/smoke-security.sh
./scripts/probe-local.sh --list
./scripts/verify-readiness.sh
git status --short
```

`rg` 结果不能机械要求为零，必须使用 allowlist 区分 legal/upstream/xAI provider 与 primary product shell。

人工检查：

1. 空 HOME + DeepSeek key。
2. 空 HOME + 无 key。
3. 只有 `~/.grok` 的 legacy HOME。
4. local chat-only model。
5. capability 从 ready 到 provider change。
6. verify pass 后再次 edit。
7. mid-run 401/503 后恢复。
8. 80×24 keyboard-only 全流程。
9. 16-color 与 NO_COLOR。
10. M5 陌生用户录像与时间证据。

## 26. 最终产品姿态

FINAL-5UX 的胜负不在于比 Grok Build 多一个 panel、更多颜色或更大的 logo。它的优势是每个关键状态都更可信：provider/model 身份清楚，tool capability 有证据，permission 有边界，cache 有来源，verification 有 freshness，失败有恢复路径。

在 M5/M6 通过前，它是一套完整、可落地、值得实现的目标体验，不是已经被真人证明的终局。实现后仍要让证据决定能否称为极致。
