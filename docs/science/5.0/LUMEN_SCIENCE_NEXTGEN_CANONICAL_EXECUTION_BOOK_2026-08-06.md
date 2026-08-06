# Lumen Science NextGen 唯一执行总纲

## 2026-08-06 基于 Lumen 2.2 的辩证修订终稿：事实反转、gate 重映射与重新排序

**日期：** 2026-08-06（北京时间）

**性质：** 从本文件提交后起，Lumen Science 后续工作的唯一排序、依赖、停止条件、验收和交接总纲。它不是功能完成、CI 全绿、package、release、live provider、HPC、设备或科学结论证明。

**仓库范围：** 只约束 /Users/lei/code/lumen-science。/Users/lei/code/lumen 是只读底座事实源和跨仓依赖，不由本计划修改；跨仓代码迁移以 draft PR + 单 writer 纪律进行，本计划不授权直接在 lumen 仓提交。

**产品范围：** macOS first；Windows 专项延后。所有 provider、billable endpoint、真实 HPC、实验设备、deploy、tag 和 release 都要另行授权。

**排序关系：** 本书替代 2026-08-02 Science 总纲（`LUMEN_SCIENCE_NEXTGEN_CANONICAL_EXECUTION_BOOK_2026-08-02.md`）的执行顺序；08-02 书、单底座书、自治控制平面书、吸收账本继续作为规范细节和历史证据，不能删除，也不能用旧状态覆盖本书的当前事实。本书的修订理由是**上游事实已经改变**（canonical Lumen 从"dirty WIP、无 API"变成"v2.2.0 已发布、governance 全量落地、Science kernel 已入 main"），不是对 08-02 书纪律的放松。

---

# Part I — 事实冻结与辩证评审

## 0. 一页结论

08-02 书开篇的三个结构性断口，在 2026-08-06 的 2.2 现实下重新判定为**两个未动、一个反转、两个新增**：

1. **未动（仍是 P0）**：Science PR #28 的 Linux product red 仍在；Desktop Skill 生命周期 shipping bypass 仍在。S0-A / S0-B 从未执行。
2. **反转（最大变化）**：08-02 书断言"canonical Lumen 没有可消费的 stable public governance API，Science 仍复制整套 Rust Core，单底座未完成"。事实相反：**隔壁会话已把 Science kernel 吸收进 canonical Lumen main**——`xai-grok-science` crate（0.1.0，SCHEMA_VERSION=1，含 project/claim、evidence_graph、workflow、connectors、governance 等）随 v2.0.0 起就在主线上，ACP 接缝 `x.ai/science/*` 和 `x.ai/governedTree/*` 已经可消费，SessionActor 门真实存在。单底座的剩余工作从"等 API 再迁移"反转成"把 Science-only 切片推上 canonical 底座，再删复制"。
3. **反转（等待变核验）**：08-02 书 L0 是"等 canonical Lumen R0/P0-NR 收据"。2.2 现实：R0 已证明三次（v2.0.0→f51fb902、v2.1.0→3d5d52cf、v2.2.0→098f7cd4，tag 均指向 source A），P0-NR 已证明（S8 sealed receipts），CI 5/5 绿。L0 从等待变成 V0 核验卡——本会话已执行大部分核验并留下收据。
4. **新增（08-02 书没有的前置）**：PLATFORM_API_GATE 从"不存在（BLOCKED_CONTRACT）"变成"接缝存在但未正式化（IMPLEMENTING）"：缺版本化方法目录、compat manifest 和 consumer compile fixture——这三样是 Science 自己就能做的文档+fixture 工作。
5. **新增（真实且必须尊重）**：canonical Lumen 现在同时是"发布线"（v2.x，governance 全量）和"Science kernel 所有者"。Science 仓 0.1.222 复制 Core 已经落后于上游 4 个 minor 版本；继续在复制件上做新功能等于给第二底座续命，违反 08-02 书 M1 的初衷。

正式进度口径（§40）：2026-08-06 实测 **12/100**（唯一 PASS 是 L0 canonical Lumen source gate，凭 v2.2.0 tuple 收据）。这不是说成果变多了，而是上游 gate 的收据终于存在且可核验。

## 1. 2026-08-06 事实冻结

### 1.1 Canonical Lumen 2.2 当前实测

**核验时间：** 2026-08-06（本会话，git ls-remote / gh api / 本地只读检查，均带时间戳）。

| 项 | 实测 |
|---|---|
| origin/main | 79a5c2d9（auto-ledger 再生成；实质链为 7b8f385b merge ← ba93cd8e readiness ← af2857a2 evidence ← 098f7cd4 source A） |
| v2.2.0 tag（peeled） | **098f7cd4 = source A** ✓（tag 指向 A，不指向 B） |
| v2.1.0 tag（peeled） | 3d5d52cf |
| v2.0.0 tag（peeled） | f51fb902 |
| release-source-tuple.json | version=2.2.0, source_commit=098f7cd4, evidence_commit=af2857a2, tag_commit=098f7cd4, binary_sha256=f1aa4061…, source_lock_sha256=18b95181…, generated=2026-08-05T16:56:20Z |
| GitHub release v2.2.0 | 存在，19 assets，非 draft 非 prerelease |
| CI（7b8f385b check-runs） | 5/5 SUCCESS（version-check / regenerate / Lumen crates+clippy / Offline gates / Expert v2 gate） |
| readiness（artifacts/readiness/status.json） | 自动化 gate 全 PASS；blockers 仅 M5（10 分钟陌生人测试）与 M6（15 天真实日志）人工门；eval_live 20/20 PASS |
| 测试基线 | memory ~590 / shell 6330 / pager 8006 / tools 3013；clippy 全绿；32 道发布 gates；36 条 INVARIANT manifest |
| lumen 工作树 | 本会话只读核验，未修改任何 lumen 文件 |

### 1.2 2.2 里已存在的"08-02 书 C1–C8 / K1a 等待清单"实况

以下全部在 canonical lumen main（v2.0.0–v2.2.0 期间落地），本会话以源码存在性 + 测试名 + gate 名核验：

