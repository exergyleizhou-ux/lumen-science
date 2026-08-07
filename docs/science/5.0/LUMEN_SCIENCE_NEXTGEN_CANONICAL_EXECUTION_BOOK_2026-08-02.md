# Lumen Science NextGen 唯一执行总纲

## 2026-08-02 极致源码吸收、单 Rust 底座、自治科研与 macOS 产品化终稿

**日期：** 2026-08-02（北京时间）

**性质：** 从本文件提交后起，Lumen Science 后续工作的唯一排序、依赖、停止条件、验收和交接总纲。它不是功能完成、CI 全绿、package、release、live provider、HPC、设备或科学结论证明。

**仓库范围：** 只约束 /Users/lei/code/lumen-science。/Users/lei/code/lumen 是只读底座事实源和跨仓依赖，不由本计划修改。

**产品范围：** macOS first；Windows 专项延后。所有 provider、billable endpoint、真实 HPC、实验设备、deploy、tag 和 release 都要另行授权。

**排序关系：** 本书替代 2026-08-01 Science 总纲的执行顺序；旧书、单底座书、自治控制平面书、吸收账本继续作为规范细节和历史证据，不能删除，也不能用旧状态覆盖本书的当前事实。

---

# Part I — 先把真相说清楚

## 0. 一页结论

Lumen Science 现在不是空壳，也不是已经完成的 5.0。它已经有一套很强的、actor-gated 科研产品底座，但仍卡在三个结构性断口：

1. Science PR 的 Linux 产品测试还有一个真实失败，因此当前 GitHub 不是全绿；
2. Science 仍复制整套 Rust Core，canonical Lumen 还没有可消费的稳定 public governance API，所以单 Rust 底座没有完成；
3. 九个来源已经做了大量锁定、目录化和局部产品化，但绝大多数仍是 catalog、fixture 或 quarantine，不是极致生产力。

本书的路线不是重写前面的成果，而是把已有成果变成三种资产：

- **安全 oracle：** 现有 seq_analyze、project migration、workflow、review、skill quarantine 的 Begin / Allow-only Finish / artifact / provenance / replay 语义；
- **迁移 corpus：** 现有 31 条 Science ACP 路由、23 个 SessionCommand、26 个 actor 方法和 built-binary 反例；
- **科研产品积木：** Motif、Biomni、Open Science、project/evidence/review/Desktop 已完成的算法、fixture、目录和 UI。

随后用 strangler 方式把它们迁到 canonical Lumen 的公共治理接口，逐族删掉复制 Core，再把九源能力按真实科研闭环一条条产品化。

### 0.1 当前完成度：三个口径，禁止互相冒充

| 口径 | 当前 | 含义 |
|---|---:|---|
| 现有 Science 5.0 基础工程人工盘点 | 约 70% | 非gate、无统一分母的工程印象：actor 权威、项目/证据、多个 workflow/sequence/product tests、Desktop 和离线来源基础已经很厚；它仍是 preview，不是 GA。 |
| 本书定义的 NextGen 终局人工准备度 | **33 / 100** | 非gate、不可累计的人工盘点；把单底座、三层治理、九源高生产力、macOS GA和长期运行纳入观察。 |
| 现有 machine gate registry 终态 | **0 个顶层 gate PASS** | LUMEN_R0、PLATFORM_API、TASKTREE、SOURCE_INTAKE、PRODUCT_PROOF 都仍为 blocked/implementing；这是正式 gate 真相。 |

70%/33分都不是完成宣称，也不是machine status或按代码行数估出来的；它们只能用于大白话定位，不能随卡片完成自动增加。正式进度只看后文顶层gate PASS。下面保留33分的人工构成，不能替代收据：

| 维度 | 权重 | 当前得分 | 当前事实 |
|---|---:|---:|---|
| Science actor/evidence 安全 oracle | 15 | 12 | 大量本地和产品反例存在；PR merge-candidate 仍有 1 个 Linux product test 失败。 |
| 九源权利、资产和来源覆盖 | 10 | 5 | 九源 pin/evidence 已写入 draft lock；最终逐 component 法务、数据、模型和 executable disposition 未闭合。 |
| 单 Rust 底座与升级机器人 | 20 | 2 | 有 ownership guard、draft pin 和 drift inventory；无可消费 public API、无 family cutover、复制 Core 尚在。 |
| 三层 Agent、ledger、Advisor、Kairos | 20 | 4 | Lumen 有真实局部实现；完整 grants、ContextManifest、operation recovery、Science 产品链尚未可消费。 |
| 科研能力产品化 | 15 | 4 | Motif 多片、Biomni UniProt 一片可跑；大多数候选只是 catalog/quarantine。 |
| macOS 产品 | 10 | 5 | Desktop CI、unsigned full package、headed/live-engine E2E 有证据；没有 canonical pin、签名/notarization/clean install GA。 |
| 发布、安全运营与科学有效性 | 10 | 1 | legacy Go v1.0.1 是真实 release；NextGen Rust/Desktop release、soak、live 和设备均未完成。 |
| **合计** | **100** | **33** | 每次只按 evidence receipt 更新，不能靠文档或代码量涨分。 |

### 0.2 最终用户闭环

~~~text
研究者提出问题
  → Lumen Science 建立 ResearchProject 与不可变研究合同
  → root SessionActor 创建受治理 TaskTree
  → Research / Code / Review / Evidence 子节点只在收缩后的 grant 内工作
  → 数据、文献、序列、模型和附件先 admission，再运行
  → 每个结果落为 hash artifact + evidence + provenance + uncertainty
  → child 只能提出 Proposal；host verification 与 root 才能 Accept
  → workflow 可 cancel / recover / replay，但未知副作用永不重放
  → Desktop 真实展示树、状态、证据、阻断和下一安全动作
  → 可复现 dossier / report / export
  → exact-source macOS package、升级、回滚与审计
~~~

---

## 1. 2026-08-02 当前事实冻结

### 1.1 Science 当前现场

| 项 | 当前实测 |
|---|---|
| Git 顶层 | /Users/lei/code/lumen-science |
| 分支 | ls5-core-v0.1.251-sync |
| HEAD / branch remote | 5181bdd3f3aaffef3319a0c38a8d6b1b3af1c026；与 origin/ls5-core-v0.1.251-sync 0/0 |
| origin/main | d5a16642c2bceab5cfb713d639ddbab066da63c4 |
| 相对 main | behind 1 / ahead 148；merge base d0746ce5612a53732228b627fd44e26d48f32116 |
| 开始编写前工作树 | clean |
| PR | #28，OPEN、DRAFT、BEHIND |
| PR checks | 25 SUCCESS、1 FAILURE、1 SKIPPED |
| 失败 | synthetic merge b428bf52 上的 Linux built-binary test：workflow retained interpreter fixture 位于 /tmp，被仅准入 /usr runtime 的正确策略拒绝 |
| skipped | Release build + SHA256SUMS 在 PR 上因 workflow 只允许 main 而跳过；它不是 PR release proof，也不是由本次失败直接触发 |
| PR 规模 | origin/main...HEAD 共 523 paths、178,960 insertions、5,680 deletions；大目录和 catalog 不能当作等量 runnable capability |
| Rust Core 身份 | Science copy 仍为 0.1.222；不能因正式 Lumen v0.1.251 或候选 2.0.0-alpha.1 而改号冒充 parity |
| legacy release | Go CLI/MCP v1.0.1 真实发布；它不是 Rust Core 或 Desktop release |
| Desktop | 1.1.0-dev；当前 CI 有 full unsigned macOS package 和 E2E 证据，仍非 GA |

PR 的失败不是策略回归：策略正确拒绝了临时目录中的伪 runtime。修复必须调整测试能力构造和身份交换方法，绝不能放宽 /usr trust root、允许 caller path、跳过 Linux test 或把失败移出 required product list。

### 1.2 Canonical Lumen 当前观察

**只读快照时间：** 2026-08-02 13:00:27 +08:00。以下是可过期观察，不是pin；F0必须使HEAD/check变化后旧快照自动变`STALE`。

| 项 | 当前实测 / 解释 |
|---|---|
| Git 顶层 | /Users/lei/code/lumen |
| 分支 / committed HEAD | sync/absorb-upstream-20260731 @ 9ae4762aeaeb74a57c7428dd4912304de441ce70 |
| remote branch | 与 origin/sync/absorb-upstream-20260731 0/0 |
| origin/main | 2f47a9ad84e94b20291a1ad3d6b005ccbd3885f4 |
| 相对 main | behind 0 / ahead 169 |
| 当前工作树 | 1 个由隔壁Lumen会话拥有的untracked review文档：`docs/R0-UPSTREAM-A422-REVIEW-2026-08-02.md`；Science不碰 |
| 最近提交 | `2e778682`提交no-provider-receipt fail-closed；`9ae4762`刷新P0 source lock。它们是UNVERIFIED_COMMITTED，不是P0/R0 PASS |
| 活跃进程 | `cargo test -p xai-grok-shell --lib advisor_shadow_ -- --nocapture`仍在运行；未收集结果 |
| PR | #134 OPEN、UNSTABLE，head=9ae4762；13:00后API实测3 checks：1 SUCCESS、2 IN_PROGRESS；不能报绿 |
| committed candidate version | 2.0.0-alpha.1；只是开发身份，不是 release |
| 正式 main/release line | origin/main 仍为 0.1.251；正式 v0.1.251 release 存在 |
| 最新 Lumen 总纲 | 已提交于 4d81bbf，共 2,051 行，SHA-256 为 9ce40685e3740265c69ec44b7a1c65f149f242f9ea526db0415623c02efc2b69 |
| 用户附件旧稿 | 570 行，SHA-256 为 3e378f5376c82f01cc3f84d4918b8f2d6a1c275092ef59ba2be499541722313f；只作历史设计输入 |
| pin eligibility | **false**：dirty WIP、P0/R0/CI 未闭合、无 public versioned Science extension contract |

Science 只能消费将来被 canonical Lumen 合法提交、review、CI 和 public contract gate 证明的 source tuple。Lumen 本地代码“已经写了”、source lock已刷新或定向测试“正在跑”都不等于 Science 依赖可用。刷新命令至少包括`git ls-remote`、PR head、exact-head check-runs、worktree status和相关进程；结果必须带时间戳。

### 1.3 Lumen 当前已有的好设计，Science 如何吸收

最新版 Lumen 总纲的以下设计被本书采用为跨仓合同，而不是在 Science 复制实现：

1. **GovernedRunEnvelope：** 所有 identity、assignment、accepted snapshot、grant、budget、model receipt、lease、operation class、evidence sink 和 deadline 来自一个治理信封；
2. **ToolContract + SecretBoundary + UntrustedContent：** 工具 schema/scope/idempotency、bounded preview/full artifact、retention，以及所有外部文本只作为 quoted data；
3. **P0 no-replay：** 没有 sealed receipt 先禁全部同轮重投，再逐 reason 重新开放；
4. **TaskTree + CapabilityGrant + TreeBudget：** 真实 lineage、单调收缩、原子 reservation；
5. **ClaimJournal → ClaimAuthority → AcceptedSnapshot → ContextManifest：** child 防跑偏靠状态机与证据，不靠 Advisor；
6. **WriteScopeLease：** worktree 不是写权限，child 不能 commit/push/merge；
7. **DeliveryObservation：** queue/channel 未知必须 Frozen，不能丢事件后报成功；
8. **OperatorControlPlane：** Inspect/Freeze/Cancel/ApproveResume/TakeOver 都是 typed actor command；
9. **source A / evidence B：** binary/tag source 与 evidence suffix 分工明确，验证关系而非强迫所有 SHA 相等；
10. **shadow-only golden path 先于 bounded assignment：** 先证明树在没有自动模型分配时安全，再给 Advisor 有限消费权。

### 1.4 现有 Science 成果，禁止重做或丢失

下列成果是迁移 oracle，不因换底座删除：

- seq_analyze 的 durable Begin / permission / Allow-only Finish、store-owned hash artifacts、provenance、evidence、terminal/replay、deny/timeout/cancel/tamper/boundary tests；
- project_migrate 与 project mutation 的 source bundle、retained root、revision/commit fence、recovery和反越权语义；
- workflow execution 的 approval、pinned interpreter、sandbox、partial artifact rollback、deterministic reuse 和 built-binary tests；
- kernel admission、review record、evidence dossier、skill ZIP quarantine；
- Motif 的 FASTA、metrics、IUPAC、translation tables、ORF、restriction/digest、primer thermodynamics；
- Biomni 224 tool descriptors、273 resource records和唯一历史admitted mapping `query_uniprot`；它只达到offline fixture产品切片，live仍`pending-deny`，且不代表当前exact-head CI；
- Open Science archive classifier/preview、SCP 207 本地 discovery documents；
- Desktop sender identity、ACP registry、project/evidence/preview/review/notebook/skills/compute UI 与 full unsigned package；
- 31 条 Science ACP route、23 个 SessionCommand variant、26 个 SessionActor methods、10 个 SessionHandle methods 的 authority map；
- Core ownership guard、external Cargo path denial、draft platform pin、drift inventory、machine honesty gates。

这些资产迁移时必须做 byte/semantic parity corpus；删除旧路径只能发生在新公共接缝的 actor、product、CI proof 通过以后。

### 1.5 两条当前 P0，不能被“ZIP 已 actor-gated”掩盖

第一条是上面的 Linux product red。

第二条是 Desktop Skill 生命周期仍有 shipping bypass：

~~~text
main/ipc.ts
  → registerSettingsIpcHandlers
  → settings/ipc.ts raw ipcMain.handle
  → SettingsService create/update/delete/setEnabled
  → UserSkillRepository mutable write
  → onSkillsChanged / materializeAgentSkills / runtime reload
~~~

精确读链为：

- packs/science-desktop/src/main/ipc.ts:88-105；
- packs/science-desktop/src/main/settings/ipc.ts:423-447；
- packs/science-desktop/src/main/settings/service.ts:1050-1085、2706-2727；
- packs/science-desktop/src/main/skills/user-skill-repository.ts:310-320、947-1005。

现有 source-analysis test 明确不运行真实 Electron，且没有展开上述模块注册，所以 Desktop authority CI 成功不能证明 skill mutation actor-only。ZIP quarantine闭环只证明一种输入路径，不能替代 create/update/delete/enable/activation/reload 的权威收口。

---

## 2. 证据等级、状态词和 source of truth

### 2.1 九层证据

| 层 | 必须保存什么 | 不能替代什么 |
|---|---|---|
| E0 Plan | 文档、依赖、owner、stop/rollback | 任何实现 |
| E1 Source | exact diff、source pin、license/provenance、format/check | test/product |
| E2 Unit/contract | 正例、反例、property/fuzz、真实 counts | actor durability |
| E3 Actor | Begin/approval/Finish、identity、cancel/recovery、artifact/replay | exact binary |
| E4 Product | 新构建 exact-source binary 走 ACP/Desktop seam | GitHub CI |
| E5 CI | exact GitHub source或明确 synthetic merge SHA、workflow/job URL、conclusion | package/live/release |
| E6 Package | clean build artifact、binary/SBOM/signature/attestation、install smoke | GitHub release/live |
| E7 Release | tag/assets/checksums、clean-user install、upgrade/rollback、release receipt | live provider/device |
| E8 Live | 单独授权的endpoint/host/HPC/device和operator receipt | 不能反向覆盖任何较低层失败 |

任何报告固定写 PASSED、FAILED、BLOCKED、NOT RUN、SKIPPED 和 NO TESTS MATCHED；不使用“基本绿”“应该可以”“目录已接入”等模糊词。

### 2.2 证据优先级

1. 当前 worktree、Git commit、git ls-remote/gh 原始结果和原始测试 exit；
2. 当前源码中的 typed contract、状态机和测试；
3. 本书的执行合同；
4. 旧 Science 计划和历史 provenance；
5. 外部项目 README、宣传、二手说明。

低层材料与高层冲突时直接作废。旧 CI 不能证明新 HEAD，local binary 不能证明 CI/package，synthetic merge failure 不能被 head-only local pass 隐藏。

### 2.3 必读文档和允许参考的代码锚点

实施者在改代码前必须完整读相应列，不允许根据本书摘要自行发明 API：

