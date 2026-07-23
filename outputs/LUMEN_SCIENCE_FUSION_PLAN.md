# Lumen Science 融合方案

**状态：** 执行中；架构决策已冻结，所有外部能力仍逐项 default-deny
**日期：** 2026-07-23
**底座：** Rust Lumen 内核（已完工，Phase A/B/C 全部验收）
**纳入源：**
- `github.com/synthetic-sciences/openscience` (synsci) — CLI/Bun 工具，SolidJS 前端，279 技能，42 连接器
- `github.com/aipoch/open-science` (aipoch) — Electron 桌面应用，23 连接器(200+ 工具)，5 层 MCP 服务器，持久笔记本内核
- `github.com/jvogan/motif` (Motif) — 分子生物学工作台，自包含 HTML，MCP 服务器

---

## 0. 勘探核心发现

### 三个源是三个完全独立的项目

| | synsci | aipoch | Motif |
|---|---|---|---|
| 架构 | Bun CLI + SolidJS 前端 | Electron 桌面应用 | 自包含 HTML + MCP 服务端 |
| 语言 | TypeScript (Bun) | TypeScript (Electron/Node) | TypeScript (React 19 + Vite) |
| 许可证 | Apache 2.0 | Apache 2.0 | MIT |
| 技能 | ~279 (16 类别) | ~18 精选 | 无（仅分子生物学功能） |
| 连接器 | 42 (REST) | 23 (200+ MCP 工具) | 无 |
| MCP 服务器 | OAuth 代理 | 5 层（artifacts / notebook / reviewer / HTTP bridge / user custom） | 2 工具（open_workbench / create_artifact） |
| UI | SolidJS SPA + 9 科学渲染器 | React 19 + shadcn | 自包含 HTML（离线可用） |
| 笔记本内核 | 无 | Python + R + REPL（持久，跨内核文件交接） | 无 |
| 运行时管理 | 无 | Claude Code / OpenCode / Codex 可插拔后端 | 无 |
| 安装方式 | npm + 原生二进制 | Electron 安装包（Apple 公证） | npm ci + build |

**关键结论：synsci 和 aipoch 不是 fork 关系——代码零重合。它们是从不同方向（CLI vs 桌面应用）解决同一问题的两个独立项目。**

### 我们已有的（Lumen Science 内核）

| 模块 | 状态 | 证据等级 |
|---|---|---|
| `connectors` (PubMed + ChEMBL) | ✅ | L4 e2e |
| `import` + `preview` (CSV/FASTA, magic byte sniffing) | ✅ | L4 e2e |
| `transport` (offline file transfer, timeout/cancel/kill) | ✅ | L4 e2e |
| `durable` storage (runs/artifacts/evidence/provenance/approvals) | ✅ | L4 e2e |
| `review` + `science_goal` (L3 unit-verified) | ✅ | L3 |
| ACP protocol bridge (permission_handle.request, AccessKind::Bash) | ✅ | L4 e2e |

---

## 1. 融合架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                    Lumen Science (Rust 内核)                      │
│                                                                  │
│  ┌──────────┐  ┌───────────┐  ┌──────────┐  ┌───────────────┐  │
│  │ Durable  │  │ Permission│  │   ACP    │  │   Transport   │  │
│  │ Storage  │  │  Bridge   │  │ Protocol │  │   (SCP)       │  │
│  └──────────┘  └───────────┘  └──────────┘  └───────────────┘  │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              连接器层 (Connector Layer)                    │   │
│  │  ProtocolAdapter trait → 42 连接器 (P1→P3 分批扩展)        │   │
│  │  从 synsci 取连接器清单, 在 Rust 内核中实现                   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              MCP 桥接层 (MCP Bridge)                       │   │
│  │  从 aipoch 借鉴:                                            │   │
│  │  • artifacts MCP server（工件注册/安全写入）                  │   │
│  │  • notebook MCP server（持久 Python/R/REPL 内核）            │   │
│  │  • reviewer MCP server（选择加入式专家审查）                    │   │
│  │  • HTTP bridge（为不接受 stdio 的框架桥接 MCP）                 │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              技能层 (Skills Layer)                         │   │
│  │  从 synsci 取 ~370 个 MIT/BSD/Apache 技能 → ACP 扩展格式     │   │
│  │  从 aipoch 取 ~18 个精选计算生物学技能                        │   │
│  │  排除 7 GPL + ~40 Unknown（逐个人肉）                         │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              科学 UI 层 (S1 Renderers)                     │   │
│  │  从 synsci 取 9 个科学渲染器 (Mol*/RDKit/IGV/KaTeX/...)    │   │
│  │  + Motif 分子生物学工作台 (第 10 个渲染器)                    │   │
│  │  通过 ArtifactRenderer 框架接入，轻量 web view 渲染          │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 分轨实施计划