| 08-02 书等待项 | 2.2 落地点（module/test/gate） |
|---|---|
| P0 no-replay（L0.1） | S8 sealed attempt receipts：`no_replay_policy_*` counting-server 测试、`s8_sealed_retry_live` 5 测试、`P0_NR_A_FULL_AUDIT_GATE` 六条 RetryDenyReason |
| R0 source A/evidence B（L0.2） | 三次发布 tuple + `artifacts/release-source-tuple.json` + `RELEASE_TUPLE_GATE` |
| C1 public API 语义件 | `GovernedRunEnvelopeV1` 等四 DTO（identity_envelope.rs）、`DispatchPermitV1` 线性链（crate-private + PermitConsumer 注册表）、`RootBypassPermission`（INV-5 全字段）；ACP 接缝 `x.ai/science/*`（extensions/science.rs）+ `x.ai/governedTree/*`（extensions/governed_tree.rs） |
| C2 TaskTree | TaskTreeLifecycle JSONL（child handoff receipt 入 journal）、depth=3 上限、governed-tree preview、resume/orphan/late-event 处理 |
| C3-A CapabilityGrant | `CapabilityGrantV1`（TTL/revoke token/投影）、child 不再克隆 raw PermissionHandle、root bypass 只经 typed grant |
| C3-B Tool/Secret/Untrusted | 生产 dispatch 强制 `authorize_tool_dispatch` + `clamp_tool_result_text`；`SecretRef` + redaction fail-closed；claim 状态机四变体 |
| C4-A TreeBudget | `BudgetLedger` reserve/settle 12 测 + `A3_TOKEN_RESERVATION_GATE` |
| C4-B0/B1 operation journal | `SchedulerLoopOutbox` 幂等门（成功 terminal 才标记 delivered）+ kairos lease + manifest-bound resume |
| C4-C WriteScopeLease | `SessionCommand::GetWriteScopeLease` + worktree auto-handoff（真实 git delta 评估）+ child git commit/push/merge 硬拒 |
| C4-D Flow | 生产 prompt mailbox 有界（128，DroppedFull）+ `FLOW_CONTROL_GATE` |
| C5 sealed no-replay | `SealedAttemptReceiptStore`（上限 4096，fail-closed）+ attempt count=1 + per-reason 重开 |
| C6-A ClaimJournal/AcceptedSnapshot | `ClaimAuthority` + prev-hash 审计链 + 有界模型检验（625 状态穷尽）+ `CLAIM_STATE_MACHINE_GATE` |
| C6-B ContextManifest | `context_manifest_v1_*` 真实测试 + `authorize_context_rebuild`（compact/resume/reconnect 同 manifest） |
| C7 Advisor shadow | `ConsultAdvisorHost` + `lumen_advisor_consult` 真实 ToolRegistry + advice epoch 生产写侧 |
| K1a OperatorControlPlane | 五命令矩阵 + `OperatorGrantV1` + fake-clock lease cycle + outbox 幂等 |
| C8 bounded assignment | `x.ai/governedTree/assignmentRecommendation`（按条件 readiness 矩阵）+ root approval + budget + model receipt 链 |
| NG10 release foundation | `ReleaseSourceTupleV1` + release.sh 两段事务 + install provenance + rollback receipt |

### 1.3 2.2 里已存在的 Science kernel（关键反转事实）

- `agent/crates/codegen/xai-grok-science`（0.1.0，SCHEMA_VERSION=1）：`project/`（claim、evidence_graph、migration、model、query）、`workflow/`（kernel、package）、`connectors/`、`review.rs`、`import.rs`、`csv.rs`、`preview.rs`、`collaboration/`、`device/`、`dummy_lab.rs`、`remote/`、`remote_compute/`、`multimodal/`、`release/`、`governance/`、`api.rs`、`transport.rs`。crate 文档自述："This crate owns records, never execution authority. Product execution must enter through xai-grok-shell::SessionActor"——与 08-02 书 kernel 层定位一致。
- ACP 方法：`x.ai/science/run_csv`、`import_preview`、`connector_fetch`、`ssh_scp_fixture`、`goal_host_verify`（extensions/science.rs 内 match 分发，dispatch 点 acp_agent.rs:3821）；`x.ai/governedTree/status`、`x.ai/governedTree/assignmentRecommendation`。
- 与 science 仓复制件的差集：canonical 版**没有** `seqbench.rs`、`primer_thermo.rs`、`dossier.rs`、`skill_quarantine.rs`、`capability/`、`features.rs`——这六个 science-only 模块就是剩余迁移面（X-M1 的输入）。
- 08-02 书说"canonical Lumen 仓中不存在 ExtMethodContributor / SessionAuthorityPort / DomainOperation / PreparedOperation / TerminalOutcome 公共类型"——这些**命名类型**确实仍不存在（2.2 用的是 extensions 模块 + acp_agent match + crate-private DTO），但 08-02 书把它们当"proposal name"是对的；2.2 的事实是**机制已经存在，形式化缺失**，不是"机制不存在"。

### 1.4 Lumen Science 当前现场

| 项 | 实测 |
|---|---|
| Git 顶层 / 分支 | /Users/lei/code/lumen-science / `ls5-core-v0.1.251-sync` |
| HEAD | 11ec961（"docs(science): publish canonical NextGen execution book"，即 08-02 书入库） |
| 相对 origin/main | ahead 149 / behind 1（origin/main = d5a16642） |
| 工作树 | clean（本会话修改前） |
| PR #28 | OPEN、MERGEABLE；check-runs 仍含 **1 FAILURE**（"Rust xai-grok-science (test + clippy)"，2026-08-02T05:59:13Z）与 1 SKIPPED（Release build，main-only）；S0-A **未执行** |
| VERSION / SOURCE_LOCK | 0.1.222 / 2026-07-16（legacy 身份，truthful，未改号） |
| Desktop skill bypass | **仍在**：settings/ipc.ts 426–445 直连 `setSkillEnabled` / `createSkill` / `updateSkill` / `deleteSkill` → SettingsService → UserSkillRepository 可变写；S0-B **未执行** |
| upstream-lock.v2.json | status=draft（设计如此），9 个 source；I1-A 未闭合 |
| gate registry（schema-1） | 5 个粗粒度 gate，状态停留在 2026-08-01/02 观察，已过期 |
| science-only 模块（canonical 缺的 6 个） | 在 science 仓复制件中：seqbench、primer_thermo、dossier、skill_quarantine、capability、features |
| 复制 Core | agent/ 66 crates（codegen），与 canonical 2.2 差 4 个 minor 版本线 |

### 1.6 本地资产地图（辩证取用，不重做）

以下资产是此前 Codex / Claude Code 会话的累积成果，全部供后续卡片辩证取用——**先取用，不重写**；取用先对照 §1.4 与九源 intake 纪律：

**A. lumen-science 仓内（主迁移 corpus）：**