| 工作 | 必读输入 | 可复用模式 |
|---|---|---|
| 全局排序 | 本书；PLAN_SUPERSESSION_MAP；NEXTGEN_GATE_REGISTRY | gate/status/evidence 纪律 |
| 单底座 | EXTREME_ADOPTION_SINGLE_BASE_EXECUTION_PLAN；science-core-ownership.v1.json；draft platform pin | ownership、strangler、pin guard |
| 自治控制 | NEXT_GENERATION_AUTONOMY_CONTROL_PLANE_EXECUTION_PLAN；最新 Lumen 2 总纲 | authority、lineage、memory、Advisor、Kairos |
| source intake | upstream-lock.v2、schema、forbidden-paths、intake inventories、provenance | exact pin、nested license、exact-one disposition |
| Skill migration | SKILL_AUTHORITY_MIGRATION_CONTRACT | admission/activation/revocation 分离 |
| seq pilot | seqbench.rs；extensions/science.rs；commands.rs；run_loop.rs；acp_session_impl/science.rs；handle.rs | 现有安全 oracle |
| project/workflow | project/mutation.rs、project/migration.rs、workflow/executor.rs、pinned_executable.rs、built-binary tests | recovery、store capability、commit fence |
| Desktop | files/science-ipc.ts、settings/service.ts、settings/ipc.ts、acp-session-manager.ts | sender identity、read model和已知 legacy bypass |

当前 canonical Lumen 仓中不存在已实现的 ExtMethodContributor、SessionMutationContributor、SessionAuthorityPort、DomainOperation、PreparedOperation 或 TerminalOutcome 公共类型。它们都是 proposal name，不是可 import API。C1 只有在 Lumen owner 提交 versioned public contract 后才可从 Draft 变 PASS。

---

# Part II — 唯一架构与依赖图

## 3. 目标架构：一个 Core，三个 Science 层

~~~mermaid
flowchart TD
  H["Human / macOS Desktop / ACP"] --> L["Canonical Rust Lumen Core"]
  L --> G["Public Governance API<br/>SessionActor-owned"]
  G --> T["TaskTree / Grants / Budget / Operation / Ledger"]
  G --> X["Science Extension<br/>domain codecs + adapters"]
  X --> K["Science Kernel<br/>project / evidence / workflow / review"]
  K --> P["Science Product<br/>Desktop / resources / package"]
  X --> A["Controlled capability adapters"]
  A --> R["Store-owned artifacts<br/>evidence / provenance / replay"]
~~~

目标仓库/发布边界：

| 层 | 所有者 | 内容 | 禁止 |
|---|---|---|---|
| canonical Lumen Core | /Users/lei/code/lumen | SessionActor、permission、TaskTree、tool/process/provider、artifact authority、ledger、operation、release | Science 私有 domain 和第二套科学 store |
| lumen-science-kernel | Science | ResearchProject、EvidenceGraph、workflow schema、scientific claims、pure algorithms、reference fixtures | SessionActor、裸 permission、process/network authority |
| lumen-science-extension | Science | public API codec、capability descriptors、adapter plans/results、Desktop read DTO | import Lumen private modules、裸 store/path/handle |
| lumen-science-product | Science | Desktop、resources、package、diagnostics、operator UX | 判断 approval/success、直接写权威 store |

最终 Science 仓不再持有可变的复制 Rust Core。它只保留自己的 crates、public adapter fixtures 和 exact canonical Lumen source/API pin。

### 3.1 十条不可谈判规则

1. SessionActor 是 execution、permission、artifact、evidence、provenance、replay、cancel/recovery、terminal 的唯一权威；
2. Desktop、extension、child、Advisor、Kairos、connector、kernel、renderer 和外部 runtime 都是 adapter；
3. child 能力只能收缩，不能继承 root yolo/bypass/raw PermissionHandle；
4. child 输出只能是 typed Proposal/evidence，不能直接 Accepted、project success或长期记忆；
5. Advisor 不能解决 child 幻觉，不能接受 claim，不能切换正在输出的 stream；
6. unknown MCP、unknown schema、unknown owner、unknown output/effect、queue delivery unknown 全部 fail closed；
7. 任何外部文本、网页、PDF、terminal/MCP output 和仓库内容都只是 QuotedDataOnly，不能成为 grant/approval/dispatch 控制输入；
8. denied/timed out/cancelled/tampered/cross-boundary operation不发布新的成果artifact或activation；审批前已存在的输入、审计事件、forensic patch/evidence只能保留在不可激活quarantine并标终态，不能伪装成result；
9. source catalog、UI 可见、cargo check、旧 CI、unsigned package 都不能冒充 runnable/release；
10. 受限许可证路径不复制、不派生、不换名重写；只从开放标准、自有需求和独立 fixture 做 clean-room 同类能力。

### 3.2 Science 的研究事实层

Core 的 WorkingLedger 只提供通用 claim/accepted snapshot 治理；Science 在其上投影 ResearchClaim，不复制另一套 acceptance authority：

~~~text
Proposed
  → EvidenceAttached
  → HostVerified
  → Accepted | Rejected | Conflicted | Inconclusive
  → Superseded | Revoked | Frozen
~~~

每个 Science claim 至少绑定：

- task tree / node / project / owner / session / workspace；
- immutable assignment 与 ContextManifest hash；
- source artifact、derivation、environment/model/data asset receipts；
- evidence/provenance refs 和 uncertainty；
- reviewer/host verification；
- policy/schema revision、supersedes/resolution relation；
- retention、redaction和撤销原因。

模型输出、搜索摘要、child summary、Advisor report 默认最多 Proposed。只有 actor-owned transition validator 能产生 Accepted；Accepted 也不自动等于 workflow/project 成功。

### 3.3 三层科研 Agent 的实际产品拓扑

~~~text
Root Research Agent          depth 0，提出accept/permission/terminal instruction
└── Workstream Lead          depth 1，可在显式 WriteScopeLease 内准备候选
    ├── Literature/Data      depth 2，默认只读，输出 evidence proposal
    ├── Analysis/Code        depth 2，受限 runner，不能 commit/push/merge
    └── Review/Reproduction  depth 2，独立验证
        └── Evidence leaf    depth 3，只读、不可 spawn、不可 MCP
~~~

深度 3 只是硬上限，不是默认开放。任何一级没有 TaskContract、grant、budget、deadline、accepted snapshot、tool contracts 和 expected output schema 都不得 spawn。

Human/root instruction也只是typed proposal；只有SessionActor校验identity、grant、state和receipt后才能执行accept/permission/terminal transition。root Agent不能直接改store、ledger或终态。

### 3.4 子 Agent 防跑偏的八道门

1. root 写 immutable assignment；
2. actor 从真实 lineage 构造 node，不信 caller parent/depth；
3. CapabilityGrant 按 root ∩ parent ∩ role ∩ operation 收缩；
4. TreeBudget 原子 reserve；
5. AcceptedSnapshot + ContextManifest 构造受控输入；
6. ToolContract 和 QuotedDataOnly 阻断工具输出注入；
7. child 只写 Proposed/EvidenceAttached，host/root 复验；
8. root 合流、冲突显式、verification debt 不消失。

Advisor 是其后的独立意见，不在这八道门中。

---

## 4. 正确依赖图：不再形成 gate 环

~~~mermaid
flowchart TD
  S0["S0 Science P0 repair"] --> F0["F0 current baseline receipt"]
  LP0["Lumen P0_NR_SAFETY_GATE"] --> LR0["LUMEN_R0_SOURCE_GATE"]
  LR0 --> C1["C1 public Governance API"]
  C1 --> PAPI["PLATFORM_API_GATE"]
  LR0 --> C2["C2 TaskTree"]
  C2 --> C3["C3 Grants + ToolContract"]
  LR0 --> C40["C4-0 activity/unload safety"]
  C2 --> C4A["C4-A atomic TreeBudget"]
  C3 --> C4A
  C4A --> C4B0["C4-B0 pre-dispatch operation journal"]
  C40 --> C4B0
  C2 --> C4D["C4-D flow + delivery observation"]
  C4B0 --> C4D
  LR0 --> C5["C5 sealed no-replay receipts"]
  C2 --> C6A["C6-A Claim/AcceptedSnapshot"]
  C6A --> C6B["C6-B ContextManifest"]
  C3 --> C6B
  C4A --> C6B
  C6B --> C4B1["C4-B1 manifest-bound operation recovery"]
  C4B0 --> C4B1
  C4D --> C4B1
  C3 --> C4C["C4-C WriteScopeLease"]
  C4B0 --> C4C
  C5 --> C7["C7 Advisor shadow"]
  C6B --> C7
  PAPI --> S1C["S1-A seq adapter compile/parity"]
  S1C --> S1P["S1-B root governed product pilot"]
  C3 --> S1P
  C4A --> S1P
  C6B --> S1P
  S1P --> S2A["S2a shadow-only Science golden path"]
  C3 --> S2A
  C4B1 --> S2A
  C4C --> S2A
  C4D --> S2A
  C5 --> S2A
  C6B --> S2A
  C7 --> S2A
  S2A --> C8["C8 bounded assignment"]
  C8 --> S2B["S2b assignment extension"]
  C4B1 --> K1A["K1a Core Kairos local"]
  C4D --> K1A
  C5 --> K1A
  C6B --> K1A
  K1A --> K1B["K1b Science managed run"]
  PAPI --> K1B
  S1P --> K1B
  F0 --> M1A0["M1-A0 immediate copied-Core anti-growth"]
  LR0 --> M1A1["M1-A1 source ownership freeze"]
  M1A0 --> M1A1
  M1A1 --> M1A2["M1-A2 active Core/API consumer pin"]
  PAPI --> M1A2
  PAPI --> M1B["M1-B short family de-copy"]
  M1A2 --> M1B
  S1P --> M1B
  K1B --> M1C["M1-C workflow de-copy"]
  M1B --> M1C
  M1C --> SB["SINGLE_BASE_GATE"]
  F0 --> I1A["I1-A evidence + component completeness"]
  I1A --> WD["W self-owned domain contracts + fixtures"]
  LR0 --> I1B["I1-B active nine-source admission"]
  I1A --> I1B
  WD --> W["W runnable capability admission"]
  I1B --> W
  PAPI --> W
  SB --> G1["G1 macOS product/release"]
  W --> G1
  LR0 --> NG10["Lumen NG10 release foundation"]
  S2B --> NG10
  K1A --> NG10
  NG10 --> UPT["Lumen UPDATER_TRUST_GATE"]
  UPT --> G1
~~~

关键修正：

- P0_NR_SAFETY_GATE 是 Lumen R0 的 runtime 前置；
- C1 只依赖 R0，并负责产生 PLATFORM_API_GATE，不能反过来要求 API gate；
- TaskTree、provider truth、source ownership可在 R0 后与 C1 并行；
- S2a 必须包含 Tool/Secret/Untrusted、operation recovery、write scope、flow、ledger replay、ContextManifest；
- bounded assignment 必须在 shadow-only exact-binary golden path 后；
- M1-A0 anti-growth不等R0；M1-A1 active tuple等R0；M1-B等public API；只有workflow/long-running的M1-C等K1b；
- 九源domain contract、rights和fixture可提前，任何runnable admission都必须等public API和对应治理gate；
- G1还必须等canonical Lumen的NG10 release foundation，不能只看Science package。

### 4.1 当前 gate 状态

| Gate | Owner | 当前状态 | 变 PASS 的最低条件 |
|---|---|---|---|
| SCIENCE_PR_CI_GATE | Science | FAILED | Linux retained-interpreter产品反例修复，PR synthetic merge required jobs 绿 |
| SKILL_LIFECYCLE_AUTHORITY_GATE | Science | FAILED | shipping create/update/delete/enable fail-closed，真实 IPC负例无写入/无 reload |
| SOURCE_INTAKE_COMPLETENESS_GATE | Science | IMPLEMENTING | I1-A full-tree exact-one coverage、transitive closure、nested rights/assets、destination receipts/negative tests；仍可BLOCKED_UPSTREAM |
| SOURCE_INTAKE_ACTIVE_GATE | Science | BLOCKED_UPSTREAM | completeness PASS + Lumen R0 + 每source gate pass + active lock；只有它可驱动direct-adapt/runnable admission |
| P0_NR_SAFETY_GATE | Lumen | UNVERIFIED_COMMITTED / CI IN_PROGRESS | no same-turn resubmit、transport attempt count=1、targeted counts/check/exact-head CI receipt |
| LUMEN_R0_SOURCE_GATE | Lumen | BLOCKED_UPSTREAM | clean source A、exact CI、evidence B、main review和 rollback |
| PLATFORM_API_GATE | Lumen | BLOCKED_CONTRACT | versioned public contract、compat manifest、adapter fixture、exact CI |
| TASKTREE_GATE | Lumen | PARTIAL_UPSTREAM | durable read model、resume/orphan/late-event与 product projection |
| CAPABILITY_GRANT_GATE | Lumen | BLOCKED_CONTRACT | 消灭 raw PermissionHandle inheritance，TTL/revoke/scope property proof |
| TOOL_CONTRACT_GATE | Lumen | BLOCKED_CONTRACT | every tool mapping、unknown MCP deny、result/redaction/catalog hash |
| ACTIVITY_UNLOAD_GATE | Lumen | BLOCKED_CONTRACT | mailbox activity、unload race、late event、lease/restart seam |
| TREE_BUDGET_GATE | Lumen | BLOCKED_CONTRACT | atomic reserve/settle/replay、usage unknown truth |
| OPERATION_RECOVERY_GATE | Lumen | BLOCKED_CONTRACT | lease/event/outbox/reconcile/Frozen fault matrix |
| WRITE_SCOPE_GATE | Lumen | BLOCKED_CONTRACT | canonical scope/overlap/stale base/root handoff |
| FLOW_CONTROL_GATE | Lumen | BLOCKED_CONTRACT | bounded queue、delivery observation、late event/no false success |
| LEDGER_REPLAY_GATE | Lumen | PARTIAL_UPSTREAM | claim journal/authority/snapshot/rebuild same hash |
| CONTEXT_MANIFEST_GATE | Lumen | BLOCKED_CONTRACT | spawn/compact/resume same manifest，tamper/legacy fail closed |
| NO_REPLAY_GATE | Lumen | BLOCKED_CONTRACT | sealed attempt/effect/delivery receipt 和每 reason 独立 reopening |
| ADVISOR_SHADOW_GATE | Lumen | PARTIAL_UPSTREAM | versioned advice、privacy/budget/independence receipt、zero execution impact |
| HARNESS_REGRESSION_GATE | Lumen | NOT_STARTED | versioned scenario/mutation corpus、exact binary、known debt count |
| BOUNDED_ASSIGNMENT_GATE | Lumen | NOT_STARTED | S2a 后 root-approved new child/turn assignment receipt |
| KAIROS_LOCAL_GATE | Lumen | NOT_STARTED | operation port、operator control、crash/freeze/reconcile exact-binary |
| UPDATER_TRUST_GATE | Lumen | NOT_STARTED | signed metadata、rollback/freeze、wrong-key/rollback/mirror/partial-update negatives |
| SINGLE_BASE_GATE | Both | BLOCKED_CONTRACT | all copied Core families cut over and deleted，one exact pin |
| SCIENTIFIC_VALIDITY_GATE | Science/per-capability | NOT_STARTED | registered benchmark spec + independent receipt；不是全局一次性PASS |
| DEVICE_SAFETY_GATE | Separate safety program | NOT_STARTED/DISABLED | HIL/Real operator/physical/e-stop/regulatory plan；W7-A不能推进它 |
| SCIENCE_MACOS_GA_GATE | Science | BLOCKED | exact canonical composition、package/sign/notarize/install/rollback |

当前schema-1 machine registry中的粗粒度`SOURCE_INTAKE_GATE`在F0迁移前按`SOURCE_INTAKE_ACTIVE_GATE`解释，绝不能由I1-A completeness提前置PASS。

---

# Part III — 从今天开始的实施程序

## 5. Phase S0 — 先修 Science 自己的两条 P0

### S0-A — 修 PR #28 Linux product red

