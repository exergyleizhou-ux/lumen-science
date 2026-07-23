# 12 — FINAL-5UX 差距表（对照 `21ef079`）

> **目标规格：** [11-FINAL-5UX-目标态规格.md](./11-FINAL-5UX-目标态规格.md)  
> **（桌面同源：）** `/Users/lei/Desktop/FINAL-5UX-2026-07-17.md`  
> **代码基线：** `/Users/lei/code/lumen` @ `21ef079`（2026-07-17 记录）  
> **工程 readiness：** `engineering_complete=true`；`ready=false`（M5+M6 人类门）  
> **用途：** 给 Codex / 实现者做拆解与二次验收，不替代 11 号全文规格。

---

## 0. 关系澄清

| 文档/事实 | 角色 |
|-----------|------|
| FINAL-2.0 工程门禁 | 机器验收已基本绿；挡 READY 的是 M5/M6 |
| FINAL-5UX | **TUI 目标态 UX**；在 Grok 底盘上做身份/真相/迁移，**非第二套 TUI** |
| 当前用户体感「不像 Grok / 也不像成品 Lumen」 | 预期内：仍是 Grok shell + 半套 Lumen 文案 + 共享 `~/.grok` |
| 完成 FINAL-5UX 实现 | **仍不能**用文档或 mock 顶替 M5/M6 真人证据 |

---

## 1. 总览（按 FINAL-5UX Gate）

| Gate | 主题 | 对照 21ef079 | 状态 |
|------|------|--------------|------|
| **A** | UI contract / TruthSnapshot / 不变量测试 | `ui_contract.rs`（`e301b99`） | ✅ |
| **B** | Lumen 身份 + LUMEN_HOME + legacy 迁移 | `a4412ec`：`lumen_home`/`migration`/title/welcome/auth | ✅ 核心完成 |
| **C** | Provider / probe / trust 旅程 | CLI `probe-local` + **`truth_assembly` 数据层**；TUI wizard 仍未做 | 🟡 数据 ✅ / UI 待 Codex |
| **D** | Truth bar / phase / verify freshness | agent_status 等有片段；无 truth bar 合同 | ❌ 未达标 |
| **E** | 键盘可达 / 对比度 / motion off | 部分 Grok 能力；welcome/键盘 parity 未按 5UX 验收 | 🟡 未知/未证 |
| **F** | Cutover 删旧壳 + PTY matrix + M5 | 未 cutover；M5 仅模板+gate | ❌ |

**一句话：** 工程与模型目录已较完整；**FINAL-5UX 几乎仍是规格，不是已上线体验。**

---

## 2. P0 差距（规格：任一未完不能切换）

| P0 项（FINAL-5UX §20） | 目标 | 21ef079 现状 | 建议落点（§18） |
|------------------------|------|--------------|----------------|
| 单一 Lumen identity | 主 chrome 100% Lumen | CLI help 已 Lumen；TUI/welcome/login 仍大量 Grok Build / Sign in to Grok 基因 | `welcome/*`、`cli.rs`、`title.rs`、minimal `auth.rs` |
| 真实 release channel | 显示真实版本如 `0.1.220-alpha.4`，禁硬编码 Beta | 版本有；需核对 UI 是否用真实 channel | version / about / header |
| Terminal title | `Lumen · {repo} · {phase}` | 需核对是否仍 grok fallback | `notifications/title.rs` |
| LUMEN_HOME + 无损迁移 | 默认 `~/.lumen`；从 `~/.grok` preview/拒绝/恢复 | 配置路径仍偏 GROK_HOME/`~/.grok`；与官方 grok 串配置 | `xai-grok-config` `paths.rs` + 新 `migration.rs` |
| Provider-scoped auth | DeepSeek 首启不经 xAI/Grok login | BYOK 路径存在；用户机 `~/.grok` 仍偏 grok-4.5 + OIDC 体验 | minimal auth + startup |
| Capability probe → Tool-ready | TUI 内真实 tool-call + fingerprint | `scripts/probe-local.sh` 在 CLI；**无** session 级 Tool-ready 合同 UI | 新 readiness 旅程 + probe 接入 shell |
| TruthSnapshot + truth bar | 五类事实 + source/freshness | 无统一 snapshot；无 truth_bar 模块 | 新 `ui_contract.rs`、`truth_bar.rs` |
| Verification freshness | edit 后 Verified 立即 stale | verify-after-edit 有工程能力；UI 新鲜度合同未锁 | agent_status + verify 事件 → snapshot |
| Permission why/risk/scope | 非模糊 Safe；hard-deny 真相 | permission 体系有；文案/展示未按 5UX | `permission_view.rs` |
| Welcome keyboard parity | Enter/focus 等 | 未按 5UX 矩阵证明 | welcome + app_view |
| M5 可执行 | ≤10 min 真人路径+证据 | 模板+`onboarding-gate.sh`；**证据 missing** | 真人 + gate 通过 |

---

## 3. P1 差距（与 FINAL-5UX 同批）

| 项 | 现状 |
|----|------|
| Cache source/freshness（无数据不显示 0%） | sampler 可能有 usage；UI 合同未强制 |
| Phase grouping + progressive disclosure | 部分 transcript 折叠能力；未统一 phase 语义 |
| Dashboard 共用 snapshot / 排序优先级 | dashboard 存在；未绑定 TruthSnapshot |
| minimal ↔ fullscreen 语义一致 | 双 renderer 仍在；parity 未按 5UX 验收 |
| 80/120/180 × color matrix 快照 | 无 FINAL-5UX 专项 PTY matrix |
| reduced motion / idle redraw = 0 | 规格要求 default motion=off；需查 welcome logo shimmer 是否仍在 |
| Recovery + redacted `/status` report | 部分 recovery；无 status_detail 规格实现 |