| 资产 | 位置（science 仓） | 取用卡 |
|---|---|---|
| seq_analyze 安全 oracle（Begin/Allow-only Finish/store-owned artifacts/provenance/replay/tamper 反例） | `xai-grok-science/src/seqbench.rs` + shell 触点 + built-binary 反例 | X-S1 |
| project migration/recovery（source bundle、retained root、revision/commit fence） | `xai-grok-science/src/project/migration.rs` + mutation 反例 | X-S2 |
| workflow 执行器族（executor、pinned_executable、python_runner、admission、io、kernel_admission_protocol）——canonical 只有 kernel/package 模型，执行器是 science 独有 | `xai-grok-science/src/workflow/` | X-S1 后随 seq 一并 push-up |
| review record / evidence dossier（claim 独立复核、dossier 打包） | `review.rs`、`dossier.rs` | X-S2 |
| skill ZIP quarantine（Begin/Allow/Finish + materialized=false） | `skill_quarantine.rs` + Desktop 侧闭环 | X-S3 / S0-B |
| Motif 切片（renderers + seqbench 的 motif/ORF/翻译表/引物热力学） | `primer_thermo.rs`、`packs/science/renderers/` | W3 |
| Biomni 224 descriptors + `query_uniprot` 唯一 admitted fixture | `connectors/`、third_party 审计 | W2 |
| Open Science classifier/preview + SCP 207 本地文档 | `preview.rs`、`import.rs`、docs/science | W1/W0 |
| Desktop 全套（sender identity、ACP registry、project/evidence/preview/review/notebook/skills/compute UI、full unsigned package 证据） | `packs/science-desktop/` + `docs/science/status/evidence.v1.json` | S0-B / G1 |
| 31 条 ACP 路由 / 23 SessionCommand / 26 actor methods / 10 handle methods authority map | `scripts/report-science-authority-map.py` 输出 | X-M1 每族 |
| 功能分支（Codex/Claude 会话）：`ls5-f0-foundation`、`ls5-kernel-admission-authority`、`ls5-review-record-authority`、`security/sync-core-fixes-20260727`（8 个安全修复） | 本地/远端分支 | 逐族取用前先对比 main |
| machine guards（core drift、ownership、external path denial、intake verifiers、machine gates） | `scripts/` | 全部卡 |

**B. 仓外参考资产：**

| 资产 | 内容 | 使用纪律 |
|---|---|---|
| `/Users/lei/code/lumen-open-science` | aipoch/open-science 本地 clone（blob:none） | 九源 intake 审计对象；不直接搬运 |
| `/Users/lei/code/魔改版本-claude-code` | claude-code fork + mgcc vs lumen 评测脚本；**活跃开发中**（claudecode/claudescience 方向持续改动） | 只参考方法与 bench 思路；**取用前必须重新快照**（内容随时变化）；吸收必须走 license/intake 纪律 |
| `/Users/lei/code/lumen` | canonical（只读） | 按 X-U pin 消费 |

**取用规则：** ① 先跑对应 guard/inventory 确认资产当前字节；② canonical 已含的部分（§1.3）不重复搬运；③ 受限来源只 clean-room；④ 任何取用都落到 card 的 parity corpus 里，不口头引用。

---

## 2. 辩证评审：08-02 书 × 2.2 现实

### 2.1 被证实的判断（继续有效，不改）

1. S0-A / S0-B 是真实 P0：PR #28 红、skill bypass 在，均未修。
2. 九源 intake 纪律：v2 lock 仍 draft；14 步、transitive bridge、tree coverage、disposition 词表、clean-room 规则全部继续有效。
3. 能力入场链（§21 六态）、risk class、证据包、Scientific Validity、Data/Model/Runtime Asset 合同、G1 macOS 门、§44 DoD——全部继续有效。
4. 证据等级 E0–E8、状态词、证据优先级、STOP/rollback 矩阵、卡 DoD、交接词模板——继续有效。
5. "禁止重做或丢失"的资产清单（seq_analyze oracle、project migration、workflow、review、dossier、skill quarantine、Motif、Biomni、Open Science、Desktop）——继续是迁移 corpus。
6. 10 条不可谈判规则、三层 Agent 拓扑、八道门——继续是安全脊柱；好消息是 2.2 已经在 lumen 侧把它们实现为代码。

### 2.2 被反转的判断（本书据此改排序）

1. **"canonical Lumen 无可消费 stable public governance API"（08-02 §1.2 / C1 前提）→ 反转。** 可消费面 = ACP 接缝（`x.ai/science/*`、`x.ai/governedTree/*`）+ `xai-grok-science` crate + SessionActor 门。C1 的语义清单（GovernedRunEnvelope、TerminalOutcome 语义、ArtifactSink、no caller path、actor-only transition）已实现为 2.x 的 identity/dispatch/operator 机制。
2. **"单 Rust 底座没有完成，Science 等 API 后 strangler"（08-02 §15–19 / M1 方向）→ 反转。** 底座内核已被 lumen 吸收；剩余迁移 = 6 个 science-only 模块 push-up + 方法注册 + Desktop cutover + 删 science 仓复制件。方向是"推上去"，不是"等下来"。
3. **"L0 = 等待 Lumen R0/P0 收据"（08-02 §8）→ 反转。** 收据已存在且本会话已核验（§1.1）。L0 → V0（核验并记录），不再是阻塞卡。
4. **"C1–C8 / K1a 是 lumen 侧未交付的等待卡"（08-02 §9–14）→ 反转。** 全部已交付（§1.2 表）。它们在 Science 侧的剩余工作 = 逐项核验 + 消费 fixture + 反例（V0 / S2a 承担）。
5. **"PLATFORM_API_GATE = BLOCKED_CONTRACT，等 Lumen owner"→ 重定向。** 接缝存在；缺 versioned contract、compat manifest、consumer compile fixture。这三样主要是 Science 侧（consumer）工作 + lumen 侧少量 doc/type 提交（X-C1）。

### 2.3 需要重定向的执行项

| 08-02 卡 | 08-06 处置 |
|---|---|
| L0（等 R0） | → V0：核验 2.2 收据并写 Science-side receipt（本会话已完成 R0 部分） |
| C1-RFC0 / C1-API | → X-C1：把现有 `x.ai/science/*` 接缝正式化（版本化方法目录 + compat manifest + consumer fixture + 反例）；lumen 侧只做最小 doc/type 提交 |
| S1（seq strangler pilot） | → X-S1：seqbench/primer_thermo 推入 lumen `xai-grok-science`（跨仓 draft PR），parity corpus 冻结在 science 仓，注册 `x.ai/science/seq_analyze`，Desktop cutover，删 science-copy 触点 |
| M1-A0（anti-growth） | 继续有效，立即执行（guard 已在，保持 CI 预算零新增） |
| M1-A1/A2、M1-B family 迁移 | → X-M1：顺序 seq → dossier → skill_quarantine → capability/features；每族 = parity 冻结 → push-up PR → 方法注册 → cutover → 删触点 |
| M1-C（workflow long-running） | 依赖 K1b（lumen 侧已交付 K1a）；workflow 已在 canonical kernel，science 侧只做消费 fixture |
| C2–C7 / K1a 卡 | → V0 核验清单（非实现卡） |
| S2a | 前置已满足（§1.2），从"等前置"变"建 corpus"：五类 scenario manifest vs 2.2 exact binary |

### 2.4 新出现的约束与坑（08-02 书没写，现在必须遵守）