**目标：** 不等待上游，在不放宽安全策略的前提下修复 PR #28 Linux product test，并重建当前事实。

#### 根因

test_stdio_science_workflow_execute_retains_store_and_interpreter_across_approval 为了在审批窗口替换可执行文件，把 /usr/bin/python3 copy 到临时 workdir。新的 Linux pinned-executable admission 正确规定 shipping interpreter 必须来自被接纳的 /usr runtime，于是测试在真正的 swap 行为前就被拒绝。

#### 唯一允许的修复方向

1. 保留 production /usr trust-root、root-owned/nonwritable/native-ELF和 no caller-path 策略；
2. 把 test 分成两个职责：
   - product test 使用 canonical /usr interpreter，证明 store capability 在 approval 后不被 path/symlink swap；
   - pinned-executable 模块的低层 fixture 用受控 test-only admitted runtime abstraction 或 retained file descriptor，证明 bytes/path swap 不执行替换内容；
3. test-only seam 必须 cfg(test) 或 integration fixture 注入，shipping code不接受临时 root；
4. 继续断言恶意 marker 不出现、outside store 为空、retained store收到 ledger/output；
5. product required-test inventory 仍必须精确匹配 1，不允许 ignore/skip/rename逃逸。

#### 禁止的假修复

- 放宽 production 为任意 executable path；
- 允许 /tmp、workdir、PATH discovery 或 symlink runtime；
- Linux 条件跳过该 test；
- 只改变 expected state 为 failed；
- 从 CI required test list 删除；
- 用 mock-only unit test替代 built binary；
- 合并/rebase main 来掩盖失败，而不解释策略冲突。

#### 实施触点

允许优先检查：

- agent/crates/codegen/xai-grok-shell/tests/test_built_binary_e2e.rs 的该测试；
- xai-grok-science/src/workflow/pinned_executable.rs 的 existing test seam；
- .github/workflows/science-ci.yml required product test inventory。

除非低层反例证明必须，不改 production admission。若确需 production 变更，必须单独 authority review，不与 test fixture commit 混合。

#### 验证

~~~zsh
cd /Users/lei/code/lumen-science/agent
cargo fmt --check -- crates/codegen/xai-grok-shell/tests/test_built_binary_e2e.rs crates/codegen/xai-grok-science/src/workflow/pinned_executable.rs
cargo test -p xai-grok-science --lib --locked
cargo check -p xai-grok-shell --locked
cargo build -p xai-grok-pager-bin --bin lumen --locked
shasum -a 256 target/debug/lumen

# Linux 产品证明应由 GitHub PR merge candidate 或隔离 Linux runner执行：
GROK_BINARY="$PWD/target/debug/lumen" \
cargo test -p xai-grok-shell --test test_built_binary_e2e --locked \
  test_stdio_science_workflow_execute_retains_store_and_interpreter_across_approval \
  -- --ignored --exact --nocapture

cd /Users/lei/code/lumen-science
python3 scripts/report-science-authority-map.py
scripts/science-machine-gates.sh
git diff --check
~~~

每条保存原始 exit 和 passed/failed/ignored/filtered。macOS 因 cfg(target_os=linux) 不匹配时必须写 NOT RUN，不能算绿。

#### Exit / rollback

- Source：无 trust-root放宽、无新 direct write；
- Product：Linux临时 executable必须拒绝，受保护 executable retention和store symlink attack分别有明确测试；
- CI：PR required job不再因该 test失败；
- Release job仍按 main-only规则单独验证，不能把 PR skip变 PASS；
- Rollback：回到 S0 前 commit，保留失败 evidence；不得回滚安全策略。

### S0-B — 立即冻结 Desktop Skill shipping bypass

**目标：** public Platform API出现以前，先停止四个直接修改 mutable store和触发runtime reload的shipping IPC；保留只读 catalog、preview和actor-gated ZIP quarantine。

第一提交只做 fail-closed：

1. createSkill、updateSkill、deleteSkill、setSkillEnabled 的production IPC返回 typed AuthorityUnavailable / MigrationRequired；
2. renderer明确显示“等待受治理 Skill Revision API”，不能假成功；
3. legacy store变 migration-read-only，不删除用户已有内容；
4. materializeAgentSkills不能因上述四个 IPC触发；
5. GitHub URL和agent-home import继续fail-closed；
6. ZIP quarantine仍 materialized=false、enabled=false，不自动激活。
7. task API传入的`skillIds/forcedSkillIds`不能让disabled、revoked、unknown或未获actor-approved `ActiveRevision`的skill重生、nudge或materialize；现有forced activation链同样先fail-close。

必须补真实 registration product negative，而不是只扫 source：

~~~text
register app IPC
  → enumerate / invoke four shipping channels
  → each returns fail-closed
  → repository bytes unchanged
  → runtime directory unchanged
  → reload callback count = 0
  → forced disabled/revoked/unknown skill id cannot respawn or nudge
  → read-only list/preview and ZIP quarantine remain available
~~~

之后的 actor-owned create/revision/activation/revocation只能等 PLATFORM_API_GATE，通过 M1-B Skill family迁移实现。

**Stop：** 如果关闭会导致无法读取/导出既有用户 skill，先补只读 recovery/export，不允许以数据风险为理由继续保留写 authority。

---

## 6. Phase F0 — 建立活的 Science 事实与防扩张边界

**目标：** 让计划、状态、CI 和 source ownership 不再靠聊天同步。

交付：

1. 本书成为 canonical ordering pointer；
2. 新 execution snapshot 记录 Science HEAD、PR merge SHA/checks、Lumen observation、附件/当前 Lumen book hash；
3. gate registry增加第 4.1 节的 granular gates、owner、dependencies、status和 last evidence；
4. verifier检查九源 exact ids、计划 hash、gate DAG无环、上游 blocked gates不能被文档手改 PASS；
5. 现有Rust source-only ownership guard继续禁止增加新的Science-specific SessionCommand/actor method/private Core import，但明确它只扫既有Rust热点，不宣称完整authority coverage；
6. 新建独立Desktop authority guard，扫描/验证`ipcMain.handle` mutation、`UserSkillRepository`写入、`forcedSkillIds`、runtime materialization、裸store/path写和reload；并以真实注册负例证明，不只grep；
7. docs/VERSIONING、PRODUCT_STATUS、current.json明确 0.1.222、v1.0.1、Desktop 1.1.0-dev、formal Lumen v0.1.251、Lumen 2 alpha candidate五条不同身份；
8. current product status与external exact-head CI receipt分离，历史binary不能冒充current。

反例：

- 改一行 status为PASS但无evidence；
- plan hash漂移；
- dependency cycle；
- missing gate/source id；
- current HEAD不是snapshot ancestor；
- Lumen dirty observation被改pin_eligible；
- synthetic merge failure被head-only local pass覆盖；
- docs把catalog count写成runnable count。

F0只交付truth tooling，不改runtime authority。

在F0 granular registry真正落地前，现有`NEXTGEN_GATE_REGISTRY.json`仍是五个粗粒度历史gate，不能被引用为本书全部gate的machine proof。本书提交只使排序和合同生效，不使任何新gate PASS。

---

## 7. Phase I1 — 九源 immutable intake 真正闭合

**当前状态：** third_party/upstream-lock.v2.json已包含九个source的exact commit、archive hash、root/nested license evidence和初步component disposition，但status=draft，verifier故意返回BLOCKED。它不是admission完成。

### I1.1 当前九源 pin

| Source | 当前 lock pin | 当前产品真相 |
|---|---|---|
| snap-stanford/Biomni | 400c1f366b96a35ca253e13c9b06c5076af41d65 | catalog很深；仅query_uniprot历史mapping有offline fixture产品切片；live pending-deny，不代表当前E5 |
| jvogan/motif | 876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0 | 多个deterministic seqbench slice已actor-gated |
| aipoch/open-science | fd2853f0b9bdb6c063ccc1e741687584ab94bf9a | preview/classifier已适配；connector/service仍未产品化 |
| qzzqzzb/OpenClaudeScience | 4a5f2ab2879ebd4f806155c796e247da94bb1625 | catalog/UX可吸收；agent runtime拒绝；四个受限skills clean-room only |
| HUST BGC-Prophet | de5068695a381d117ae3829d9c2698d954b85efc | code候选quarantine；weights/data restricted |
| Aureka OpenDDE | f607bb3c9ff299c0627ac20f5ef8e25d716ed46f | inference候选quarantine；weights/MSA restricted |
| ai4s-research/open-science | f3928bda37acfdbe3dfe18792fc1eca38c2e884f | read-only UX候选；OpenCode runtime拒绝 |
| exergyleizhou-ux/lumen | lock中历史main 2f47a9ad… | catalog-only；真正source pin等待P0/R0/API |
| exergyleizhou-ux/lumen-science | lock中历史41ec8cd5… | own source inventory；当前HEAD另有observation |

### I1-A — evidence和component completeness（现在可做）

I1-A只把证据、边界和待办补全。只要canonical Lumen记录仍为`blocked-upstream-r0`，整个v2 lock就必须保持`draft/BLOCKED_UPSTREAM`；I1-A不能把`SOURCE_INTAKE_ACTIVE_GATE`改成PASS。

#### 每个 source 必须完成的 14 步

1. 锁 full commit和可复核retrieval record；
2. 锁archive/tree digest；
3. 哈希root LICENSE/NOTICE；
4. 递归找nested licenses、generated/vendor/data/model/skill terms；
5. 枚举build-time/runtime fetch、endpoint、credential、binary、container；
6. 每个component exact-one `SourceComponentDispositionV2`；词表严格限定为`vendor/adapt/clean-room/catalog-only/quarantine/reject-authority/reject-license/reject-data-model`；
7. 对vendor/adapt记录版权、NOTICE、source path/hash和变化；
8. 对clean-room建立开放标准、自有requirement、隔离实现者和独立fixtures；
9. 对data/model记录publisher、license、terms、hash、size、用途和科学限制；
10. 对service记录ToS、privacy、egress、quota、cache/retention和live authorization；
11. 写threat model：path/archive/parser/deserialization/shell/MCP/SSRF/secret；
12. 写一个最小capability card，不能直接导入runtime；
13. validator + tamper/omission/nested-license negative；
14. 最终reviewer只在上述v2词表中裁决disposition和reuse mode；不得把`admitted`写进source lock。

### I1-A.1 SCP / InternScience transitive bridge

当前207个SCP documents只有旧`ecosystem-admission.lock.json`的`transitive_sources`收据，不属于九源v2 exact-source validator。它们只能维持historical catalog/quarantine，不能借I1或W0自动提升。

I1-A必须二选一并有negative test：

1. 给v2 schema增加显式`transitive_sources`，逐项绑定parent source、exact commit/tree/hash、rights和component disposition；或
2. 建独立`transitive-source-registry.v1`，由v2 root record以hash引用并验证闭包。

缺bridge、parent错、commit/hash漂移、嵌套license缺失、transitive executable默认allow都必须失败。

### I1-A.2 SourceTreeCoverageV1 与 destination receipt

现有v2 verifier只证明声明的component至少命中某个tree path，不能证明整个pinned tree无遗漏、无重叠。因此I1-A必须新增：

- 每个tree entry记录path/blob hash/kind，恰好命中一个disjoint component或`default=quarantine`规则；
- forbidden rule优先；adapt/vendor/clean-room/reject是显式例外；
- overlap、uncovered、extra file、changed blob、inventory/tree digest不一致全部失败；
- 输出per-source/per-disposition counts和完整tree digest；
- omission/overlap/extra-file/changed-blob negative corpus。

每个真正进入Science目标树的adapt/vendor文件还要有`AdaptedSourceReceipt`：source id/component id、upstream path/blob hash、destination path/hash、transform、license/NOTICE。lock非active、非允许adapt path、source blob错、destination无receipt或未标vendor/adapt文件都必须被CI拒绝。

### I1-A.3 三套互不混用的状态名

- `SourceComponentDispositionV2`：只描述来源component如何处理，使用上述lock词表；
- `CapabilityAdmissionState`：`Cataloged/Quarantined/FixtureOnly/Sandboxed/Managed/Released`；
- `ProductEvidenceLevel`：本书E0–E8，只描述证据层。

一个`adapt` component仍可能只有`Cataloged + E1`；一个`Managed` capability也不能反向改写其source rights。代码、JSON schema、UI和报告都不得用同一`status`字段混装三者。

### I1-A.4 极致拿来的八层漏斗

| Intake层 | 交付 | 仍不能声称 |
|---|---|---|
| IL0 Observe | immutable pin/tree manifest | rights/admission |
| IL1 Rights | license/data/model/service receipts | runnable |
| IL2 Pure knowledge | spec/schema/fixture/reference vectors | actor/product |
| IL3 Governed adapter | descriptor/quarantine/actor contract | exact binary |
| IL4 Product | rebuilt binary + negative path | CI/package |
| IL5 CI | exact source/merge check | release/live |
| IL6 Package | macOS install/upgrade/rollback | release/endpoint/device |
| IL7 Live | separately authorized endpoint/device | 不能替代任何前层失败 |

### I1-A.5 复用和 clean-room 规则

| rights | 允许 | 不允许 |
|---|---|---|
| Apache/MIT且nested clear | 保留notice/source hash的vendor/adapt、独立优化、reference differential | 导入其agent/permission/runtime成为authority |
| root permissive、nested restricted | 只复用clear paths；受限path clean-room或reject | root license覆盖nested restriction |
| data/model/service terms不清 | catalog/quarantine | 下载、运行、发布、把输出当科学事实 |
| 明确禁止copy/derive/distribute | 从开放标准和自有需求独立实现同类功能 | 对着受限源码逐行改写、换名、保留fixture/模板派生 |

### I1-A Exit

- 九源与已知transitive sources的full-tree coverage、component completeness、rights/asset待办和exact-one disposition可机器验证；
- tamper/omission/nested-license/transitive-bridge suite通过；
- lock仍诚实`draft/BLOCKED_UPSTREAM`；
- `SOURCE_INTAKE_COMPLETENESS_GATE=PASS`只说明盘点闭合，不允许direct adapt；
- 只允许自有domain contract和fixture工作并行，不称upstream capability admitted。

### I1-B — active source admission（必须等Lumen R0）

**前置：** I1-A + LUMEN_R0_SOURCE_GATE。

将Lumen source record更新为已核验的source A/evidence B关系和rollback；public API仍由独立PLATFORM_API_GATE决定。重跑全九源及transitive closure。只有validator规定的每个source都`rights_status=verified && source_gate_status=pass`，lock才可从draft变active。

### I1-B Exit

- 九源和所有必要transitive sources exact-one disposition；
- verifier在real lock PASS，tamper suite全过；
- NOTICE/SBOM/source manifest可生成；
- 没有unreviewed executable、model、data或service被标admitted；
- SOURCE_INTAKE_ACTIVE_GATE=PASS只代表来源/权利覆盖，不代表capability runnable。

---

## 8. Phase L0 — 等待并验收 canonical Lumen，而不是跟着脏工作树跑

### L0.1 P0_NR_SAFETY_GATE

Science 不实现该 gate，只验收 receipt。最低必须包含：

- compact/auth/reroute 三个 shell同轮resubmit consumer关闭；
- sampler retry/backoff/image-strip/HTTP1/doom resample的effective max retries=0；
- ordinary pool preselection只在root初次sampler admission；
- 401/402/413/503/context overflow/doom、root/child/pin/empty pool反例；
- real SamplerActor + counting server证明attempt count=1；
- exact source SHA、diff hash、raw counts、cargo check、diff check、rollback。

未提交 dirty diff和本地running test不算PASS。

### L0.2 R0 source A / evidence B

正确R0 receipt不是“所有SHA必须相等”，而是可验证`LumenCoreSourceTupleV1`：

~~~text
source_commit_a
evidence_commit_b
evidence_suffix_base_a
binary_source_commit_a
exact_ci_source_or_merge_commit
canonical_main_ref
tag_commit_a (only if an authorized tag exists)
rollback_source_a / rollback_evidence_b
~~~

硬规则：

- A 是clean build/tag source；
- B 是A的allowlisted evidence-only direct suffix；
- B不夹runtime/version/Cargo.lock/policy source；
- binary stamp、source lock和CI关系逐项验证；
- tag如存在指向A，不指向B；
- current branch merge/main review和exact CI独立保存；
- release、install、notarization仍是后置门。