### 轨道 1: 连接器全量扩展 (Rust)

**源：** synsci 42 连接器清单 → Lumen Rust 内核

**已完工：** PubMed, ChEMBL (2/42)

**架构扩展：**
```
connectors/
├── mod.rs          ← + ProtocolAdapter trait, HashMap registry
├── fetch.rs        ← trait-based dispatch (替换 match 语句)
├── pubmed.rs       ✅ 已有
├── chembl.rs       ✅ 已有
├── uniprot.rs      🆕 P1
├── europepmc.rs    🆕 P1
├── crossref.rs     🆕 P1
├── openalex.rs     🆕 P1
├── semantic_scholar.rs 🆕 P1
├── arxiv.rs        🆕 P1 (XML 解析)
├── rcsb_pdb.rs     🆕 P1
├── alphafold.rs    🆕 P1
├── ensembl.rs      🆕 P1
├── pubchem.rs      🆕 P1
├── (P2: 18 个)     🆕
└── (P3: 12 个)     🆕
```

**每连接器成本：** 80-150 行 Rust + fixture + L5 live probe

| 梯队 | 数量 | 时间 |
|---|---|---|
| P1 高价值 (UniProt, PDB, arXiv 等) | 10 | 2-3 天 |
| P2 域覆盖 (ClinVar, STRING, KEGG 等) | 18 | 2 天 |
| P3 专项 (BindingDB, GtoPdb 等) | 12 | 1 天 |
| **总计** | **40** | **5-6 天** |

---

### 轨道 2: MCP 桥接层 (Rust + Node)

**源：** aipoch 的 5 层 MCP 服务器架构

**融合方式：** Lumen 本身不变成 Electron 应用。而是通过 ACP 协议桥接以下 MCP 功能：

#### 2a. Artifacts MCP（工件注册）

**借鉴 aipoch 的 `open-science-artifacts` 设计：**
- Lumen 已有 durable storage 和 artifact 管理
- 新增：`write_artifact_file` MCP 工具，安全地将生成文件注册到当前运行
- 权限：应用级控制，智能体不能直接写工作区
- 集成点：Lumen 的 `api.rs` artifact 路由 + 新增 MCP stdio 子进程

#### 2b. Notebook MCP（持久计算内核）

**借鉴 aipoch 的设计：**
- 新增 `lumen-notebook` MCP 服务器，提供：
  - `notebook_execute` — Python 代码单元执行
  - `notebook_restart` / `notebook_state` / `notebook_shutdown`
  - `manage_packages` — 通过 conda 管理包
  - `manage_environments` — 命名的 conda 环境
- 内核：Python (默认), R (可选), REPL (Node.js)
- 跨内核文件交接：`./handoff/` 目录
- 安全：内核在隔离的 conda 环境中运行，非特权

**为什么这很重要：** 当前 Lumen Science 只能做数据查询/转换/传输，不能执行科学计算。笔记本内核补上了这个缺口——让智能体调用 Python 跑统计分析、作图、模拟。

#### 2c. Reviewer MCP（专家审查）

**借鉴 aipoch 的 `open-science-reviewer` 设计：**
- 选择加入式审查：智能体完成任务后，后台启动审查者
- 审查者对账：工具声明 vs 执行日志 vs 工作成果
- 输出：结构化通过/警告/失败 + 可中断的修复循环
- 当前 Lumen 已有 `review.rs` + `science_goal.rs` (L3)，审查者 MCP 将其升级到 L4

#### 2d. HTTP Bridge（框架桥接）

**借鉴 aipoch 的 `AgentMcpHttpHost`：**
- 把 MCP stdio 服务器桥接到 HTTP（带 Bearer 认证）
- 使不支持 stdio 的外部工具也能访问 Lumen 的 MCP 服务

---

### 轨道 3: 技能移植 (Markdown → ACP 扩展格式)

**源：** synsci ~279 + aipoch ~18 + Motif skill

**两阶段：**

| 阶段 | 内容 | 数量 | 时间 |
|---|---|---|---|
| 自动通过 | MIT/BSD/Apache 技能 | ~370 | 2 天 |
| 人肉把关 | GPL 毒瘤 (7) + 缺少许可证 (40+) | ~50 | 1 天 |