1. **不许在复制件上新建 authority**：science 仓 0.1.222 复制 Core 已是"落后 4 个 minor 的第二底座"。任何新 SessionCommand / actor method / ACP route 只能以 canonical 侧提交或 X-C1 方法目录的方式落地；复制件上的新增一律拒绝（M1-A0 guard 已扫既有 Rust 热点，本规则扩展到 ACP 方法面）。
2. **版本身份六线制**（08-02 §34 扩展）：canonical Lumen v2.2.0、legacy Science Rust 0.1.222、Go v1.0.1、Desktop 1.1.0-dev、canonical `xai-grok-science` crate 0.1.0、Lumen 2 alpha/2.x 开发线。报告必须分栏，禁止互相冒充。
3. **PASS_UPSTREAM ≠ PASS**：lumen 交付只证明上游事实；Science 侧 gate 只有在 V0 核验收据后才从 PASS_UPSTREAM 转 PASS。任何 Science 卡不得以"lumen 已交付"为由直接宣称自己的 gate 通过。
4. **跨仓 PR 纪律**：向 lumen 提 push-up PR 必须 draft、单 writer、不自动 merge、不触碰 lumen 的 evidence 链；science 仓只提供 parity corpus 和 consumer fixture。
5. **xai-grok-science 0.1.0 无 semver 版本化**：在 X-C1 完成前，任何对 canonical kernel 的消费都必须在 composition 里固定 exact source SHA（本会话已记录 098f7cd4），不得用"最近 main"语义。

## 3. 证据等级、状态词和 source of truth

沿用 08-02 书 §2 全文（九层证据 E0–E8、六个状态词、证据优先级、必读文档表），本书记录的差异只发生在事实层（§1）和 gate 状态层（§4.1），不改变证据纪律。08-02 书 §2.3 中"当前 canonical Lumen 仓中不存在已实现的公共类型"一句按 §1.3 修正，其余照旧。

---

# Part II — Gate 重映射与新的依赖图

## 4. 正确依赖图：2026-08-06 版

~~~mermaid
flowchart TD
  S0["S0 Science P0 repair"] --> F0["F0 current baseline receipt"]
  S0 --> S0A["S0-A Linux product red"]
  S0 --> S0B["S0-B Desktop skill bypass"]
  S0A --> F0
  S0B --> F0
  F0 --> V0["V0 verify 2.2 receipts"]
  F0 --> XU["X-U base lifecycle"]
  XU --> V0
  V0 --> XC1["X-C1 seam contract"]
  V0 --> I1A["I1-A completeness"]
  V0 --> S2A["S2a shadow corpus"]
  XC1 --> XS1["X-S1 seq family"]
  XC1 --> XS2["X-S2 dossier family"]
  XC1 --> XS3["X-S3 quarantine family"]
  XS1 --> XS2
  XS2 --> XS3
  XS3 --> XS4["X-S4 capability/features"]
  XS4 --> M1["M1 de-copy + cutover"]
  M1 --> SB["SINGLE_BASE_GATE"]
  S2A --> HARNESS["HARNESS_REGRESSION_GATE"]
  S2A --> C8["C8 bounded assignment receipt"]
  I1A --> I1B["I1-B active admission"]
  I1B --> W0["W0 catalog/skills"]
  HARNESS --> W1["W1 literature connectors"]
  HARNESS --> W3["W3 seq workbench"]
  W0 --> W1
  W1 --> W3
  I1B --> W2["W2 Biomni"]
  W3 --> W4["W4 BGC"]
  W3 --> W5["W5 OpenDDE"]
  W2 --> W6["W6 workflows"]
  W4 --> W6
  W5 --> W6
  W6 --> W7["W7-A device boundary"]
  SB --> G1["G1 macOS product"]
  W7 --> G1
  V0 --> NG10["Lumen NG10 receipt"]
  NG10 --> UPT["UPDATER_TRUST receipt"]
  UPT --> G1
~~~

关键修正（相对 08-02 §4）：

- S0 后直接 F0（本会话已交付），F0 后 V0 一次性核验 2.2 全部上游收据，不再逐卡等待；
- X-C1 只依赖 V0（上游 R0/P0 已证），不再依赖"lumen owner 提交 API"；
- X-S1..4 是串行 push-up 链，X-C1 后即可开始，不等待 S2a；
- M1 de-copy 在 X-S4 后，SINGLE_BASE 依赖 M1；
- S2a 前置（C2–C7/K1a 等价物）已全部 PASS_UPSTREAM，S2a 卡从"等前置"变"建 corpus"；
- G1 仍要求 SINGLE_BASE + 至少一个 W 能力 E4 + lumen NG10/UPDATER 收据。

### 4.1 当前 gate 状态（2026-08-06 实测）

状态词（本书）：`PASS`（Science 侧独立核验收据）、`PASS_UPSTREAM`（lumen 侧交付有收据，Science 核验待 V0）、`IMPLEMENTING`、`BLOCKED_UPSTREAM`、`BLOCKED_CONTRACT`、`BLOCKED`、`FAILED`、`NOT_STARTED`、`DISABLED`。`PASS`/`PASS_UPSTREAM` 都必须携带 receipt 字段，无 receipt 的 PASS 一律视为伪造。