### L0.3 Science admission checklist

收到 Lumen handoff 后，Science 独立执行：

1. git ls-remote核对A/B/main/branch；
2. 验签或校验commit/tag/manifest；
3. 读R0 path ownership manifest；
4. 若handoff同时声称public API，复跑compatibility fixture；否则明确`PLATFORM_API_GATE=BLOCKED_CONTRACT`，不阻止R0本身验收；
5. 对比release lock、Cargo.lock/toolchain；
6. 确认P0 no-replay、安全ledger、grants等protected gate状态；
7. 若API未完成，R0可PASS但PLATFORM_API仍BLOCKED；
8. 任何dirty path、CI failure、evidence mismatch或旧receipt立即STOP。

### L0 Exit

LUMEN_R0_SOURCE_GATE=PASS只表示有可审计canonical source tuple。它不表示public Science API、TaskTree、Advisor、Kairos、package或release已经完成。

---

## 9. Phase C1 — canonical Lumen public Governance API

**Owner：** Lumen Core单writer。Science提供consumer contract、compatibility fixtures和反例，不在复制Core中先实现。

### C1-RFC0 — 先冻结组合拓扑和调用方向

首版只允许first-party static composition：Science extension crate在构建时进入composition binary的版本化registry；不开放动态插件、任意本机路径、临时下载或peer agent fallback。

RFC必须冻结：

- Core source tuple、API receipt、Science source、extension manifest、composition lock、binary和Desktop resource的hash关系；
- method namespace/version、contributor identity、duplicate/collision/unknown registration规则；
- startup时manifest/API/catalog不匹配即diagnostic/read-only；
- 调用方向：ACP/Desktop只向Core提交untrusted request；human/root只能提出typed approval/cancel instruction；SessionActor验证并执行transition；
- Core在durable Allow后调用adapter；adapter只回`bytes + bounded non-authoritative metadata`，不得回caller hash、path、approval或terminal；
- Core自算hash、写artifact/evidence/provenance并决定terminal；
- `DomainProjectionCommitV1`把Science domain event/projection delta与authoritative operation transition置于同一事务，或用durable outbox/reconcile证明等价exactly-once；extension不能直写ScienceStore形成第二权威。

必测：恶意/重复contributor、namespace collision、未注册extension、私有Core import、extension尝试Approve/Finish、伪造hash/path/terminal、domain commit在每个crash point、late/duplicate output、composition mismatch和fallback。

### C1.1 目标

让每一种 Science operation都走一个versioned generic host，而不是每种能力继续增加：

~~~text
Science ACP route
  → extension descriptor + domain codec
  → public Governance API
  → SessionActor Prepare
  → durable AwaitingApproval
  → Allow-only Running
  → controlled adapter returns hash outputs
  → actor Finish / artifact / evidence / provenance / terminal
~~~

### C1.2 最小公共合同

最终命名由RFC决定，但语义必须覆盖：

- SessionAuthorityPortV1：对extension只暴露Submit/RequestCancel/InspectRecovery/RebuildReadModel；Approve/Finish/terminal setter是Core内部能力，C1不暴露retry/resubmit；
- OperationDescriptorV1：kind/schema/risk/required capabilities/idempotency/result artifact classes；
- GovernedRunEnvelopeV1：唯一identity/context/grant/budget/model/lease/evidence信封；
- PreparedOperationRef：opaque、actor-issued、expiry-bound，不能由extension构造；
- TerminalOutcome：Succeeded/Denied/TimedOut/Cancelled/Failed/Blocked(reason)/Frozen/RecoveryRequired；Succeeded必须绑定verification、evidence和artifact receipt，但绝不隐含scientific claim已被root Accept；
- ArtifactSink：只提交bytes/hash/metadata，不暴露store root/path；
- AdapterExecutionPortV1：只有Core在Allow后调用；adapter回bytes和bounded metadata，Core忽略caller hash并自行计算；
- DomainProjectionCommitV1：domain event与operation transition原子提交或durable outbox/reconcile；
- Evidence/Provenance refs：owner/project/session/workspace/call/operation绑定；
- ExtMethodContributor：只注册descriptor/codec，不拿actor/store；
- compatibility manifest：API semver、schema revisions、capability set、deprecations、source tuple。

### C1.3 GovernedRunEnvelope 对 Science 的约束

Science 的DomainOperationEnvelope只能是Core信封的domain projection，不新增第二套identity。必须回链：

- run/task tree/node/root/immediate parent/lineage；
- immutable assignment/accepted snapshot/ContextManifest hashes；
- capability grant/policy revision/budget reservation；
- model selection receipt；
- operation class/idempotency/lease；
- evidence sink、deadline、owner/project/session/workspace/call；
- domain request hash和schema revision。

caller传入owner/path/depth/grant/model/terminal都只是untrusted request，不是权威字段。

### C1.4 永不暴露

- &mut SessionActor或private actor command enum；
- raw PermissionHandle/yolo/bypass；
- ScienceStore、MemoryStorage、scheduler actor；
- caller-chosen store root/output path；
- raw process/network/client/credential；
- direct Accepted/terminal setters；
- arbitrary MCP/run closure；
- workspace自由文件写。

### C1.5 compatibility 与升级

C1另产出`LumenPlatformApiReceiptV1`，字段至少包含：Core source tuple ref、API source commit、API evidence suffix（如采用A_API/B_API）、semver、schema/contract hashes、compat fixture commit/hash、provider/consumer CI receipts和rollback API revision。它是R0之后的独立收据，不能被塞进R0制造依赖环。

1. API semver + schema hash；
2. public adapter compile fixture驻留在Lumen和Science；
3. N/N-1读兼容，写只用当前schema；
4. unknown/new field保留或明确reject，不能silently drop authority fields；
5. removal/deprecation至少一条migration fixture和rollback；
6. Science composition分别引用`LumenCoreSourceTupleV1`和`LumenPlatformApiReceiptV1`，并验证compat manifest/CI关系；
7. bot只开draft PR，不自动merge；
8. breaking contract或protected negative失败自动STOP。

真正会再次dispatch provider/tool/external effect的`Retry/Resume/Replay`不是C1 baseline方法。它们只有在C4-B1完整operation recovery与C5-B sealed receipt gate通过后，才能作为独立版本化命令逐reason开放。`RebuildReadModel`只读store-owned events/artifacts，绝不发起新外部作用。

### C1.6 必测反例

- forged owner/project/session/workspace/call/node；
- prepared ref reuse、expiry、foreign process、duplicate finish；
- Finish before Allow；
- deny/timeout/cancel仍产artifact；
- adapter提交caller path而非bytes；
- artifact hash/manifest swap；
- unknown operation/schema/MCP；
- extension直写store/ledger/terminal；
- extension调用Approve/Finish、提交caller hash/path/terminal、domain projection与terminal撕裂；
- actor restart、caller drop、late result、duplicate result；
- 已emitted token/thought/tool/effect/unknown delivery仍尝试retry，transport/provider attempt count必须保持1；
- public API pin来自A、CI却来自另一个source；
- deprecated field被静默丢失。

### C1 Exit

PLATFORM_API_GATE=PASS需要public crate/protocol、API docs、compat manifest、consumer compile fixture、actor negatives、exact binary sample、exact GitHub CI、source A/evidence B关系和rollback。

---

## 10. Phase S1 — 用 seq_analyze 做 generic strangler pilot

### S1-A — compile/parity seam

**前置：** PLATFORM_API_GATE=PASS。只编译descriptor/codec/compat fixture、冻结legacy parity oracle；不创建durable run、不dispatch、不产出产品完成声明。

### S1-B — root-only governed product

**前置：** S1-A + C3-A CapabilityGrant + C4-A TreeBudget + C6-B ContextManifest。它不等待完整三层Agent、Advisor或Kairos。

S1-B必须使用immutable root assignment、deterministic genesis AcceptedSnapshot、root ContextManifest、grant和budget reservation。root-only profile不能spawn、不能workspace write、不能降级到legacy summary；从fast/compile profile升级失败必须`Blocked`，绝不能用`None`填authority字段后继续。

### S1.1 为什么选它

seq_analyze拥有最完整的现有安全语义和反例：

- durable create_run/event/pending approval；
- Running + Allow才可Finish；
- store-owned analysis.json/report.md；
- owner/project/session/workspace/call boundary；
- parse failure Failed；
- deny/timeout/cancel无artifact；
- replay、tamper、caller-drop、single-flight、cross-process serialization；
- built-binary ACP product corpus。

因此它是兼容 oracle，不是待重写业务。

### S1.2 迁移步骤

1. S1-A冻结现有request/result/artifact/provenance fixture hashes；
2. S1-A将FASTA/seqbench pure domain迁入science-kernel，不依赖shell/actor/store；
3. S1-A在science-extension实现OperationDescriptor和codec；
4. 通过public port提交immutable bytes、options和domain context；
5. actor发prepared ref和permission；adapter只在Allow后计算；
6. adapter回传bytes/hash/metadata，不拿store root；
7. actor提交artifacts/evidence/provenance/terminal；
8. ACP route只查registry，不hard-code新SessionCommand；
9. Desktop继续使用同method或versioned alias；
10. 对同一parity corpus同时跑legacy和generic path；
11. generic E4/E5通过后，删seq专用command/handle/actor/route触点；
12. 更新authority map和copied-Core drift/ownership。

### S1.3 反证矩阵

| 类别 | 必须否证 |
|---|---|
| identity | foreign owner/project/session/workspace/call/tree/node |
| approval | finish-before-Allow、stale/expired/foreign approval、deny/timeout/cancel |
| bytes | post-approval source swap、wrong request hash、artifact hash/manifest tamper |
| lifecycle | caller drop、actor restart、duplicate Begin/Finish、late result、single-flight race |
| storage | caller path、symlink escape、partial publish、non-store artifact、rollback residue |
| replay | same operation/different bytes、same bytes/different identity、tampered CAS |
| extension | direct fs write、private actor import、raw store/permission/process handle |
| governed admission | missing/genesis mismatch AcceptedSnapshot、missing/tampered ContextManifest、budget/grant absent、fast-path downgrade、root-only profile spawn/write |

### S1.4 验证层

- E1：pure kernel + codec check/format；
- E2：domain/reference/property tests；
- E3：generic host actor negative suite；
- E4：newly rebuilt Lumen + Science extension从ACP真正运行；
- E5：exact pin/merge CI；
- Desktop：sender-bound request和artifact preview；
- rollback：关闭generic consumer回到legacy oracle，不删除新artifacts。

### S1 Exit

只有legacy与generic parity、全部negative、built binary和CI通过，才删除seq专用Core触点。S1通过不代表其他30条route已迁移。

---

## 11. Phase C2–C7 — Science 消费 Core 治理能力的完整前置

这些能力由canonical Lumen实现。Science负责科研projection、compatibility和product corpus，不复制实现。

所有durable schema统一执行migration matrix：read current；只对明确N-1做迁移；write current；unknown/new authority field、partial/torn record、rollback schema mismatch和legacy half-state全部Frozen/Blocked。每个C2–C8 card都要有read-old/write-new/unknown/partial/torn/rollback negatives，不能各自静默丢字段。

### C2 — TaskTreeLineage

**现状：** Lumen已有真实root/immediate parent/depth/path、depth=3 hard max和树cancel局部实现；默认深度仍1，durable read model/recovery未完成。

Science要求：

- root→workstream→research/review/test→evidence leaf真实lineage；
- caller forged parent/depth/path拒绝；
- resume/reconnect/orphan/late-event不改lineage；
- ancestor cancel递归；
- Desktop/ACP只读projection与coordinator一致；
- legacy single-layer session保持兼容但不自动re-admit。

Exit：TASKTREE_GATE=PASS；仍不开write/MCP/automatic assignment。

### C3-A — CapabilityGrant

**当前最大危险：** Lumen child仍克隆parent PermissionHandle。现有tool list收缩不是独立、可撤销permission grant。

必须完成：

- root-issued grant绑定tree/node/owner/project/workspace/resource/tool/schema/policy；
- TTL、nonce、approval ref、revoke/expire；
- effective capability = root policy ∩ parent ceiling ∩ role ∩ operation；
- depth2/3默认read-only；depth3 no spawn/no MCP/no background；
- unknown ToolKind/MCP/endpoint/schema deny；
- ancestor cancel/revoke使旧manifest不可dispatch；
- child永不持有raw PermissionHandle/yolo/bypass。
- root bypass也必须是typed `RootBypassGrant`，有resource/tool scope、TTL、nonce、revoke和audit receipt；它不能被派生、序列化进child manifest或变成永久默认；
- depth-2 Analysis/Code只能经固定ToolContract读取host-staged input、在sandbox计算并向ArtifactSink返回bytes；禁止workspace任意写、任意shell、隐式network和caller path。

反例：root Auto/AcceptEdits/yolo/bypass传child、root bypass过期/撤销后仍用、sibling scope、expired grant、stale policy、unknown MCP、child commit/push、shell/workspace/symlink/path escape。

### C3-B — Tool/Secret/Untrusted Content

每个Science runnable adapter必须有ToolContract：

- canonical identity + input schema hash；
- operation class和idempotency class；
- required capability/resource/endpoint；
- result policy：redacted bounded preview + full hash artifact；
- context byte limit；
- credential只用SecretRef；
- classification、retention deadline和deletion authority。

所有PDF/HTML/MCP/terminal/repo/issue/web/data文字标记UntrustedToolOrRemoteData + QuotedDataOnly。它们不能改变grant、approval、assignment、claim transition或dispatch。

组合退出门：

- TOOL_CONTRACT_GATE；
- SECRET_BOUNDARY_GATE；
- UNTRUSTED_CONTENT_GATE。

三门是任何W波次runnable和S2a的前置。

### C4-0 — Activity / unload safety

在operation journal和Kairos之前先封actor mailbox unload race：活动请求/stream/tool/approval/outbox必须进入同一activity accounting；有active lease、pending durable event或unobserved late event时不得卸载。unload/reload、cancel/late result、receiver close、double-decrement、stale activity snapshot和crash边界必须有deterministic seam。`ACTIVITY_UNLOAD_GATE`不PASS时，C4-B0和K1均不得启动。

### C4-A — TreeBudget

必须原子reserve/release：

- depth/fanout/live/background nodes；
- token/tool/wall time；
- trusted cost usage；
- artifact bytes；
- per-tree process count。

missing provider usage显示unavailable，不按0；success/fail/cancel/timeout/late event恰好settle一次。

### C4-B0 — pre-dispatch GovernedOperation journal

先建立不会产生外部作用的operation identity、owner tree/node/session、lease epoch、budget reservation、append-only lifecycle event、state+outbox原子提交和`Prepared/Blocked/Cancelled`恢复。B0没有ContextManifest时绝不允许进入Running或dispatch；它只解决“先durable记录，再决定是否可运行”。

### C4-B1 — manifest-bound running / recovery

**前置：** C4-B0 + C6-B ContextManifest + C4-D Flow control。

统一child、terminal、monitor、scheduler、workflow的：

- operation id、owner tree/node/session；
- lease id/epoch/holder/heartbeat；
- budget和已验证ContextManifest；
- append-only lifecycle event；
- state transition + outbox原子；
- reconcile/freeze/recovery reason；
- idempotency receipt。

外部副作用、任何已发model block、unknown delivery/effect都Frozen，绝不自动replay。

`OPERATION_RECOVERY_GATE`只有B1的crash/outbox/reconcile/manifest/flow负例全过才PASS；B0 source或unit通过不能提前放行S2a/Kairos。

### C4-C — WriteScopeLease

worktree不是权限。任何writer必须持root-signed path scope：

- canonical path/glob、base commit、worktree id、target ref、TTL；
- overlap detector；
- symlink/../absolute/empty glob拒绝；
- child只能交patch/evidence artifact；
- root检查dirty target/stale base/conflict/verification；
- child永不commit/push/merge；
- cancel/expiry保留patch/evidence，不能清理掉成果。

S2a首版可以完全不签write lease，但gate本身仍须证明系统能拒绝写。