---

## 4. 已有、可复用（不要重做）

| 资产 | 路径 | 说明 |
|------|------|------|
| 工程 readiness | `artifacts/readiness/status.json` | eng complete；勿改假 ready |
| 多模型 29 | `default_models.json` | DeepSeek 默认；provider 数据已有 |
| 本地 probe CLI | `scripts/probe-local.sh` | 接入 TUI Tool-ready 的证据源候选 |
| Onboarding 模板/gate | `docs/user/10-minute-onboarding-evidence.md`，`scripts/onboarding-gate.sh` | M5 证据格式 |
| 安全/verify | `lumen-guard`，`lumen-verify`，smoke-security | hard-deny / 验证后端 |
| Go→Rust 落点 | `10-旧Go到新Rust模块落点.md` | 架构边界 |
| 安装 | `scripts/install-local.sh` | 版本戳应对 HEAD |

---

## 5. 双客户端问题（实现时必须处理）

| | 官方 | Lumen |
|--|------|-------|
| 二进制 | `~/.grok/bin/grok` 0.2.101 | `~/.local/bin/lumen` 0.1.220-alpha |
| 配置 | `~/.grok/config.toml`（常 default=grok-4.5） | 现状也读 `~/.grok` → **串味** |
| FINAL-5UX | — | 默认 **`~/.lumen` / LUMEN_HOME** + legacy dry-run 导入 |

验收：空 `LUMEN_HOME` + DeepSeek key 首启 **不得**弹出「Sign in to Grok」主路径。

---

## 6. 建议实现顺序（对齐规格 §19，给 Codex）

1. **Gate A** — `ui_contract.rs` + 单测/property（不发布 UI）  
2. **Gate B** — identity + LUMEN_HOME + migration dry-run/receipt  
3. **Gate C** — startup：provider → probe → Tool-ready / chat-only  
4. **Gate D** — truth bar + verify freshness + dashboard 同源  
5. **Gate E** — keyboard / 80×24 / NO_COLOR / motion  
6. **Gate F** — 删旧 product shell 文案路径 + PTY + **真人 M5**  

禁止：永久 old/new shell flag；第二套 TUI；伪造 M5/M6。

---

## 7. 验收命令（规格 §25 + 仓库现状）

```bash
cd /Users/lei/code/lumen
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

# 身份：主 chrome 不应再以产品名推销 Grok（allowlist 见 11 号文档）
rg -n 'Grok Build|Sign in to Grok|Run Grok' agent/crates/codegen/xai-grok-pager agent/crates/codegen/xai-grok-pager-minimal || true

./scripts/assert-defaults.sh
./scripts/smoke-security.sh
./scripts/probe-local.sh --list 2>/dev/null || ./scripts/probe-local.sh | head -20
./scripts/onboarding-gate.sh   # 期望：真人证据前仍 fail
./scripts/productivity-gate.sh # 期望：日记不足仍 fail
# 实现过程中增加：
# cargo test -p xai-grok-pager -p xai-grok-pager-minimal -p xai-grok-config
```

Readiness 诚实性：

```bash
python3 - <<'PY'
import json
s=json.load(open("artifacts/readiness/status.json"))
assert s.get("engineering_complete") is True
assert s.get("ready") is False
print("blockers:", s.get("blockers"))
PY
```

---

## 8. Definition of Done 勾选（对照 11 号 §23，基线全空）

实现前基线（21ef079）：下列 **均未**因 FINAL-5UX 而勾选完成：

- [ ] Primary chrome 产品名 100% Lumen  
- [ ] Grok 仅 allowlist  
- [ ] 默认配置 `~/.lumen` / LUMEN_HOME  
- [ ] legacy migration preview/拒绝/恢复  
- [ ] DeepSeek first-run 无 xAI auth gate  
- [ ] Tool-ready 真 probe + fingerprint  
- [ ] Chat-only 不能冒充 agent-ready  
- [ ] Truth bar 五类事实 + freshness  
- [ ] Verified 对 current files + edit→stale  
- [ ] Permission why/risk/scope  
- [ ] 键盘等价 + 80×24 + color 降级 + motion  
- [ ] 恢复路径 + redacted `/status`  
- [ ] 性能预算 / PTY matrix  
- [ ] **M5 真人证据**  
- [ ] **M6 不伪造；未过则 BLOCKED**  

（工程侧 M5/M6 **门禁脚本**已存在；**证据**未过。）

---

## 9. 给 Codex 的最小开工包

**必读：**

1. `docs/masterplan/11-FINAL-5UX-目标态规格.md`（全文）  
2. 本文 `12-FINAL-5UX-差距表-vs-21ef079.md`  
3. `09-FINAL-2.0-执行路径.md`（工程边界）  
4. `10-旧Go到新Rust模块落点.md`（勿重写 agent loop）  

**首 PR 建议：** Gate A only — `ui_contract.rs` + tests，不切 UI。  
**禁止：** 改 `ready=true`、伪造 journal/onboarding。

---

## 10. 一句话

FINAL-5UX 已入库为 **11**；与 **21ef079** 的差距是 **整段 UX 合同未落地**（身份家、Truth bar、TUI probe、迁移），工程 readiness 已绿不能代替。下一步按 Gate A→F 拆 PR；M5/M6 仍靠真人。