| Gate | Owner | 08-02 状态 | 08-06 状态 | Receipt（exact 引用） |
|---|---|---|---|---|
| SCIENCE_PR_CI_GATE | Science | FAILED | **PASS** | exact-head CI 绿 @1d3fd7d（run 31066618159：26 SUCCESS + 1 SKIPPED main-only）；S0-A 产品测试 + swap fixture + lib 619/0/8 全过（2026-08-06 观测） |
| SKILL_LIFECYCLE_AUTHORITY_GATE | Science | FAILED | **FAILED** | packs/science-desktop settings/ipc.ts 426–445 直连 mutation |
| P0_NR_SAFETY_GATE | Lumen | UNVERIFIED_COMMITTED | **PASS_UPSTREAM** | S8：no_replay_policy 3 counting-server 测、FULL_AUDIT_GATE 六 deny reason、s8_sealed_retry_live 5 测（v2.0.0 起） |
| LUMEN_R0_SOURCE_GATE | Lumen | BLOCKED_UPSTREAM | **PASS_UPSTREAM** | v2.0.0→f51fb902 / v2.1.0→3d5d52cf / v2.2.0→098f7cd4（peeled tag）；B=af2857a2；CI 5/5 @7b8f385b；release 19 assets；readiness 仅 M5/M6 blocker |
| PLATFORM_API_GATE | Lumen | BLOCKED_CONTRACT | **IMPLEMENTING** | 接缝存在（`x.ai/science/*` + `x.ai/governedTree/*` + xai-grok-science 0.1.0）；缺版本化 contract / compat manifest / consumer fixture → X-C1 |
| TASKTREE_GATE | Lumen | PARTIAL_UPSTREAM | **PASS_UPSTREAM** | TaskTreeLifecycle JSONL + depth 上限 + governedTree preview + resume/orphan + assignmentRecommendation |
| CAPABILITY_GRANT_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | CapabilityGrantV1 TTL/revoke + DispatchPermitV1 + RootBypassPermission + child 继承恒拒 |
| TOOL_CONTRACT_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | authorize_tool_dispatch + clamp_tool_result_text + TOOL_CONTRACT_DISPATCH_GATE |
| SECRET_BOUNDARY_GATE | Lumen | （书内合同） | **PASS_UPSTREAM** | SecretRef + redaction fail-closed + SECRET_REF_GATE |
| UNTRUSTED_CONTENT_GATE | Lumen | （书内合同） | **PASS_UPSTREAM** | claim 状态机四变体 + QuotedDataOnly 纪律接线 |
| ACTIVITY_UNLOAD_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM**（V0 复核） | prompt mailbox 有界 + drain + late-event 处理 |
| TREE_BUDGET_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | BudgetLedger reserve/settle 12 测 + A3_TOKEN_RESERVATION_GATE |
| OPERATION_RECOVERY_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | SchedulerLoopOutbox 幂等门 + kairos lease + manifest-bound resume |
| WRITE_SCOPE_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | GetWriteScopeLease + worktree auto-handoff + child git 硬拒 |
| FLOW_CONTROL_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | 生产 mailbox 128 上限 + DroppedFull + FLOW_CONTROL_GATE |
| LEDGER_REPLAY_GATE | Lumen | PARTIAL_UPSTREAM | **PASS_UPSTREAM** | ClaimAuthority + prev-hash 链 + 同 hash rebuild |
| CONTEXT_MANIFEST_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | context_manifest_v1_ 真实测试 + authorize_context_rebuild |
| NO_REPLAY_GATE | Lumen | BLOCKED_CONTRACT | **PASS_UPSTREAM** | SealedAttemptReceiptStore（4096 上限 fail-closed）+ attempt=1 + per-reason 重开 |
| ADVISOR_SHADOW_GATE | Lumen | PARTIAL_UPSTREAM | **PASS_UPSTREAM** | ConsultAdvisorHost + lumen_advisor_consult ToolRegistry + advice epoch |
| HARNESS_REGRESSION_GATE | Lumen/Science | NOT_STARTED | **NOT_STARTED** | Science S2a 五类 scenario corpus 未建 |
| BOUNDED_ASSIGNMENT_GATE | Lumen | NOT_STARTED | **PASS_UPSTREAM** | assignmentRecommendation + root approval + budget reservation + model receipt |
| KAIROS_LOCAL_GATE | Lumen | NOT_STARTED | **PASS_UPSTREAM** | operator_control 五命令 + OperatorGrantV1 + fake-clock lease cycle + outbox |
| NG10_RELEASE_FOUNDATION_GATE | Lumen | （书内合同） | **PASS_UPSTREAM** | ReleaseSourceTupleV1 + release.sh 两段事务 + install provenance + 三次 signed release |
| UPDATER_TRUST_GATE | Lumen | NOT_STARTED | **PASS_UPSTREAM**（V0 复核 rollback 语义） | A12 rollback receipt + install-local 落 provenance |
| SINGLE_BASE_GATE | Both | BLOCKED_CONTRACT | **IMPLEMENTING** | 6 个 science-only 模块未 push-up；science 仓复制 agent/ 66 crates 未删 |
| SOURCE_INTAKE_COMPLETENESS_GATE | Science | IMPLEMENTING | **IMPLEMENTING** | upstream-lock.v2 draft；I1-A 14 步未闭合 |
| SOURCE_INTAKE_ACTIVE_GATE | Science | BLOCKED_UPSTREAM | **BLOCKED_UPSTREAM** | 上游 R0 已 PASS_UPSTREAM；仍等 completeness + 每 source gate + active lock |
| SCIENTIFIC_VALIDITY_GATE | Science | NOT_STARTED | **NOT_STARTED** | per-capability，未开 |
| DEVICE_SAFETY_GATE | Separate | NOT_STARTED/DISABLED | **DISABLED** | W7-A 不能推进它 |
| SCIENCE_MACOS_GA_GATE | Science | BLOCKED | **BLOCKED** | 等 SINGLE_BASE + E4 + NG10/UPDATER 收据 |
| PRODUCT_PROOF_GATE（schema-1 粗粒度） | Science | BLOCKED_UPSTREAM | **BLOCKED_UPSTREAM** | 等 PLATFORM_API PASS + 重建 binary + 负例 + exact-head CI |

V0 卡把上表所有 `PASS_UPSTREAM` 逐项核验后转 `PASS`；任何一项核验失败立即降级并 STOP。

---

## 5. 新执行脊柱

### N0 — Science 自己的两条 P0（不变，第一优先）

**S0-A**（修 PR #28 Linux product red）：唯一允许方向 = 修 test fixture seam（cfg(test)/integration fixture 注入的 retained fd 或 test-only admitted runtime），保留 /usr trust-root、root-owned/nonwritable/native-ELF、no caller-path 策略；禁止放宽策略、禁止 Linux 跳过、禁止从 required list 删除、禁止 mock-only 替代。验证命令沿用 08-02 书 §5（GROK_BINARY 指向 rebuilt binary 跑 `test_stdio_science_workflow_execute_retains_store_and_interpreter_across_approval`）。

**S0-B**（fail-close Desktop skill shipping bypass）：createSkill/updateSkill/deleteSkill/setSkillEnabled 四个 shipping IPC 返回 typed AuthorityUnavailable/MigrationRequired；legacy store 变 migration-read-only；materializeAgentSkills 不被四 IPC 触发；`skillIds/forcedSkillIds` 不能让 disabled/revoked/unknown skill 重生；必须补真实 Electron 注册 negative（invoke 四通道 → fail-closed → repository bytes 不变 → runtime 目录不变 → reload callback=0 → forced id 不能 respawn）。

### F0 — 活事实与防扩张边界（本文件即 08-06 F0 交付）

本会话已交付：本书（canonical pointer 切换）、`NEXTGEN_BASELINE.json`（08-06 快照）、`NEXTGEN_GATE_REGISTRY.json`（schema-1 状态刷新 + receipts）、`NEXTGEN_GATE_REGISTRY_V2.json`（granular gates，新 schema）、`verify-nextgen-baseline.py` / `verify-nextgen-canonical-book.py` 收据化更新、v2 verifier + tamper corpus、machine-gates 接线。F0-2（authority lint：desktop ipcMain.handle mutation / forcedSkillIds / reload / 裸 store 写扫描 + 真实注册负例）仍待建卡。

### V0 — 独立核验 2.2 收据

按 08-02 书 L0.3 checklist 执行：git ls-remote 核对 A/B/main/tag（已完成：§1.1）、release/CI/readiness artifact 复核（已完成）、源码级 spot check 每个 PASS_UPSTREAM gate 的 module/test/gate 存在性（已完成 §1.2 清单）、逐项转 PASS 并写 receipt。V0 是核验卡，不是实现卡。

### X-C1 — 把现有 ACP 接缝正式化为版本化合同