### C4-D — Flow control

**前置：** C4-B0 operation identity。每个delivery observation必须绑定operation/sequence；无operation的匿名queue状态不能驱动authority transition。

authority event不可静默drop。必须记录：

- Enqueued/Coalesced/Dropped/ReceiverClosed/Unknown；
- sequence、queue pressure和owner operation；
- UI chunk可明确coalesce；
- grant/cancel/tool/terminal/evidence不可drop；
- channel closed/sequence gap令operation RecoveryRequired/Frozen；
- bounded queue、deadline、fair share和shutdown drain。

### C5 — Provider health 与 no-replay

分两步：

1. P0-NR-A先关闭所有无receipt同轮重投；
2. R0后实现sealed ProviderAttemptReceipt，每次只重开一种reason。

唯一允许fallback条件：

~~~text
sealed
+ NoAssistantTextDelta
+ NoObservableReasoningDelta
+ NoToolSignal
+ OutboundNotAttempted
+ NoExternalEffect
+ allowlisted candidate
+ no user pin
+ valid budget/privacy/compatibility
~~~

receipt只依据transport/provider协议可观察事件、dispatch记录和attempt counter，不捕获、不保存、也不判断模型隐藏CoT。任一Unknown、assistant/reasoning/token delta、tool signal、backend tool、dispatch、delivery uncertain、effect possible都不重放。

### C6-A — ClaimJournal / AcceptedSnapshot

顺序不能颠倒：

1. append-only claim journal，schema/sequence/hash link；
2. actor-owned root transition validator；
3. deterministic AcceptedSnapshot；
4. index/read model可删可重建，同input同hash；
5. foreign/torn/middle corruption/unknown schema/legacy partial migration Frozen；
6. LongTerm promotion只有root显式、evidence-backed、idempotent。

Science EvidenceGraph引用Core accepted claim refs，不另开“模型写真相”旁路。

### C6-B — ContextManifest

每个spawn/compact/resume/reconnect都从同一受控输入重建：

- immutable assignment和user objective refs；
- real lineage；
- AcceptedSnapshot revision/end hash；
- tool catalog和allowed contracts hashes；
- grant/policy/budget/deadline；
- permitted artifact refs；
- model selection ref；
- producer/schema version。

禁止raw root chat、sibling Proposed/scratch、secret、裸路径、yolo/PermissionHandle或可执行自然语言。hash mismatch/unknown schema/legacy自动进入Blocked。

### C7 — Advisor shadow

Advisor只输出可审计advice：

- user pin/pool/BYOK/privacy/tool compatibility；
- provider health/context/budget；
- task class和failure-domain independence；
- candidate/rejection/reason/uncertainty；
- zero switch/spawn/tool/write/Accept影响。

子agent幻觉仍由C2/C3/C6和root acceptance控制。ADVISOR_SHADOW_GATE需要offline replay corpus policy violation=0、pin override=0、stream switch=0、audit missing=0。

---

## 12. Phase S2a — 三层 shadow-only 科研黄金路径

**前置：** S1-B + TaskTree + Grants + Tool/Secret/Untrusted + TreeBudget + OperationRecovery + WriteScope + Flow + Claim/AcceptedSnapshot/ContextManifest + NoReplay + AdvisorShadow。

**刻意不等待 bounded assignment。** 所有模型分配用offline fixture或root pin；Advisor只能Shadow。

### S2a.1 第一条黄金路径

~~~text
root创建immutable research contract和fixture project
  → root批准depth-1 Workstream Lead
  → Lead请求depth-2 Literature / Analysis / Review
  → Review请求一个depth-3 Evidence leaf
  → 每个child读取同一revision的AcceptedSnapshot
  → branches只提交typed Proposal + fixture artifact
  → root/host独立重算、显式解决一个冲突
  → Advisor写Shadow advice但不能switch/Accept
  → root取消一个branch，siblings继续
  → ledger/index crash后rebuild同hash
  → exact binary通过ACP/Desktop展示真实tree、debt和terminal
~~~

Science fixture应组合：

- 一个本地ResearchProject；
- 一个小FASTA/Motif sequence review；
- 一个本地文献/数据fixture；
- 一个矛盾claim；
- 一个tampered artifact；
- 一个cancelled branch；
- 无network、无provider、无arbitrary shell。

### S2a.2 Versioned scenario corpus

不是写一个演示脚本，而是五类scenario manifest：

1. **Authority：** forged root/parent/depth、child bypass、unknown MCP、expired grant、operator UI direct state write；
2. **Context/claim：** raw chat/sibling Proposed/secret注入、manifest/snapshot/tool catalog tamper、missing evidence、conflict；
3. **Execution/liveness：** budget/lease race、closed channel、late terminal、cancel/freeze/takeover、outbox crash；
4. **Provider/advisor：** user pin、pool exhausted、thought/tool/unknown receipt、advice non-authority；
5. **UX/provenance：** actor truth vs UI、redaction/retention、Blocked/Frozen/debt可见、no false PASS。

每条记录input hash、expected typed transitions、allowed/forbidden effects、negative mutation、binary hash、raw exit和retention deadline。

### S2a.3 Verification debt

任何Blocked、Frozen、failed/NOT RUN gate、unverified patch、unresolved conflict、unconsumed advice都是durable debt。下一次loop成功不能删除旧debt；只能用明确resolution receipt关闭。

### S2a Exit

- HARNESS_REGRESSION_GATE=PASS；
- rebuilt exact-source binary真跨ACP/Desktop；
- depth/lineage/grant/budget/manifest/claim/recovery/UI一致；
- no provider/network/effect；
- Advisor仍Shadow；
- known debt count和rollback source完整。

---

## 13. Phase C8 / S2b — 最后才开放 bounded assignment

### C8 允许条件

仅对new child/new turn，且同时满足：

- S2a已PASS；
- no output/no thought/no tool/no effect的sealed receipt；
- no user pin；
- candidate在用户allowlist和privacy/endpoint policy内；
- tool/context兼容；
- provider health可用；
- budget reservation成功；
- capability不扩大；
- root approval记录完整；
- actual model receipt和ledger decision完整。

禁止global SetDefaultModel、替换stream、silent cross-provider、无限spend、Advisor PASS→completion、advice text→tool call。

### S2b product extension

在S2a corpus上只增加一次root批准的新child assignment。反例覆盖pin、private endpoint、breaker open、quota/pool exhausted、budget exhausted、schema mismatch、stale advice、already emitted output、thought/tool/effect/unknown。

Exit：exact binary显示advice→root approval→reservation→actual assignment→receipt完整因果；关闭consumer即可rollback，历史advice不改写。

---

## 14. Phase K1 — Kairos 和长期科研运行

### K1a — canonical Core local proof

**前置：** C2、C3、C4-B1/C4-D、C5-B NO_REPLAY、C6-B以及OperatorControl。Advisor不是前置。

Kairos不是新Agent或第二scheduler。它只能调用ManagedOperationPort：

~~~text
Draft → AwaitingScheduleApproval → Scheduled → Leased → Starting
      → AwaitingActorApproval → Running → Checkpointing
      → Succeeded | Failed | RetryScheduled | DeadLetter
      → Cancelled | Frozen | TakenOver | RecoveryRequired
~~~

Operator surface只能发typed actor commands：

每条命令还必须携带actor-issued `OperatorGrantV1`：subject/operator role、operation id、owner/project/session/workspace scope、允许动作集合、TTL/nonce/revoke、human approval ref、reason和audit receipt。拥有UI或daemon进程不等于拥有grant。

| Command | 作用 | 禁止 |
|---|---|---|
| InspectOperation | owner/lease/manifest/evidence/budget/queue/next safe action | raw secret、scratch、thought |
| FreezeOperation | 阻止新dispatch，有界drain | 把unknown effect标success |
| CancelOperation | revoke/cancel descendants/release budget/stop adapter | 只杀PID、删证据 |
| ApproveResume | 对明确operation/attempt签新approval/lease/manifest | 用旧UI文本或expired approval恢复 |
| TakeOver | reconcile后更新lease epoch | 双owner、绕receipt、replay effect |

fake-clock、two-holder、crash point、duplicate outbox、expired approval、cancel/late event、freeze/restart、exact binary start/ready/crash/reconcile/stop全部通过，才有KAIROS_LOCAL_GATE。

权限反例：foreign operator连operation metadata都不可见；跨owner/project/workspace的inspect/freeze/cancel/resume/takeover全部拒绝；expired/revoked grant、stale lease epoch、双holder失败；unknown effect即使operator有resume权限也仍保持Frozen，除非独立receipt满足C5-B。

`RetryScheduled`不是scheduler自行决定的普通状态；只有C5-B对该具体reason给出sealed `NoOutput + NoToolCall + NotAttempted + NoExternalEffect` receipt并获得新approval时才能进入。任何unknown、partial output或可能external effect都只能Frozen/RecoveryRequired，attempt count保持1。

短测试不等于24h。24h no-side-effect soak是单独授权和证据。

### K1b — Science managed-run proof

**前置：** K1a + PLATFORM_API + S1-B。

只接一个read-only/deterministic Science fixture。Scheduler只能请求Begin；worker只返回hash artifacts；SessionActor决定approval/finish/recovery。不能接live connector、任意shell、模型stream或外部effect。

必须证明：

- schedule/lease/attempt和Science run identity一致；
- crash前后artifact/terminal不重复；
- emitted/unknown状态Frozen；
- cancel cascade；
- operator inspect/freeze/resume/takeover；
- Desktop显示真实owner、lease、reason、queue和next safe action。

K1b是workflow/long-running M1-C前置，不阻塞short family去复制。

---

# Part IV — 真正完成单 Rust 底座

## 15. 当前双底座债务的准确规模

当前被machine lock监控的historical comparison是：

- copied Core：131 diverged + 5 missing = 136；
- duplicated xai-grok-science crate：31 shared-diverged + 22 Science-only = 53 duplicate delta；
- 现有drift基于audited Lumen dc563b1e，不是2026-08-02观察到的9ae4762，也不是未来可消费source A；
- Science branch仍包含整个agent/crates Core和大量Science-specific shell触点。

因此136不是全部双底座问题，也不能继续靠提高lock数字解决。每次提议新canonical pin都必须重新生成Core和Science-crate两份inventory。

## 16. Phase M1-A — 立即停止复制 Core 扩张

### M1-A0 — 现在即可执行的 anti-growth

**前置：** F0事实快照；不依赖Lumen R0。

交付：

1. 以当前Science ownership manifest冻结复制Core的现有边界；
2. guard禁止新增Science-specific SessionCommand、SessionHandle inherent、SessionActor method、hard-coded route和private Core import；
3. cargo metadata拒绝unapproved external path dependency；
4. 所有例外必须有owner、理由、到期卡和删除门；
5. 这只是“债务不再增长”，明确不宣称单底座完成。

### M1-A1 — R0后冻结source ownership

**前置：** M1-A0 + LUMEN_R0_SOURCE_GATE。

交付：

1. Core ownership map区分canonical-core、science-kernel、extension、product、legacy-delete；
2. 记录`LumenCoreSourceTupleV1`和rollback Core tuple；
3. xai-grok-science双份crate归属RFC：domain最终只在Science维护，canonical Lumen只持public host/sample fixtures；
4. API未PASS时仍不激活consumer pin。

### M1-A2 — PAPI后绑定active consumer pin

**前置：** M1-A1 + PLATFORM_API_GATE。

1. 唯一消费形态：versioned Rust governance API crate或versioned ACP extension protocol，不能混用本机path dependency；
2. active composition分别引用`LumenCoreSourceTupleV1`与`LumenPlatformApiReceiptV1`，不强迫A/B/API/CI同SHA，而是验证关系；
3. draft-only upgrade bot骨架；
4. rollback Core/API composition tuple；
5. mismatch或API downgrade直接STOP。

M1-A不删除现有Core，不等待Kairos；A0让债务不再增长，A1冻结source ownership，A2才把consumer绑定到可消费的canonical Core+API composition。

## 17. Phase M1-B — 按风险迁出短运行 family

**前置：** M1-A2 + PLATFORM_API + S1-B。

每个family固定十二步：

1. legacy behavior/authority map；
2. freeze request/result/artifact fixtures；
3. 提取pure schema/algorithm到science-kernel；
4. public operation descriptor/codec；
5. generic host actor path；
6. identity/approval/artifact/recovery negative corpus；
7. legacy vs generic parity；
8. ACP cutover；
9. Desktop cutover；
10. rebuilt binary；
11. exact CI；
12. 删除该family专用Core触点并降低drift。

迁移顺序：

| 顺序 | family | 原因 / 额外门 |
|---:|---|---|
| 1 | seq_analyze | S1 pilot，最强oracle |
| 2 | attachment / ZIP quarantine | immutable bytes，较短路径 |
| 3 | Skill revision / activation / revocation | 关闭S0-B后的完整替代；必须有真实Desktop product proof |
| 4 | evidence dossier / artifact queries | read-heavy，验证ArtifactSink/read model |
| 5 | project create/update/migrate/transition | revision/commit fence和recovery复杂 |
| 6 | review / collaboration records | claim/accepted snapshot和独立review |
| 7 | kernel admission | executable/model asset策略 |
| 8 | connector fetch / capability_run | network policy、no-replay、data terms |
| 9 | SSH/SCP/remote plan | effect/risk高；默认fixture-only |

### M1-B Skill 特别合同

状态机分开：

~~~text
DraftBytes → Prepared → AwaitingApproval → Running
          → Succeeded(immutable revision) | Denied | TimedOut | Cancelled | Failed

Succeeded revision
  → separate Activation approval
  → ActiveRevision record
  → Revocation tombstone
~~~

materializer只读actor-approved immutable revision和activation，验证SHA，写app-owned read-only copy。它不能审批、不能读legacy mutable content、不能自动reload未知runtime。

必须证伪create/update/delete/enable direct IPC、post-approval byte swap、path activation、revoked revision、cross-session、deny/timeout/cancel、materializer hash mismatch。

## 18. Phase M1-C — workflow / long-running 最后迁

**前置：** M1-B主要short families + K1b。

迁移workflow validate/dry-run/execute、environment/kernel run、managed connectors：

- 每stage都有operation/lease/manifest/budget；
- pinned executable/asset；
- partial artifact rollback；
- deterministic reuse验证真实bytes；
- crash/outbox/reconcile/no-replay；
- external effect无receipt永远Frozen；
- long-running product proof不能用short operation parity替代。

最后才删除Science copy中的workflow SessionCommand/actor/handle/run-loop专用路径。

## 19. 最终仓库和构建形态

~~~text
lumen source tuple A/B + governance API semver
       ↓ one exact source/API pin
lumen-science-kernel
       ↓
lumen-science-extension
       ↓
lumen-science-product / Desktop
~~~

最终验收：

- Science Cargo metadata没有复制Core crates和未批准path dependency；
- canonical Lumen只有一份可变SessionActor/tool/provider/memory/operation代码；
- Lumen仓不再维护另一份可变Science domain crate；
- Science专用route以descriptor registry贡献，不改Core enum/run loop；
- Go CLI/MCP v1.0.1冻结为legacy compatibility，停止独立新增authority；
- Desktop只启动source-lock-pinned composition；
- rollback只恢复上一个source/API pin，不手工合并上百文件。

## 20. Upgrade bot：以后底座更新只审一个 tuple

流程：

1. 观察immutable canonical Lumen source A和evidence B；
2. 核对A/B/main/tag/signature/release manifest关系；
3. 读取compat manifest和breaking changes；
4. 只创建Science draft PR；
5. 更新source/API pin和Cargo.lock；
6. 跑public adapter compile；
7. 跑所有已迁family parity/actor/product corpus；
8. 跑Desktop E2E/package smoke；
9. protected gate失败即STOP并列兼容差异；
10. 人工review后merge；
11. rollback=previous tuple。

机器人永不自动merge、改authority policy、覆盖protected Science files、接受breaking API、调用provider、tag或release。

# Part V — 把九源真正变成可用科研生产力

## 21. 通用 Capability Admission Contract

“看过源码”“有descriptor”“能列在UI里”都不等于可用能力。每一个进入Lumen Science的能力都必须有同一条、可机器验收的入场链：