**排除清单（永不移植）：**
- 7 GPL 技能（bioservices, pathml, cobrapy, scikit-survival, etetoolkit, denario, fluidsim）
- ~20 Grok 内置技能（xAI 专有，无许可证）
- KEGG/HMDB 数据库技能（商业许可限制）

**移植格式：** 每个技能从 SKILL.md → ACP extension descriptor
```
skill_id: "synsci/biology/pydeseq2"
display_name: "DESeq2 Differential Expression"
license: "MIT"
category: "biology"
protocol: "ACP extension"
```

---

### 轨道 4: 科学 UI 层 (SolidJS 渲染器)

**源：** synsci 的 `frontend/workspace/src/science/renderers/`

**移植策略：只搬 S1 科学渲染器层，不搬整个 workspace chrome。**

| 渲染器 | 用途 | 依赖 | 移植难度 |
|---|---|---|---|
| ProteinStructure | 3D 大分子查看器 | Mol* (molstar 5.10) | 中 |
| Chem2D | 2D 化学结构 | RDKit.js | 中 |
| GenomeTrack | 基因组浏览器 | IGV.js 3.8 | 中 |
| KaTeX / Latex | 数学公式 | KaTeX | 低 |
| PdfViewer | PDF 内联查看 | pdfjs-dist | 低 |
| SequenceViewer | 序列展示 | 纯 Canvas | 低 |
| MsaViewer | 多序列比对 | 纯 Canvas | 低 |
| ImageView | 图片查看 | 浏览器原生 | 低 |
| Motif | 分子生物学工作台 | 自包含 HTML | **极低** |

**Motif 是最好集成的——它是自包含 HTML，通过 MCP 生成，零修改接入。**

**集成模式：**
```
Lumen 科学扩展 → 生成 Mol/序列/基因组数据
    ↓
ArtifactRenderer 注册表 → 匹配 artifact kind
    ↓
对应渲染器 → 在轻量 web view 中渲染
```

---

## 3. 总工时估算

| 轨道 | 内容 | 工期 |
|---|---|---|
| 1: 连接器 | 40 个 Rust 模块 + ProtocolAdapter trait | 5-6 天 |
| 2: MCP 桥接 | Artifacts + Notebook + Reviewer + HTTP Bridge | 4-5 天 |
| 3: 技能移植 | ~370 自动 + ~50 人肉 | 3 天 |
| 4: 科学 UI | 9 渲染器 + Motif + ArtifactRenderer 框架 | 4-5 天 |
| 集成测试 | 端到端融合测试 | 2 天 |
| **总计** | | **18-21 天** |

---

## 4. 与 Open Science 的最终关系

```
Open Science (上游)          Lumen Science (我们的)
─────────────────────        ──────────────────────
synsci: 技能/连接器清单  ──→  移植到 Rust 内核 + ACP 扩展
aipoch: MCP 架构/笔记本  ──→  MCP 桥接层 (Artifacts/Notebook/Reviewer)
Motif: 分子生物学工作台  ──→  第 10 个 ArtifactRenderer
UI: SolidJS S1 渲染器   ──→  轻量 web view 渲染
```

**我们不是 fork Open Science——我们是把它的精华（技能、连接器、MCP、UI）嫁接到我们自己的 Rust 底座上。底座不做任何妥协：ACP 协议、durable 证据链、生产级权限桥、L4 端到端验证——这些是 Lumen 独有的，Open Science 没有。**

---

## 5. 已冻结架构决策（2026-07-23）

1. **Notebook：Python only。** 先用受限、可重启、可审计的 Python 内核；R 与 Node REPL 不在首个生产闭环中引入额外包管理器、持久状态和供应链。
2. **UI：轻量 WebView renderer。** 数据和决策仍由 Rust Lumen 持有；renderer 只消费已注册 artifact，绝不成为第二个状态或执行权威。
3. **顺序：连接器 → Artifacts MCP → Python notebook → Reviewer/HTTP bridge → skills/UI → 集成测试。** 这先建立可验证的数据与工件边界，再接入计算与呈现。
4. **许可证：GPL 永久拒绝；未知许可证和受限数据库默认拒绝。** 只有精确路径、版本、许可证、数据条款、依赖和 fixture 证据都齐全的资产才能单项准入。
5. **Motif：作为可选 MIT renderer 候选。** 在其仓库身份、MIT 文件、依赖和浏览器隔离逐项验证前不自动合入或执行。

这些选择优先保证科学结果可追溯、离线回放和安全边界，而不是最大化功能数量。