1. 冻结 `x.ai/science/*`（run_csv、import_preview、connector_fetch、ssh_scp_fixture、goal_host_verify）与 `x.ai/governedTree/*`（status、assignmentRecommendation）现有 7 个方法为 v1 基线；
2. 写 `SciencePlatformApiReceiptV1`（Science 侧）与对应的 canonical 侧 `LumenPlatformApiReceiptV1`：方法名/namespace、schema hash、composition source tuple ref、compat manifest、consumer compile fixture、rollback API revision；
3. compat manifest 覆盖：read current、N-1 读兼容、unknown field 显式拒绝、deprecation 至少一条 migration fixture；
4. 反例：未知方法、跨版本调用、缺失字段、伪造 owner/path、extension 尝试 Approve/Finish、caller 提交 hash/path/terminal；
5. lumen 侧只做最小提交（如方法目录 doc 或类型导出），以 draft PR + 单 writer 纪律进行；
6. PLATFORM_API_GATE：IMPLEMENTING → PASS（凭 receipt + exact-head CI）。

### X-M1 — 反转方向的 strangler：把 Science-only 切片推上 canonical 底座

每族固定步骤（参考 08-02 书 M1-B 十二步，方向反转）：

1. 在 science 仓冻结 legacy behavior/parity corpus（request/result/artifact fixture hashes）；
2. 把纯 domain（如 seqbench、primer_thermo 的算法与 reference fixtures）以 draft PR 推入 lumen `xai-grok-science`；
3. 在 canonical 侧注册对应 `x.ai/science/*` 方法（走 SessionActor 门）；
4. science 仓 consumer：同一 parity corpus 同时跑 legacy 与 generic path，字节/语义 parity；
5. Desktop cutover（sender identity + ACP registry 指 canonical composition）；
6. exact-head CI 绿后，删 science 仓对应复制触点并降低 drift 计数；
7. 更新 authority map 与 ownership guard。

顺序：**X-S1 seq（seqbench+primer_thermo）→ X-S2 dossier → X-S3 skill_quarantine → X-S4 capability/features**。全部完成后 M1 de-copy（删除/冻结 science 仓 agent/ 复制件），SINGLE_BASE_GATE 才可 PASS。M1-A0 anti-growth 现在立即生效（不许在复制件上新增 authority）。

### X-U — 底座更新与 pin 生命周期（Lumen 会持续发版）

**前提事实：** canonical Lumen 是活跃产品线，v2.2.0 之后还会继续发版（2.3/2.4/3.x…）。Science 的底座消费策略必须是"pin 一个已核验 tuple"，而不是"追最新 main"。

**当前 pin（2026-08-06）：** `LumenCoreSourceTupleV1` = (A=098f7cd4, B=af2857a2, tag=v2.2.0→A, CI 5/5 @7b8f385b)。Science 每个消费卡默认声明此 pin；X-U 未完成前，旧 pin 继续有效，不因 lumen 发了新版本自动失效。

**触发条件（任一即开 X-U 卡）：**
1. lumen 发布新 signed tag + GitHub release（gh release 可见、assets 完整、非 draft）；
2. 用户在 Science 会话中要求"底座升级到 vX.Y.Z"；
3. 已迁移 family 的回归暴露与当前 pin 不兼容（此时 X-U 是修复卡）。

**X-U 流程（每轮一张卡，只读核验 + 记录，不夹带其他改动）：**

1. 只读核验新 tuple：`git ls-remote` peeled tag→A、evidence B、main；gh check-runs（exact commit 全绿）；release assets 清单；readiness/release-source-tuple.json；全部带时间戳写入 evidence；
2. 契约兼容检查：X-C1 完成后，比对 compat manifest（方法目录、schema hash、deprecation）——breaking 即 STOP 并登记为债务；X-C1 未完成时，记录"接缝未版本化，按 exact source SHA 消费"；
3. 更新 Science 侧 pin 记录：NEXTGEN_BASELINE.json 的 `r0_receipt`、book lock 的 `canonical_lumen_observation`、gate registry 的 PASS_UPSTREAM receipts——全部换成新 tuple，并保留旧 tuple 于 rollback 字段；
4. 回归：所有已迁 family 的 parity/actor/product corpus + Desktop E2E/package smoke 对新 tuple 重跑；
5. 升级只以 draft PR 形式存在，永不自动 merge、不触碰 protected 文件、不扩大 authority、不接受 breaking API 静默降级；
6. rollback = 回到上一个 tuple（pin 记录回退 + 回归重跑），不手工合并大 diff。

**纪律：**
- 只跟随正式 release 线（tag + release assets + exact CI），不跟随 lumen main 的每日漂移；dirty branch 永不 pin；
- 禁止"顺手升级"：S0-A/S0-B/F0/X-S 等卡片不得夹带 tuple 变更；tuple 变更只经 X-U 卡；
- 升级机器人（08-02 书 §20 语义）只观察、只开 draft PR；人工 review 后 merge；
- 新 release 不自动作废 Science 在旧 pin 上的完成证据——证据绑定 tuple，报告必须写清"基于 v2.2.0 pin"。

### S2a — 三层 shadow-only 科研黄金路径（前置已满足）

前置（C2–C7/K1a 等价物）全部 PASS_UPSTREAM（§1.2）。S2a 卡 = 建五类 versioned scenario manifest（authority / context-claim / execution-liveness / provider-advisor / UX-provenance），对 2.2 exact binary 跑：root 建 immutable contract → depth-1 Lead → depth-2 Literature/Analysis/Review → depth-3 Evidence leaf → 同 snapshot 读取 → typed Proposal + fixture artifact → root/host 独立重算 + 显式冲突解决 → Advisor shadow 无 switch → root 取消 branch → ledger/index crash 后 rebuild 同 hash → ACP/Desktop 展示真实树。Exit：HARNESS_REGRESSION_GATE=PASS。

### I1 — 九源 immutable intake（不变，前置更新）

I1-A completeness 现在即可做（不被任何上游阻塞），14 步 + SCP/transitive bridge + SourceTreeCoverage + destination receipts + 三套状态名分离；lock 保持 draft/BLOCKED_UPSTREAM。I1-B active admission 前置从"等 Lumen R0"更新为"R0 PASS_UPSTREAM + I1-A PASS + 每 source gate PASS + active lock"，仍不许提前。

### W — 能力波（顺序微调：W0/W1/W3 先行）

- W0（catalog/skills/knowledge）：只读 catalog + 六级状态 + skill 本地化 + 双层知识；mutation 一律等 X-M1 skill 迁移；
- W1（文献证据链）：Crossref → UniProtKB → Europe PMC → OpenAlex，一次一个；W1-M root-only offline evidence chain 前置 = I1-B + X-C1 + S1-B（= X-S1 后的 generic seq 路径）；
- W3（Motif/序列工作台）：seq_validate/seq_analyze/motif_scan/translate/design_review 五切片，与 X-S1 共用同一 corpus；首个 release candidate；
- W2（Biomni 224→少数）、W4（BGC）、W5（OpenDDE）在 I1-B + W3 之后；
- W6（workflow/notebook）、W7-A（Dummy/DigitalTwin only）最后。