~~~text
immutable source evidence
  → component rights / asset review
  → independently owned domain contract
  → parser + bounded fixture
  → SessionAuthorityPort Begin
  → explicit grant / approval
  → pinned runtime or pure Rust implementation
  → hashed artifacts + provenance + evidence
  → typed terminal state
  → replay/read model
  → rebuilt-binary product proof
  → exact-head CI
  → optional release admission
~~~

建议的公共Science描述符是：

~~~rust
pub struct ScienceCapabilityDescriptorV1 {
    pub capability_id: CapabilityId,
    pub schema_version: u32,
    pub domain: ScienceDomain,
    pub operation_class: OperationClass,
    pub input_schema_hash: Sha256,
    pub output_schema_hash: Sha256,
    pub tool_contract_hashes: Vec<Sha256>,
    pub runtime_asset: Option<PinnedAssetRef>,
    pub network_policy: NetworkPolicy,
    pub write_policy: WritePolicy,
    pub secret_policy: SecretPolicy,
    pub risk_class: ScienceRiskClass,
    pub evidence_profile: EvidenceProfile,
    pub provenance: SourceProvenance,
}
~~~

描述符只描述能力，不能自己获得执行权。真正运行时必须投影到`GovernedRunEnvelopeV1`，并由Core签发grant、预算、operation和artifact sink。

### 21.1 六级 admission 状态

| 状态 | 含义 | UI允许展示 | 允许执行 |
|---|---|---:|---:|
| `Cataloged` | 只知道来源和候选价值 | 是，标“候选” | 否 |
| `Quarantined` | 已抓取但权利、资产或安全未过 | 仅开发诊断 | 否 |
| `FixtureOnly` | 纯离线fixture可验证 | 是，标“离线样例” | 仅fixture |
| `Sandboxed` | pin runtime且沙箱产品证明通过 | 是 | 限定输入/环境 |
| `Managed` | actor、recovery、evidence全通过 | 是 | 是，按grant |
| `Released` | exact release、安装、回滚和声明均通过 | 是 | 是，按release policy |

状态只能由收据推进，不能由README、descriptor数量、模型回答或开发者口头说明推进。

### 21.2 Science risk class

| 类别 | 示例 | 默认策略 |
|---|---|---|
| `R0_PureRead` | 本地纯解析、序列统计 | 可做首批；无网络/写入 |
| `R1_BoundedData` | 固定fixture、只读数据库快照 | fixture-first；产物hash |
| `R2_NetworkRead` | Crossref/UniProt等在线查询 | 默认deny-live；需域名/secret/no-replay |
| `R3_LocalCompute` | Python/R/容器模型推理 | pinned runtime + sandbox +资源预算 |
| `R4_ExternalEffect` | 提交远程job、写外部仓库 | receipt/outbox/reconcile；默认关闭 |
| `R5_DeviceOrClinical` | 仪器、HIL、临床解释 | 独立安全计划；本总纲不授权live |

### 21.3 每个能力必须产出的证据包

~~~text
evidence/<capability>/<run_id>/
  source-provenance.json
  descriptor.json
  governed-envelope.json
  approval-receipt.json
  runtime-manifest.json
  input-manifest.json
  output-manifest.json
  artifacts/<store-owned-hash>/...
  claim-journal.jsonl
  terminal-receipt.json
  replay-receipt.json
  product-proof.json
~~~

这些是逻辑产物结构；实际写盘必须由store/artifact port完成，禁止能力代码拼接裸路径。

## 22. 九源价值地图：拿能力，不拿第二权威

当前九源lock是`draft`和E1/E2证据，不是admission。以下表只定义“要吸收什么”和“拒绝什么”，每项仍要经过21节合同。

| 来源 | 要吸收并拔高的能力 | 明确拒绝 | 首个可证伪切片 |
|---|---|---|---|
| Biomni | `biomni/tool/tool_description/**`中的生物医学工具taxonomy和descriptor语义 | agent runtime、任务分解实现、直接Python工具调用、未授权data lake | 现有UniProt离线能力升级为descriptor→actor→evidence product slice |
| Motif | 确定性序列分析、motif/ORF/翻译等纯函数能力和交互思路 | MCP installer、另一套approval/agent、受限catalog数据 | `seq_analyze`纯Rust黄金路径 |
| AIPOCH Open Science | bounded preview、connector types/registry、科学工作台信息架构 | MCP client manager、approval broker、ACP runtime、裸connector执行 | read-only catalog + 一个fixture connector |
| OpenClaudeScience | 仅`ui/src/app/skills/**`的catalog UX和技能发现 | `internagents`执行/工作流语义；受限docx/pdf/pptx/xlsx实现 | local catalog与独立clean-room文档技能contract |
| BGC-Prophet | 只把仓库存在性作为需求研究线索；BGC contract从公开标准和自有需求独立定义 | quarantine pipeline code、checkpoint/data、直接加载任意模型 | 独立输入验证/fixture contract；不做upstream行为复刻 |
| OpenDDE | 只把问题域作为roadmap线索；配置、artifact和评估contract独立定义 | quarantine runner、未审权重/MSA、任意pickle/unsafe loader、裸GPU job | 自有model-card schema + synthetic fixture；不解析upstream runner |
| AI4S Open Science | 只把“需要run projection”作为自有产品需求 | quarantine Tauri实现、布局/文案/测试、opencode profile、第二agent/runtime | 从Lumen actor read model独立设计run timeline |
| canonical Lumen | SessionActor、grant、operation、ledger、advisor shadow、Kairos、release transaction | 复制Core源码或消费dirty branch | C1公共API + S1 strangler |
| Lumen Science自身 | 已有ScienceStore、项目/工作流/review、fixture connectors、Desktop | 继续扩大复制Core和Go authority | M1 family parity迁移 |

### 22.1 machine-enforced component复用矩阵

当前v2 lock中只有以下component具备直接源码adapt的**候选资格**，条件必须同时满足`disposition=adapt`、`reuse_mode=adapt`、`rights_status=verified`：

| Source | component id | 唯一允许adapt的path |
|---|---|---|
| Biomni | `tool-descriptors` | `biomni/tool/tool_description/**` |
| Motif | `deterministic-sequence` | `src/bio/**` |
| AIPOCH | `bounded-preview` / `connector-types` / `connector-registry` | `src/main/skills/**`、`src/main/connectors/types.ts`、`src/main/connectors/registry.ts` |
| OpenClaudeScience | `catalog-ux` | `ui/src/app/skills/**` |

四个OpenClaudeScience office skill tree只能clean-room，禁止copy、derive和复用其fixture。BGC `src/**`、OpenDDE `runner/**`、AI4S Tauri、canonical Lumen历史`agent/**`和历史Science domain均不是可直接reuse component；MCP/agent runtimes全部reject。机械实施者必须让source lock/forbidden-path validator决定许可，不能根据本表的产品愿景扩大path。

在I1-B active `SOURCE_INTAKE_ACTIVE_GATE=PASS`和该component独立receipt之前，即使表中写`adapt`也只能做审计、差异计划和自有spec；禁止把source-derived code或fixture写入产品PR。历史已适配成果作为冻结oracle保留，不据此授权新增source path。

### 22.2 “源码级吸收”操作规范

允许复用的component：

1. 先验证component符合22.1三项machine条件，再保留source commit、path、license、notice和改造记录；
2. 只对允许adapt的path写behavioral characterization tests；
3. 将authority、I/O、path、network、secret全部替换为Lumen ports；
4. 删除自动安装、自动下载、隐式subprocess、裸路径写和自带agent循环；
5. 对科学算法做golden/negative/tolerance验证；
6. 产出“upstream行为→Lumen行为→有意差异”映射。

限制或禁止复用的component：

1. 不读受限实现来做“换名复刻”；
2. 由一名审计者只提取开放标准、公开接口和独立需求；
3. 另一名实现者只看clean-room spec；
4. 保存no-copy attestation、测试oracle的开放标准/自有来源和差异说明；禁止使用受限source fixtures作oracle；
5. 没有明确权利结论则一直`Quarantined`。

这不是保守少拿，而是保证拿来的能力最终能发布、升级、审计和长期拥有。

## 23. Wave W0 — Catalog、Skill和Knowledge Base先成为可信产品

W0-A只读catalog/schema/fixture可在I1-A后并行；W0-B任何create/update/delete/enable/activation、knowledge accept或runtime materialization必须等I1-B、S0-B、C1、M1-Skill和对应Core治理gate。提前做domain层不等于提前开放执行。

### W0.1 单一catalog

建立只读`ScienceCatalogV1`：

- 聚合built-in、adopted、user-owned revision；
- 每项显示source、license、admission state、risk、runtime、network、last proof；
- catalog本身不持有execution handle；
- 未知schema、缺hash、被撤销、源证据过期均不可运行；
- Desktop只读列表可以先保留，mutation必须走M1-B。

### W0.2 Skill本地化

Skill定义为版本化研究操作说明和schema，不是任意prompt或shell包：

~~~text
SkillRevisionV1
  id / revision / content_sha256
  declared_inputs / outputs
  permitted capability ids
  required grant scopes
  evidence requirements
  source / license / notice
  activation status / revocation
~~~

ZIP/.skill导入继续走quarantine Begin/Allow/Finish；create/update/delete/enable也必须统一。激活只引用不可变revision，不能让用户在审批后替换bytes。

### W0.3 双层知识

科学知识不得把“检索到的文本”和“已接受事实”混在一起：

- `KnowledgeArtifact`：原始文献、网页、附件、数据库记录；全部untrusted quoted data；
- `ResearchClaim`：由branch提出、带证据和限定；
- `AcceptedScientificFact`：root明确接受、可撤销、有版本；
- `LongTermKnowledge`：只从Accepted提升，保留来源、适用范围、反例和失效日期。

任何外部instruction、网页prompt、PDF隐藏文字不能提升grant、改变任务合同、切换模型或直接写Accepted Ledger。

### W0 Exit

- 四条Skill direct mutation产品负例持续通过；
- catalog能区分六级状态且不可把Cataloged误报Runnable；
- clean-room doc skills有独立spec、测试与attestation；
-知识提升只有root typed command；
- rebuilt Desktop证明Skill导入/激活、撤销、应用重启、hash swap均fail closed；clean-user app install另属G1/E6。

## 24. Wave W1 — 文献与数据库：从“搜索”变成证据链

优先顺序：Crossref → UniProtKB → Europe PMC → OpenAlex。一次只admit一个connector。

每个connector必须有：

1. versioned descriptor和query/result schema；
2. deterministic parser和至少三类fixture：正常、空结果、畸形/截断；
3. allowlisted host、HTTP method、redirect、size、MIME、timeout；
4. secret reference而不是secret value；
5. provider attempt sealed receipt；
6. raw response作为hashed artifact，preview有界且redacted；
7. normalized record保留DOI/accession、provider version、retrieved-at和source hash；
8. deny/timeout/cancel/network-unknown无“成功结果”；
9. replay从store-owned raw bytes重建相同normalized hash；
10. citation不能由模型凭空补齐。

以下三child路径属于`W1-T`，硬前置是S2a；`W1-M`只做root-only offline evidence chain。第一条三层产品黄金路径：

~~~text
research question
  → root creates literature task contract
  → child-1 runs fixture connector(s)
  → child-2 extracts bounded claims
  → child-3 independently checks citations
  → root accepts/rejects claim set
  → evidence table + review artifact
~~~

首版必须是offline fixture built-product E4；live读取仍为`pending-deny-live`，直到R2 network gate独立授权。禁止借“只读API”名义绕过no-replay、secret和provenance。

## 25. Wave W2 — Biomni：从224个候选到少数真正可运行能力

Biomni的大catalog是生产力素材，不是“一次性全开”的理由。实施分三批：

### W2-A 描述符归一化

- 对全部候选生成独立的`CapabilityBacklogClass`：`pure-domain`、`fixture-candidate`、`runtime-blocked`、`network-blocked`、`asset-rights-blocked`、`rejected`；它不是source disposition或admission state；
- 验证输入输出schema、依赖、数据源、风险和source evidence；
- unknown、动态eval、任意shell、自动pip/conda安装默认reject；
- inventory count只算Cataloged/E1来源盘点，不算可用数量。

### W2-B 首个和后续九个能力的来源绑定

先把已有`query_uniprot`映射做成完整receipt；其descriptor可绑定Biomni `tool-descriptors` component，但adapter/parser/fixture必须分别说明是允许adapt还是自有实现。FASTA/composition归W3，literature归W1，table/evidence工具归W0或独立Science domain，不能为了凑“十个Biomni能力”扩大descriptor许可。

后续九个候选只能由W2-A machine inventory按“高科研价值、低外部作用、可golden test”选出；本书不预先编造名称。每卡必须带`CapabilityProvenanceBinding`：source id、component id、exact source path/blob hash、reuse mode、destination path/hash，以及实现若为自有时的`IndependentSpecReceipt`。无绑定不得从Cataloged/FixtureOnly提升。

每个独立PR的E4 built-binary只足以把合格离线样例标为`FixtureOnly`。`Sandboxed`还要runtime/sandbox gate；`Managed`还要component/asset receipt、E2/E3、operation/recovery/grant/evidence/replay和E4；`Released`还要E6/E7。若声称CI覆盖，另须独立E5。

### W2-C 受管运行时能力

Python/R能力只能使用：

- immutable runtime asset manifest；
- protected trust root；
- fixed executable identity和loader closure；
- deny-exec/post-load策略；
- env allowlist；
- input staging和output collection；
- CPU/memory/time/artifact预算；
- no dynamic install；
- retained-store-only artifact。

macOS先实现，不以Windows gate阻塞本阶段；但跨平台声明保持`NOT RUN`。Linux CI必须使用可信test seam，不可把`/tmp`伪装成受保护runtime。

## 26. Wave W3 — Motif和确定性序列工作台

这是最适合先做深、做实的一组：纯Rust、易golden、科学解释性高。

功能切片按顺序：

1. `seq_validate`：alphabet、length、ambiguous symbols、normalization；
2. `seq_analyze`：composition、GC、basic ORF/motif summary；
3. `motif_scan`：明确pattern semantics、overlap、strand、coordinates；
4. `translate`：genetic code version、frame、stop/ambiguous rules；
5. `design_review`：输入设计与constraint检查，不自动宣称实验成功。

科研正确性要求：

- coordinate convention写进schema；
- reference fixtures来自可引用标准/独立生成；
- property tests覆盖empty、Unicode、huge input、ambiguous base、reverse complement；
- tolerance/rounding固定；
- 每个结论记录algorithm version和input hash；
- 报告分离“计算事实”“模型解释”“实验建议”。

`seq_analyze`既是C1/M1迁移pilot，也是W3第一个release candidate；只有G1/E7后才可标`Released`，不能因为算法测试通过就跳过actor/product/release gate。

## 27. Wave W4 — BGC-Prophet能力族

当前pipeline code仍quarantine，models/checkpoints/data仍reject-data-model。正确顺序：

1. 独立写`BgcPredictionInputV1`和`BgcPredictionResultV1`；
2. 先做FASTA/GenBank输入验证和synthetic fixture；
3. 从公开BGC标准、论文公开接口和自有需求独立定义stage graph；不得读取quarantine code做行为复刻；
4. 审核每个模型、checkpoint、数据库的权利、hash、格式和反序列化风险；
5. 建`ModelAssetManifestV1`和model card；
6. 在pinned offline sandbox跑小fixture；
7. 对已知positive、negative、edge dataset给precision/recall/confusion matrix；
8. 显示适用物种/数据分布/阈值/不确定性；
9. 产出结构化prediction和人类可读review；
10. 未完成独立benchmark前只能称“候选预测”，不能称科学验证。

禁止：自动下载权重、从pickle加载不可信对象、把upstream指标当本地证明、把一次demo当正式能力。

## 28. Wave W5 — OpenDDE与结构/设计推理

OpenDDE的inference config仍quarantine、weights/MSA仍restricted。分层实施：

- `W5-A`：从公开标准和自有需求独立写配置/schema/model-card validator；不得解析或复刻quarantine runner；
- `W5-B`：synthetic tiny model/fixture验证staging、budget、artifact、cancel；
- `W5-C`：权利和资产完整后，受管离线推理；
- `W5-D`：独立scientific benchmark和uncertainty；
- `W5-E`：只在明确授权后接GPU/HPC managed operation。

