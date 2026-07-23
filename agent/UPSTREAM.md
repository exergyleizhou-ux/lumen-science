# Upstream pin — 下次对标从这里开始

## 当前登记（2026-07-20 · Lumen v0.1.222）

| 侧 | 字段 | 值 |
|----|------|-----|
| **Lumen 产品版本** | `VERSION` / tag | **0.1.222** |
| **Lumen main tip** | git | `5371919` (`537191998c959b66b41f6a057dd3d3bdf16a9eb6`) |
| **Lumen 仓库** | origin | `exergyleizhou-ux/lumen` |
| **上游** | remote | `https://github.com/xai-org/grok-build.git` (`upstream`) |
| **上游对标 tip（已吸收基线）** | `upstream/main` @ 登记时 | **`ba76b0a`** = `ba76b0a683fa52e4e60685017b85905451be17bc` |
| **上游 monorepo SOURCE_REV** | 该 tip 内文件 | `ba69d70c2f7d70a130a323b2becdf137af784c7f` |
| **上游产品版本号** | `xai-grok-version` / pager crate | **0.2.106** |
| **路径映射** | 上游 → 我们 | `crates/...` → `agent/crates/...` |
| **政策** | | **PINNED** · 安全/正确性 **点状 cherry** · **禁止** 全量 merge |

### 下次「对照最新 grok-build 辩证吸收」起手式

```bash
cd /Users/lei/code/lumen
git fetch origin && git checkout main && git pull --ff-only origin main
git fetch upstream
# 基线：我们已对齐到 ba76b0a / 上游 0.2.106
# 新 tip：
NEW=$(git rev-parse upstream/main)
OLD=ba76b0a683fa52e4e60685017b85905451be17bc
git log --oneline $OLD..$NEW | head -50
# 只对安全/正确性文件做 diff，不要整树 merge
git diff $OLD..$NEW --stat -- crates/codegen/xai-grok-shell crates/codegen/xai-grok-pager-render crates/codegen/xai-grok-auth crates/codegen/xai-grok-mcp | head -80
```

吸收后：更新本文件「当前登记」的 tip/版本行 + cherry 表；bump Lumen VERSION；**勿**改下方铁律区。

---

## 政策（铁律）

- Source: **xai-org/grok-build**
- 初始快照导入：`~/Desktop/grok-build-main`（2026-07-16）
- **禁止** `git merge upstream/main` 或整树覆盖
- **禁止** 覆盖 Lumen 优势区：
  - Expert dual / merge / budget / consultant tools
  - `lumen-guard` / `lumen-discipline`
  - DeepSeek 默认与 smoke / `scripts/assert-defaults.sh`
  - `default_models.json` 与产品默认模型（不要被上游 Grok 默认带跑）
- **只取**：安全 / 正确性 / 小 alias / 竞态修复
- **拒**：hooks 大包、pager 重写、catalog 整表替换、默认模型变更

---

## Cherry-picks applied（辩证 · 相对 ba76b0a / 0.2.106）

| Date | Upstream area | What we took | What we refused | Lumen 合入 |
|------|---------------|--------------|-----------------|------------|
| 2026-07-20 | `dispatch_locks` cancel/prompt race | rename + cancel 持锁 + 回归测 | hooks/pager/catalog/Expert | #127 `f29bd2e` |
| 2026-07-20 | OSC 52 kill switch | `osc52_disabled` + GROK/LUMEN env | pager rewrite | #127 |
| 2026-07-20 | `/summarize` pager alias | `RecapCommand::aliases = ["summarize"]` | 激进 compaction | #128 `f16d27a` |
| 2026-07-20 | marketplace `require_sha` | pin gate + env + UnpinnedRemoteRefused | 全套 clone/install 签名重写 | #128 |
| 2026-07-20 | auth recovery survey | **SKIP** — 与 pin 一致 | OAuth 大包 | — |

---

## How to port more later

1. `git fetch upstream`；对比 **`ba76b0a..upstream/main`**（或本文件登记的最新 tip）
2. Diff **单文件/单模块**，不整树
3. 手工 port 到 `agent/crates/...`
4. Expert / guard / DeepSeek / defaults **不动**
5. focused 测 + CI Expert gate 绿再合
6. 更新本文件 tip 行 + cherry 表 + `SOURCE_LOCK.json` 的 `upstream_pin` 字段