### G1 — macOS 产品门（不变）

前置：SINGLE_BASE + 至少一个 W 能力 E4 + lumen NG10/UPDATER 收据 + Science A_S/B_S tuple + 签名/公证/安装/回滚。Go v1.0.1 保持 legacy-maintenance。

## 6. 卡表（2026-08-06 版）

| ID | 仓 | 内容 | 前置 | 类型 |
|---|---|---|---|---|
| F0-1 | Science | 本书 + baseline + registry v1/v2 + verifier 收据化 + machine-gates 接线 | 无 | 文档+工具（本会话交付） |
| S0-A | Science | 修 PR #28 Linux product red（test seam） | 当前 HEAD | 代码 |
| S0-B | Science | fail-close 四条 skill direct IPC + 真实 Electron 负例 | 当前 HEAD | 代码 |
| F0-2 | Science | Desktop authority lint（mutation/forced/reload 扫描 + 注册负例） | F0-1 | 工具 |
| V0 | Science | 2.2 全部上游收据核验，PASS_UPSTREAM→PASS | F0-1 | 核验 |
| X-U | Science | 底座更新生命周期：新 lumen release → 核验 tuple → 更新 pin 记录 → 回归 → draft PR | 触发条件（§5 X-U） | 升级卡 |
| S0-C | Science | Desktop electron 安全升级（41.7.1→41.10.x，GHSA-r4w5-6pfg-jxp5 / GHSA-9f4c-93c8-jc8g）；前置 = CI node 20→22（electron 41.10 安装器 ESM-only）；fast-uri/hono 已在本日修复 | 独立安全卡 | 升级卡 |
| I1-A | Science | 九源 completeness（14 步 + bridge + tree coverage） | 无 | 证据 |
| X-C1 | Both | ACP 接缝版本化合同 + compat manifest + consumer fixture | V0 | 合同+代码 |
| X-S1 | Both | seqbench/primer_thermo push-up + `x.ai/science/seq_analyze` + parity + cutover | X-C1 | 跨仓迁移 |
| X-S2 | Both | dossier family 同法 | X-S1 | 跨仓迁移 |
| X-S3 | Both | skill_quarantine family 同法（M1-B Skill 合同） | X-S2 | 跨仓迁移 |
| X-S4 | Both | capability/features family 同法 | X-S3 | 跨仓迁移 |
| M1 | Science | 删/冻结复制 agent/，Desktop 全量 cutover | X-S4 | 删除+接线 |
| S2a | Science | 五类 scenario corpus vs 2.2 binary | V0, X-C1 | 测试+产品 |
| I1-B | Science | active nine-source admission | I1-A, V0 | 证据 |
| W0-A/W1-D/W3-D | Science | catalog/connector/seq domain 深化（E2，不称 admitted） | I1-A | 纯 domain |
| W0-B/W1-M/W3-P | Science | managed 产品切片 | I1-B, X-C1, X-S1, S2a | 产品 |
| W2/W4/W5 | Science | 后置能力波 | I1-B, W3 | 产品 |
| W6/W7-A | Science | workflow/设备边界 | S2a, K1b 收据 | 产品 |
| G1 | Science | macOS signed release transaction | SINGLE_BASE, W E4, NG10/UPT | 发布 |

---

# Part III — 里程碑、进度与最终出口

## 40. 进度算法

正式顶层 gate 九项，每项只能 PASS 或不计分；PASS 必须凭 receipt：

| 顶层 gate | 权重 | 08-02 | 08-06 | 依据 |
|---|---:|---:|---:|---|
| S0 Science P0 clean exact-head | 10 | 0 | **0** | PR #28 红 + bypass 在（§1.4） |
| I1 nine-source admitted intake | 8 | 0 | **0** | v2 lock draft |
| L0 canonical Lumen source gate | 12 | 0 | **12** | v2.2.0 tuple 收据（§1.1），本会话独立核验 |
| C1 public governance API | 12 | 0 | **0** | IMPLEMENTING；X-C1 未完成 |
| C2–C7 governed autonomy foundation | 15 | 0 | **0** | lumen 侧全交付（PASS_UPSTREAM），Science receipt 待 V0 |
| S2 three-level Science product proof | 10 | 0 | **0** | corpus 未建 |
| K1 managed long-run/recovery | 10 | 0 | **0** | lumen 侧已交付，Science receipt 待 V0 |
| M1 single Rust base | 13 | 0 | **0** | 6 模块未 push-up，复制件在 |
| G1 macOS released product slice | 10 | 0 | **0** | 未发布 |
| **合计** | **100** | **0** | **12** | V0 后预计 +25（C2–C7 15 + K1 10）；X-C1 后 +12；每次只按 receipt 更新 |

L0 的 12 分绑定当前 pin tuple（v2.2.0，§1.1 收据）。lumen 发布新版本不自动作废本分——X-U 卡吸收新 tuple 并更新 receipt 前，旧 pin 的完成证据继续有效；报告必须写明证据绑定的 tuple。

## 41. 四个可感知产品里程碑（08-06 版）

- **Milestone A — 不再继续漏水**：S0-A、S0-B、F0-1/F0-2、V0、I1-A。用户得到：假绿被修、skill 直写被封、底座边界有机器收据。
- **Milestone B — 第一条单底座科研能力**：X-C1、X-S1。用户得到：`seq_analyze` 从 canonical 2.2 组合运行，Science 不再改复制 Core 热文件，升级只审一个 tuple。
- **Milestone C — 受控科研 Agent 产品面**：S2a corpus → HARNESS_REGRESSION；然后 W0/W1/W3 产品切片。用户得到：三层树真实展示 lineage/grant/budget/evidence/root 合流。
- **Milestone D — 可长期使用的 Science 产品**：X-M1 完成（SINGLE_BASE）+ 选择的 W 波 + G1。用户得到：macOS 安装版至少一条文献证据链 + 一条序列分析链，断电可恢复、可重放、可升级回滚，无第二 Rust Core authority。

## 42. 时间估算（不是承诺，按 gate 重估）

| 阶段 | 08-02 估 | 08-06 重估 | 变化原因 |
|---|---|---|---|
| A：S0/F0/I1 | 3–7 工程日 | **2–5 工程日** | F0-1 本会话已交付；S0-A/S0-B 边界清晰 |
| B：L0/C1/S1 | 2–5 周 | **5–10 工程日** | L0→V0 已完成大半；X-C1 是文档+fixture；X-S1 方向反转后是 push-up PR + parity |
| C：C2–C7/S2a | 4–8 周 | **1–2 周（corpus 为主）** | lumen 侧已交付；Science 侧只剩核验+corpus |
| C 后半：C8/S2b/K1 | 3–6 周 | **收据核验为主** | 已交付 |
| M1 单底座 | 4–10 周 | **2–5 周** | 剩余迁移面 = 6 模块 + cutover + 删除 |
| 首批 W1/W2/W3 产品 | 3–8 周 | **2–6 周** | 前置解锁 |
| G1 macOS release | 1–3 周 | **1–3 周** | 不变 |