必须保存seed、software/container hash、hardware summary、model/weight hash、MSA/database version、input hash和每步duration。不可重现的结果不能提升为AcceptedScientificFact。

## 29. Wave W6 — 科研工作流、笔记本和环境

只对22.1明确允许的AIPOCH/OCS path做可追溯adapt；AI4S Tauri保持quarantine，不抽取其布局、文案或测试。workflow/notebook/run projection的其余体验从自有产品需求和Lumen actor read model独立设计：

- workflow是versioned DAG，不是prompt串；
- 每个node引用admitted capability和schema；
- validate与execute分离；
- dry-run展示grant、runtime、network、write、budget和预期artifact；
- notebook cell作为untrusted input，不能直接获得shell；
- environment是immutable manifest + resolved lock + proof，不是用户目录；
- stage failure只提交已声明的partial evidence，不能伪Succeeded；
- resume依据operation journal和artifact hash，不重跑未知外部作用。

第一条workflow应串联W1 fixture literature + W3 sequence analysis + root review，而不是直接上复杂HPC。

## 30. Wave W7 — HPC、设备和真实实验边界

生命周期固定为：

~~~text
Dummy → DigitalTwin → HIL → Real
~~~

每次晋级都要独立风险评审和人工授权。本总纲只允许规划到Dummy/DigitalTwin：

- SSH/SCP/remote command默认deny；
- job submission属于R4 ExternalEffect；
- 必须有idempotency key、sealed receipt、scheduler job id、outbox、poll/reconcile、cancel semantics；
- delivery unknown永远Frozen，不能自动重提；
- HIL/Real需独立设备安全、急停、操作员、物理边界和法规计划。

W7-A必须把`device_command`、`hardware_in_loop`、`real_device`维持`Disabled`，并禁止SSH/SCP/socket/credential/job submission。rebuilt binary在任何driver/transport之前返回`FeatureDisabled`；Dummy/Twin只产生deterministic hashed artifacts，E8固定`NOT RUN`。未来HIL/Real必须另过`DEVICE_SAFETY_GATE`，W7-A PASS不能改变该gate。

任何“能连上集群/设备”的截图不构成科学或安全证明。

# Part VI — 科研有效性、产品证明与发布

## 31. Scientific Validity Gate

工程跑通不代表科学正确。每个Science capability至少回答：

1. 它计算/预测的对象是什么？
2. 输入适用域和排除条件是什么？
3. reference或ground truth来自哪里？
4. metric、threshold和tolerance是什么？
5. uncertainty如何表示？
6. 失败、缺失和冲突证据如何展示？
7. 能否由保存的bytes、版本、seed、runtime重放？
8. claim是计算事实、统计推断、模型建议还是实验结论？

`SCIENTIFIC_VALIDITY_GATE`是per-capability-revision gate，不是全仓一次性绿灯。每次评估先冻结`ScientificValiditySpecV1`：ground-truth来源/权利/hash、train/validation/held-out split、leakage checks、metrics/tolerances、预注册threshold、calibration/uncertainty方法、适用域/失败域、seed/runtime/model hashes和independent reviewer。运行后保存raw predictions、metric calculation、deviation和review receipt，状态只能`NOT_RUN/BLOCKED/FAIL/PASS`。

阈值必须在看held-out结果前登记；改threshold等同新spec/new run。PASS只允许该revision的scientific claim按限定范围提升，仍不等于产品E4、CI E5或release E7。

### 31.1 测试层

| 层 | 目的 | 必需证据 |
|---|---|---|
| schema | 类型和边界 | positive/negative/property tests |
| algorithm | 计算正确性 | independent golden oracle、tolerance |
| data | 数据完整性 | source/version/hash/license/retention |
| model | 性能和适用域 | model card、held-out benchmark、calibration |
| workflow | stage组合 | deterministic fixture、failure injection |
| interpretation | claim边界 | citation、review、uncertainty、counterevidence |
| reproducibility | 重放 | exact input/runtime/seed/output hash |

### 31.2 禁止的科学宣传

- 未跑benchmark不得写“准确”“领先”“验证完成”；
- offline fixture不得写“实时数据库已接通”；
- AI解释不得写成已接受事实；
- computational prediction不得写成实验验证；
- 未做临床/法规验证不得给诊断或治疗结论；
- upstream benchmark不得冒充本地exact artifact结果。

## 32. Data、Model和Runtime Asset合同

统一`ScienceAssetManifestV1`：

~~~text
asset_id / kind / exact_sha256 / size / format
source_uri / source_commit_or_version
license / terms / attribution
download_method / expected_host
serialization_safety
runtime_compatibility
scientific_scope / known_limits
retention / deletion / revocation
review_receipt / admitted_at
~~~

规则：

- repo root license不覆盖nested data/model terms；
- asset未审不执行；
- URL不是identity，hash才是；
- runtime不得静默下载缺失asset；
- unsafe deserialization默认deny；
- asset撤销后已有结果保留provenance但不得新运行；
- model更新是新revision和新benchmark，不原位替换。

每个asset还要`AssetProvenanceBinding`，把source/component/path/blob、downstream asset hash、license/terms、transform和fixture来源绑定。当前BGC `models/**`与OpenDDE `models/**`是`reject-data-model/reuse_mode=none`：I1-B active也不能放开它们、派生权重或其fixture。synthetic tiny model必须first-party独立生成，并有自己的source/hash/license receipt。上述任一upstream rejected asset命中即fail closed。

## 33. macOS Product Gate G1

Windows暂不作为当前阻塞门；macOS是首个正式产品目标，但必须诚实标注平台范围。

**硬前置：** `SINGLE_BASE_GATE`、至少一个selected capability达到E4、canonical Lumen `NG10_RELEASE_FOUNDATION_GATE`和独立`UPDATER_TRUST_GATE`。Core的release transaction不能替代Science自己的product transaction；若updater不交付则必须从产品中完全禁用，不能保留未验fallback。

### G1.0 Science source A_S / evidence B_S

Science必须独立建立`ScienceProductSourceTupleV1`：

~~~text
science_source_commit_a_s
science_evidence_commit_b_s
evidence_suffix_base_a_s
lumen_core_source_tuple_ref
platform_api_receipt_ref
composition_lock_sha256
desktop_binary_source_a_s / binary_sha256
capability_catalog_sha256
runtime_asset_manifest_sha256
exact_ci_source_or_merge_commit
tag_commit_a_s (only if authorized)
rollback_science_tuple / rollback_lumen_tuple
~~~

`B_S`只能是`A_S`的allowlisted evidence-only suffix，不能夹入产品源码、版本、Cargo.lock、policy或runtime改动。Science tag如存在指向`A_S`；Core tag仍指Core A。verifier必须同时证明Core tuple、API receipt、Science tuple和composition lock的关系，任一交叉指错即STOP。

### G1.1 Desktop composition handshake

Desktop启动时校验并显示：

- Science source commit；
- `LumenCoreSourceTupleV1`的A/B关系和独立`LumenPlatformApiReceiptV1`；
- governance API version/hash；
- exact rebuilt `lumen` binary SHA；
- capability catalog hash；
- runtime/asset manifest hash；
- release manifest和evidence level。

任一不符进入diagnostic/read-only模式，禁止科学mutation和执行。

产品负例必须逐个破坏Core tuple、API receipt、extension manifest、composition lock、catalog/asset manifest和binary SHA，确认Desktop在建立session/driver前fail closed；不得fallback到peer agent、PATH或任意本机`lumen`、未注册extension、legacy Go authority或旧cache binary。

### G1.2 必跑产品路径

每个Released候选必须由“本次源码构建的确切binary”完成：

1. 创建project/session；
2. 提交任务；
3. durable Begin/AwaitingApproval；
4. deny路径，无artifact；
5. allow路径，运行并产出hashed artifact；
6. preview/review/accept；
7. restart/replay结果一致；
8. tamper后fail closed；
9. cancel/timeout终态；
10. owner/project/session/workspace越权拒绝。

命令级/handler级测试不能替代这个产品证明。

### G1.3 打包和安装

依次验收：

1. clean checkout source A/Science release candidate；
2. locked dependencies；
3. rebuilt binary和Desktop bundle；
4. SBOM、license notices、asset manifest；
5. codesign；
6. notarization；
7. clean macOS user install；
8. first-run migration；
9. offline golden path；
10. updater trust；
11. rollback到上一tuple；
12. uninstall/data retention说明。

本地未签bundle、CI artifact、GitHub release和已安装正式版是四种状态，报告不得混写。

## 34. 版本和发布权威收口

最终规则：

- canonical Lumen发布Core binary和governance API；
- Lumen Science发布Science extension/product composition；
- Science的版本不能伪装成Core版本；
- Desktop展示Core source tuple、Science version和catalog version三者；
- Go CLI/MCP v1.0.1进入`legacy-maintenance`：仅安全/兼容修复，不新增执行权威；
- Rust Science Core中历史`0.1.222`在M1前作为truthful legacy field保留，不能改数字掩盖复制事实；
- 单底座完成后删除第二Core发布路径，再按迁移策略处理版本；
- tag、release、install evidence都必须指向正确source transaction。

# Part VII — 可以直接开工的卡片、分工和验收

## 35. PR/Work Card总序列

每卡默认“一种责任、可独立回滚、先负例后实现、最后产品证明”。`Core`卡只在canonical Lumen仓做；`Science`卡只在本仓做。

| ID | 仓 | 内容 | 前置 | Owner建议 | Exit摘要 |
|---|---|---|---|---|---|
| S0-A | Science | 修Linux可信runtime测试夹具 | 当前HEAD | Grok实现/Codex验收 | exact-head CI不再因`/tmp`假runtime失败 |
| S0-B | Science | fail-close四条Skill direct IPC | 当前HEAD | Codex | real Electron注册负例绿 |
| F0-1 | Science | live execution snapshot schema | 无 | DeepSeek | stale/unknown/NOT RUN fail closed |
| F0-2 | Science | protected-path + authority lint | F0-1 | Grok | 新direct write/route扩张被CI拦截 |
| I1-A1 | Science | 九源component completeness审计 | 无 | DeepSeek收集/Codex裁决 | draft仍BLOCKED但无漏项 |
| I1-A2 | Science | rights/asset + transitive bridge | I1-A1 | Codex | disposition闭包可追溯，仍不active |
| I1-B | Science | active nine-source admission | I1-A2,L0-R0 | Codex | real lock PASS但不等于runnable |
| L0-P0 | Core | no-resubmit safety seal | Lumen dirty WIP | Lumen owner | source SHA/diff/tests/rollback；尚无A/B要求 |
| L0-R0 | Core | clean source A/evidence B | L0-P0 | Lumen owner | canonical tuple可验 |
| C1-RFC0 | Core | static composition/call direction/domain commit RFC | L0-R0 | Codex/Core owner | topology和authority边界冻结 |
| C1-API | Core | Governance/Adapter/Artifact/Domain ports | C1-RFC0 | Core owner | contract tests绿 |
| S1-A | Science | seq adapter compile/parity oracle | C1-API | Grok | 无Core inherent import；不dispatch |
| S1-B | Science | root-only governed seq product | S1-A,C3-A,C4-A,C6-B | Codex | actor/read-model/product等价 |
| C2 | Core | durable TaskTree lineage | L0-R0 | Core owner | forged/stale/orphan fail closed |
| C3-A | Core | CapabilityGrant替代raw child handle | C2 | Core owner | TTL/revoke/scope/leaf ceiling |
| C3-B | Core | Tool/Secret/Untrusted contracts | C3-A | Core owner | injection/secret negative corpus |
| C4-0 | Core | actor activity/unload safety | L0-R0 | Core owner | unload/late-event race fail closed |
| C4-A | Core | atomic TreeBudget | C2,C3-A | Core owner | parallel oversubscription被拒绝 |
| C4-B0 | Core | pre-dispatch operation journal | C4-0,C4-A | Core owner | 无manifest绝不dispatch |
| C4-D | Core | flow/delivery observation | C2,C4-B0 | Core owner | operation-bound unknown不报success |
| C5-A | Core | all unsealed replay paths disabled | L0-P0 | Core owner | no hidden retry |
| C5-B | Core | sealed ProviderAttemptReceipt | C5-A | Core owner | only proven NoOutput可恢复 |
| C6-A | Core | ClaimJournal/AcceptedSnapshot | C2 | Core owner | root-only accept/revoke/rebuild |
| C6-B | Core | ContextManifest | C6-A,C3-B,C4-A | Core owner | spawn/compact/resume同hash |
| C4-B1 | Core | manifest-bound operation recovery | C4-B0,C4-D,C6-B | Core owner | crash/unknown effect Frozen |
| C4-C | Core | WriteScopeLease | C3-A,C4-A,C4-B0 | Core owner | overlap/symlink/late apply拒绝 |
| C7 | Core | Advisor shadow receipt | C5-B,C6-B | Core owner | 无状态/输出/authority副作用 |
| S2a | Science | 三层shadow-only黄金路径 | C2–C7,S1-B | Codex | versioned mutation corpus全绿 |
| C8 | Core | bounded model assignment | S2a | Core owner | root approval + no partial stream |
| S2b | Science | bounded assignment产品扩展 | C8 | Codex | advice≠fact，assignment可审计 |
| K1a | Core | Kairos local operation proof | C4-B1,C4-D,C5-B,C6-B | Core owner | kill/restart/freeze/takeover |
| K1b | Science | long research managed run | K1a,S1-B | Codex | crash/resume/no duplicate effect |
| M1-0 | Science | freeze复制Core扩张 | 立即 | Codex | CI budget为零新增 |
| M1-Source | Science | Core source tuple + ownership freeze | L0-R0,M1-0 | Codex | source/rollback/ownership可验；不激活consumer |
| M1-Pin | Science | active Core/API composition pin | M1-Source,PLATFORM_API | Codex | API/pin/rollback/metadata可验 |
| M1-1..n | Science | 按family迁短运行 | C1,S1-B,M1-Pin | Grok逐卡/Codex验收 | parity后删copy |
| M1-Skill | Science | 完整Skill actor迁移 | PLATFORM_API,S1-B,S0-B,M1-Pin | Codex | immutable revision/activation |
| M1-Long | Science | workflow/runtime迁移 | M1-B,M1-Pin,K1b | Codex | long operation parity |
| W0-A | Science | read-only catalog/schema/fixture | I1-A2 | Grok/DeepSeek | Cataloged不误报Runnable |
| W0-B | Science | skill/knowledge governed mutation | I1-B,M1-Skill,C3-B,C6-A | Codex | built-product E4 |
| W1-D | Science | 自有connector schema/parser/fixtures | I1-A2 | Grok/DeepSeek | pure E2，不称admitted |
| W1-M | Science | root-only offline evidence chain | I1-B,PLATFORM_API,S1-B,C3-B,C4-B1,C6-B | Codex | fixture E4，live仍deny |
| W1-T | Science | 三层文献claim/review黄金路径 | W1-M,S2a,C6-B | Codex | child evidence/root accept E4 |
| W2-A | Science | Biomni descriptor backlog triage | I1-A2 | Grok/DeepSeek | classification不等于admission |
| W2-B | Science | pure/fixture capability逐个接actor | I1-B,PLATFORM_API,S1-B,C3-B,C4-A,C6-B | Grok机械/Codex | 每能力FixtureOnly E4；CI另需E5 |
| W2-C | Science | sandboxed/managed runtime capability | W2-B,C4-B1,C4-D,C5-B,AssetProvenanceBinding | Codex | sandbox/recovery/replay/product |
| W3-D | Science | Motif算法/oracle深化 | I1-A2 | mixed | scientific E2，不扩大source path |
| W3-P | Science | seq workbench managed/released path | I1-B,S1-B,W3-D | Codex | scientific E2 + actor E3 + product E4 |
| W4-A | Science | 自有BGC contracts/fixtures | I1-A2 | specialist | 不读quarantine code复刻 |
| W4-B | Science | first-party/independently licensed asset sandbox | W4-A,I1-B,PLATFORM_API,S1-B,C3-B,C4-B1,AssetProvenanceBinding | Codex | rejected upstream assets仍拒绝；Sandboxed E4 |
| W4-C | Science | held-out benchmark/calibration | W4-B,ScientificValiditySpecV1 | independent reviewer | per-capability validity PASS/BLOCKED/FAIL |
| W5-A | Science | 自有OpenDDE contracts/fixtures | I1-A2 | specialist | 不读quarantine runner复刻 |
| W5-B | Science | first-party/independently licensed managed inference | W5-A,I1-B,PLATFORM_API,S1-B,C3-B,C4-B1,AssetProvenanceBinding | Codex | rejected upstream assets仍拒绝；reproducible E4 |
| W5-C | Science | held-out benchmark/uncertainty | W5-B,ScientificValiditySpecV1 | independent reviewer | per-capability validity PASS/BLOCKED/FAIL |
| W6 | Science | managed workflows/notebooks | I1-B,PLATFORM_API,K1b,W1-M,W3-P | Codex | DAG/recovery/product |
| W7-A | Science | Dummy/DigitalTwin only | K1b,W6,feature snapshot | Codex | FeatureDisabled before transport；E4 only；E8 NOT RUN |
| G1 | Science | macOS signed release transaction | SINGLE_BASE + selected W E4 + Core NG10 + UPDATER_TRUST | Codex | Science A_S/B_S、install/rollback/release E7 |

## 36. 三类执行者的明确分工

### Codex负责难判断和最终验收

- authority/permission/operation/replay架构；
- public API设计；
- rights和clean-room裁决；
- test seam是否破坏保护策略；
- scientific claim/benchmark标准；
- diff ownership、拆commit、GitHub exact-head和release判断；
- 独立重跑，不接受实现者摘要代替证据。

### Grok 4.5负责边界清楚的机械实施

- descriptor/schema/fixture生成；
- 路由或adapter机械迁移；
- 表驱动negative tests；
- 文档索引、manifest和CI wiring；
- 一卡一提交，绝不跨保护区；
- 不做authority设计、不放宽policy、不merge/push除非卡片明确授权。

### DeepSeek Flash负责扫描、对账和测试矩阵

- source/component inventory；
- hash/license/path/readme索引；
- stale docs、route/direct write静态扫描；
- test name/count/raw exit汇总；
- generated fixtures和schema conformance；
- 只能提出差异，不能宣布admission、release或科学正确。

### 36.1 通用交接词模板

~~~text
TASK: <single card id and outcome>
REPO/CWD: <absolute path>
BASE: <branch + full HEAD + upstream/divergence>
OWNED PATHS: <exact allowlist>
PROTECTED PATHS: <exact denylist>
READ FIRST: <exact files/lines/contracts>
IMPLEMENT: <bounded steps>
MUST PROVE FALSE: <negative cases>
COMMANDS: <exact format/check/test/product commands>
EVIDENCE: raw log paths + exit codes + test counts + diff stats
STOP IF: dirty overlap / base moved / contract ambiguity / network needed / policy would loosen
FORBIDDEN: reset clean stash rebase merge broad checkout git add -A live/provider/deploy
DELIVERY: no claim of CI/release; leave commit/push policy exactly as stated
~~~

### 36.2 Codex独立验收模板

~~~text
1. Reconcile cwd/top-level/branch/HEAD/upstream/status/remote/processes.
2. Confirm only owned paths changed; inspect every diff hunk.
3. Re-derive the threat/failure model; do not trust handoff counts.
4. Run formatter/check/focused unit/negative/product gates with raw exits.
5. Verify built binary hash and source composition.
6. Re-query GitHub exact commit/checks when remote truth is claimed.
7. ACCEPT only if source + negative + product evidence agree.
8. Otherwise REJECT with exact failing invariant and smallest next card.
~~~

## 37. 每一卡的统一 Definition of Done

一张卡只有同时满足以下条件才叫完成：

- scope和protected paths未漂移；
- implementation存在，不只是文档/descriptor；
- 正向、拒绝、timeout、cancel、越权、tamper、restart覆盖适用项；
- raw exit和测试计数保存；
- `git diff --check`通过；
- 对应crate/desktop focused check通过；
- 若卡声称产品可用，rebuilt-binary E4通过；
- 若卡声称CI，exact commit E5通过；
- 若卡声称package/install smoke，E6通过；
- 若卡声称发布，installed release E7通过；
- rollback路径已实际验证或明确NOT RUN；
- 不提升相邻未测gate。

## 38. 标准命令与证据保存

开始任何编辑前：

~~~bash
pwd
git rev-parse --show-toplevel
git branch --show-current
git rev-parse HEAD
git status --short
git remote -v
git rev-list --left-right --count HEAD...@{upstream}
pgrep -af 'cargo|rustc|node|npm|vitest|electron|lumen' || true
~~~

Science当前机器合同（每次仍须以仓内脚本为准）：

~~~bash
python3 scripts/verify-nextgen-baseline.py
python3 scripts/test-nextgen-baseline.py
python3 scripts/verify-upstream-lock-v2.py       # draft阶段预期BLOCKED，不能改成假0
python3 scripts/test-upstream-intake-v2.py
python3 scripts/test-upstream-component-coverage.py
python3 scripts/test-upstream-containment.py
python3 scripts/check-core-drift.py --self-test
bash scripts/science-machine-gates.sh
git diff --check
~~~

Rust focused验证按改动选择，不得用pipe吞Cargo exit：

~~~bash
cargo fmt --all -- --check
cargo check -p xai-grok-science
cargo check -p xai-grok-shell
cargo test -p xai-grok-science <focused-filter> -- --nocapture
cargo test -p xai-grok-shell <focused-filter> -- --nocapture
~~~

Desktop：

~~~bash
npm run typecheck
npx vitest run <focused-test-files>
~~~

真实命令必须在正确Cargo/Desktop目录执行；日志写到仓外固定evidence目录或明确artifact目录，并记录：UTC/北京时间、cwd、full HEAD、binary SHA、command、exit、passed/failed/ignored、NOT RUN门。

## 39. STOP / rollback矩阵

| 观察 | 立即动作 | 不允许动作 |
|---|---|---|
| 工作树出现非owned dirty | STOP并报告 | 覆盖、stash、checkout恢复别人内容 |
| upstream/base移动 | 重做只读reconcile | 在未知基线上继续 |
| Lumen无R0 tuple | Science继续独立F0/I1/纯domain工作 | pin dirty/PR branch |
| 权利/asset不明确 | quarantine | 换名复制或执行 |
| approval/receipt未知 | Frozen/Failed按合同 | retry/resubmit/finish success |
| built test只能靠放宽trust root过 | 修test seam | 放宽生产policy |
| CI synthetic merge绿但HEAD不同 | 标注merge-candidate only | 报exact-head绿 |
| provider/live需费用或secret | NOT RUN/等授权 | 偷跑live |
| scientific benchmark不达标 | 保留fixture/research状态 | 改阈值或删负例假绿 |
| release门缺签名/安装/回滚 | 不发布 | 把本地bundle称release |

rollback按层执行：adapter/card revert → previous capability revision → previous Science release → previous Lumen source tuple。不得用全仓reset替代可审计回滚。

# Part VIII — 里程碑、完成度和最终出口

## 40. 里程碑与诚实进度算法

正式顶层gate按九项计分，每项只能`PASS`或不计分：

| 顶层gate | 权重 | 当前（2026-08-02观察） |
|---|---:|---|
| S0 Science P0 clean exact-head | 10 | 0：PR红且Skill bypass存在 |
| I1 nine-source admitted intake | 8 | 0：draft/evidence-collected |
| L0 canonical Lumen source gate | 12 | 0：Core未R0 |
| C1 public governance API | 12 | 0：不存在 |
| C2–C7 governed autonomy foundation | 15 | 0：局部实现非稳定API |
| S2 three-level Science product proof | 10 | 0：未跑 |
| K1 managed long-run/recovery | 10 | 0：未跑 |
| M1 single Rust base | 13 | 0：复制Core仍在 |
| G1 macOS released product slice | 10 | 0：未发布 |

因此正式gate完成度是`0/100`，不是说一天成果没有价值。已有SessionActor闭环、600量级local tests、Desktop、ScienceStore、connector fixtures、sandbox、source intake和计划合同构成约`33/100`的“开工准备度”。二者必须分栏报告。

未来每次状态更新由machine registry和evidence receipt计算，不凭文字估百分比。部分完成以`IMPLEMENTING/BLOCKED/NOT RUN`描述，不折算成PASS。

## 41. 四个可感知产品里程碑

### Milestone A — 不再继续漏水

完成：S0-A、S0-B、F0、M1-0、I1-A completeness；I1-B仍等待Lumen R0。

用户得到：当前能力不再被假绿，Skill直写入口被封，九源账本和底座边界可信。

### Milestone B — 第一条单底座科研能力

完成：L0、C1、S1、W3首切片。

用户得到：`seq_analyze`从canonical Lumen公共API运行；Science不再改Core热文件；升级一个tuple即可验证第一条能力。

### Milestone C — 下一代受控科研Agent

完成：C2–C7、S2a，再到C8/S2b。

用户得到：主Agent可生三层子Agent，但每层有真实lineage、grant、预算、上下文、ledger、证据和root合流；Advisor先shadow后受控选型，不能替子Agent兜底幻觉。

### Milestone D — 真正可长期使用的Science产品

完成：K1、M1、选择的W1/W2/W3产品波、G1。

用户得到：macOS安装版能跑至少一条文献证据链和一条序列分析链，断电可恢复、产物可重放、底座可由机器人提议升级并经人工审核/授权回滚，没有第二Rust Core authority。

## 42. 时间估算（不是承诺，按gate重估）

在一个主工程师、Grok/DeepSeek机械并行、无重大Core破坏性变更的条件下：

| 阶段 | 工程量级 | 主要不确定性 |
|---|---|---|
| A：S0/F0/I1 | 3–7工程日 | Electron真实产品测试、权利完整性 |
| B：L0/C1/S1 | 2–5周 | canonical Lumen R0和公共API评审 |
| C：C2–C7/S2a | 4–8周 | durable operation、grant、context/replay |
| C后半：C8/S2b/K1 | 3–6周 | provider receipt、crash/recovery、24h范围 |
| M1单底座 | 4–10周，可与C并行一部分 | 复制Core family parity和长运行 |
| 首批W1/W2/W3产品 | 3–8周，可流水线 | 科学oracle、runtime、asset/network gate |
| G1 macOS release | 1–3周 | 签名、公证、安装、回滚、Core NG10 |

它们不是简单相加：F0/I1/W3纯domain可并行；C1后M1短family和能力adapter可流水线；C8必须等S2a；G1必须等Core release foundation和至少一个E4能力。任何人都不能用赶工绕过依赖。

## 43. 接下来十个实际动作

1. 在当前Science HEAD先完成并独立验收S0-A，不改生产trust policy。
2. 完成S0-B，真实Electron注册后四条mutation fail closed。
3. 提交F0 live snapshot设计和protected path清单。
4. 完成I1 source-to-component completeness和rights/asset待办，不把draft改绿。
5. 在Science CI加入“禁止复制Core继续扩张”和“禁止新direct mutable store”的静态门。
6. 等canonical Lumen交付P0-NR/R0 A/B收据，并独立核验。
7. 在Lumen提C1-RFC；Science同时冻结`seq_analyze`parity oracle。
8. C1 API通过后迁`seq_analyze`，做第一条单底座built-binary proof。
9. 以S1经验固定family migration kit，机械迁短运行能力。
10. 再按C2→C7→S2a的硬依赖推进三层科研Agent，不提前打开bounded assignment或Kairos 24h宣传。

## 44. 最终 Definition of Done

Lumen Science 5.0只有在以下全部成立时才完成：

- canonical Lumen Core只有一份可变源码和一个执行/权限/产物/operation权威；
- Science只依赖一个可验证的Lumen source A/evidence B tuple和versioned governance API；
- Science仓不再复制Core热文件，也不发布第二Rust Core；
- Go产品明确legacy且没有独立新authority；
- 三层Agent真实运行，lineage/grant/budget/context/ledger/claim/flow/recovery全部durable且fail closed；
- Advisor不能接受事实、修补子Agent幻觉或绕root；bounded assignment只有独立收据后启用；
- Kairos/daemon只能托管GovernedOperation，kill/restart/unknown effect不重复副作用；
- 九源每一个component都有exact source、rights、asset、disposition和provenance；
- 被吸收的好能力已变成Lumen-native descriptor/adapter/artifact/evidence/product，而非仅catalog/文档；
- 至少一条文献证据工作流和一条确定性序列工作流达到E7 macOS安装版证明；
- scientific benchmark、uncertainty、claim边界和reproducibility满足31节；
- tamper、deny、timeout、cancel、越权、crash、stale、unknown和rollback均有反证；
- exact-head CI、SBOM、签名、公证、安装、升级、回滚和release manifest全部可核对；
- 报告明确区分source、focused tests、built product、CI、release和live/device；未跑的永远写`NOT RUN`。

# Appendix A — Canonical gate crosswalk

~~~text
Lumen P0_NR_SAFETY_GATE
  → Science L0 consumption

Lumen R0_SOURCE_GATE (source A + evidence B)
  → C1 / C2 / C5 / M1-A

TOOL_CONTRACT + SECRET_BOUNDARY + UNTRUSTED_CONTENT
  → S2a + every runnable W capability

TREE_BUDGET + OPERATION_RECOVERY + WRITE_SCOPE + FLOW_CONTROL
  → S2a / K1 / W6

CLAIM_JOURNAL + ACCEPTED_SNAPSHOT + CONTEXT_MANIFEST
  → C7 / S2a / K1 / scientific knowledge

NO_REPLAY_GATE
  → C7 / C8 / network connectors / remote operations

S2a + HARNESS_REGRESSION_GATE
  → C8 Applied enablement

OPERATOR_CONTROL_GATE + operation recovery
  → K1a → K1b

NG10_RELEASE_FOUNDATION + UPDATER_TRUST_GATE + SINGLE_BASE_GATE + selected W at E4
  → Science G1
~~~

# Appendix B — 文档权威和 supersession

本书是从2026-08-02起Lumen Science的唯一排序、依赖、状态口径和最终出口。它不删除旧书中的细节，而是按以下关系使用：

- `LUMEN_SCIENCE_NEXTGEN_FINAL_EXECUTION_BOOK_2026-08-01.md`：上一版Science总纲，保留历史和细节oracle；若与本书冲突，以本书为准。
- `EXTREME_ADOPTION_SINGLE_BASE_EXECUTION_PLAN_2026-08-01.md`：单底座/M1细节来源；顺序和A/B模型以本书为准。
- `NEXT_GENERATION_AUTONOMY_CONTROL_PLANE_EXECUTION_PLAN_2026-08-01.md`：自治控制面细节来源；C2–C8依赖以本书为准。
- `LUMEN_SCIENCE_5_0_ULTIMATE_IMPLEMENTATION_PLAN_2026-07-28.md`：产品愿景和历史里程碑来源，不是当前状态。
- canonical Lumen的`LUMEN-NEXTGEN-EXECUTION-BOOK-2026-08-01.md`：Core设计和交付合同来源，不是Science任务清单，也不能凭文档状态成为pin。

计划修改后必须同步`PLAN_SUPERSESSION_MAP.md`和本目录README；旧计划不再独立改变执行顺序。

# Appendix C — 非目标

本书不授权：

- 触碰隔壁Lumen dirty工作树；
- 自动merge、tag、push、deploy或live provider调用；
- Windows产品宣称；
- 真实HPC、设备、临床或实验室控制；
- 复制受限代码、data、model或用“重构”规避权利；
- 用submodule、patch stack或提高drift数字永久维持第二Core；
- 用更多Agent替代合同、证据、review和root合流；
- 用Advisor替子Agent纠正幻觉；
- 用计划、测试名或source check冒充真实产品和release。

这份书的目标不是把所有想法一次写进代码，而是把每一步变成：能开卡、能停、能证伪、能回滚、能独立验收，并最终收敛到一个可升级的Rust Lumen底座和一个真正可用的Lumen Science产品。