## 43. 接下来十个实际动作

1. 本会话：F0-1 交付物提交并机器门全绿（进行中）。
2. 在当前 Science HEAD 完成并独立验收 S0-A，不改生产 trust policy。
3. 完成 S0-B，真实 Electron 注册后四条 mutation fail closed。
4. 提交 F0-2 Desktop authority lint。
5. 完成 V0 剩余核验（逐 PASS_UPSTREAM 转 PASS）。
6. 完成 I1-A source-to-component completeness，lock 保持 draft/BLOCKED。
7. 冻结 `x.ai/science/*` v1 方法目录，写 X-C1 contract + compat manifest + consumer fixture。
8. 推 X-S1 第一个 push-up draft PR（seqbench/primer_thermo → lumen），同时冻结 science 侧 parity corpus。
9. 以 X-S1 经验固定 family migration kit，机械迁 X-S2..S4。
10. 以 S2a 五类 corpus 对 2.2 exact binary 建 HARNESS_REGRESSION，然后按 W0/W1/W3 产品化；全部完成前不打开任何 live provider 或 24h 宣传。

## 44. 最终 Definition of Done

沿用 08-02 书 §44 全文（唯一可变 Core 与权威；Science 只依赖一个可验证 tuple + versioned governance API；Science 仓不再复制 Core 热文件；三层 Agent durable fail-closed；Advisor 不兜底；Kairos 不重复副作用；九源 exact disposition；至少一条文献 + 一条序列工作流达 E7 macOS；scientific benchmark/uncertainty/claim 边界；tamper/deny/timeout/cancel/越权/crash/stale/unknown/rollback 反证；exact-head CI/SBOM/签名/公证/安装/升级/回滚/manifest 可核对；报告分栏、未跑写 NOT RUN）。**按 08-06 事实重读 §44 时：前半（Core 单权威 + 治理全量）已在 lumen 2.2 实现，剩余的是 Science 侧消费、迁移与产品化**——完成判定不变，路径按本书 Part II。

# Appendix A — Canonical gate crosswalk（08-06 版）

~~~text
Lumen P0_NR_SAFETY_GATE (PASS_UPSTREAM)
  → V0 核验 → Science L0 consumption

Lumen R0_SOURCE_GATE (PASS_UPSTREAM, ×3 tuple)
  → V0 / X-C1 / I1-B 前置
  → Science 只依赖一个可验证的 LumenCoreSourceTupleV1 + 版本化 governance API

ScienceProductSourceTupleV1 (A_S/B_S)
  → G1：Science 自己的发布 tuple，引用 LumenCoreSourceTupleV1 与
    LumenPlatformApiReceiptV1，禁止交叉指错

TOOL_CONTRACT + SECRET_BOUNDARY + UNTRUSTED_CONTENT (PASS_UPSTREAM)
  → S2a + every runnable W capability

TREE_BUDGET + OPERATION_RECOVERY + WRITE_SCOPE + FLOW_CONTROL (PASS_UPSTREAM)
  → S2a / K1b receipt / W6

CLAIM_JOURNAL + ACCEPTED_SNAPSHOT + CONTEXT_MANIFEST (PASS_UPSTREAM)
  → S2a / K1b / scientific knowledge

NO_REPLAY_GATE (PASS_UPSTREAM)
  → S2a / C8 / network connectors / remote operations

S2a + HARNESS_REGRESSION_GATE (Science 自建)
  → C8 Applied enablement

OPERATOR_CONTROL_GATE + operation recovery (PASS_UPSTREAM)
  → K1b Science managed-run receipt

NG10_RELEASE_FOUNDATION + UPDATER_TRUST (PASS_UPSTREAM, V0 复核)
  + SINGLE_BASE_GATE (IMPLEMENTING) + selected W at E4
  → Science G1
~~~

# Appendix B — 文档权威和 supersession

本书是从 2026-08-06 起 Lumen Science 的唯一排序、依赖、状态口径和最终出口。它不删除旧书中的细节，而是按以下关系使用：

- `LUMEN_SCIENCE_NEXTGEN_CANONICAL_EXECUTION_BOOK_2026-08-02.md`：上一版 Science 总纲，保留为历史事实冻结、细节 oracle 和纪律来源；若与本书冲突，以本书为准（冲突只发生在事实层和 gate 状态层，纪律层无冲突）。
- `EXTREME_ADOPTION_SINGLE_BASE_EXECUTION_PLAN_2026-08-01.md`：单底座/M1 细节来源；方向和顺序以本书 X-M1 为准。
- `NEXT_GENERATION_AUTONOMY_CONTROL_PLANE_EXECUTION_PLAN_2026-08-01.md`：自治控制面细节来源；前置状态按本书 §4.1 更新。
- `LUMEN_SCIENCE_5_0_ULTIMATE_IMPLEMENTATION_PLAN_2026-07-28.md`：产品愿景和历史里程碑来源，不是当前状态。
- canonical Lumen 的 `LUMEN-NEXTGEN-EXECUTION-BOOK-2026-08-01.md` 与 2.x 发布链：Core 设计和交付合同来源；按 08-06 事实，canonical main 已是合法 pin（v2.2.0 tuple），不是"不可消费的 dirty WIP"。

计划修改后必须同步 `PLAN_SUPERSESSION_MAP.md` 和本目录 README；旧计划不再独立改变执行顺序。

# Appendix C — 非目标

本书不授权（沿用 08-02 书 Appendix C 全部内容，并增补）：

- 触碰 canonical Lumen 工作树或 evidence 链；
- 自动 merge、tag、push、deploy 或 live provider 调用（跨仓 push-up 一律 draft PR）；
- Windows 产品宣称；真实 HPC、设备、临床或实验室控制；
- 复制受限代码、data、model 或用"重构"规避权利；
- 用 submodule、patch stack 或提高 drift 数字永久维持第二 Core；
- 在 science 仓 0.1.222 复制件上新增 authority/route/actor method（新能力只经 X-C1 方法目录或 canonical 提交）；
- 把 lumen 2.2 的 PASS_UPSTREAM 当成 Science 自己的完成宣称；
- 用更多 Agent 替代合同、证据、review 和 root 合流；
- 用计划、测试名或 source check 冒充真实产品和 release。

这份书的目标不变：把每一步变成能开卡、能停、能证伪、能回滚、能独立验收的工作，并最终收敛到一个可升级的 Rust Lumen 底座和一个真正可用的 Lumen Science 产品。
