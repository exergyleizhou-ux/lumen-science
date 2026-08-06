# Lumen Science 看板（NextGen 执行）

> **更新规则：** 里程碑时更新（gate 翻转/进度变化）。无心跳 monitor。
> 上次刷新：**2026-08-06**（北京时间）— I1-B 九源全量采纳，67/100
> 权威进度：`docs/science/5.0/LUMEN_SCIENCE_NEXTGEN_CANONICAL_EXECUTION_BOOK_2026-08-06.md` §40；gate 真值：`docs/science/5.0/NEXTGEN_GATE_REGISTRY_V2.json`

---

## 总览

| 项 | 完成度 | 状态 |
|----|--------|------|
| **正式进度（书 §40 九项）** | **67 / 100** | S0 10 + L0 12 + C1 12 + C2–C7 15 + S2 10 + I1 8 |
| 机器门 | 22 项 | 全绿（exit 0） |
| PR #28 CI | 全绿（历次 head） | 最新轮验证中 |

## 九项 gate

| 顶层 gate | 权重 | 得分 | 状态 |
|---|---:|---:|---|
| S0 两条 P0 修干净 | 10 | ✅ 10 | PR #28 全绿 + skill 直写封死 |
| L0 底座可用 | 12 | ✅ 12 | lumen 2.2 tuple 核验 |
| C1 公共接口正式化 | 12 | ✅ 12 | 7 方法目录 + compat manifest + 真实引擎负例 |
| C2–C7 安全治理底座 | 15 | ✅ 15 | 15 个治理 gate 核验 PASS（V0） |
| S2 三层科研 Agent 证明 | 10 | ✅ 10 | S2a 五类场景语料全绿（pin binary） |
| I1 九源审查 | 8 | ✅ 8 | 九源全量采纳（资产方授权）；upstream-lock active；rights gate 9/9 |
| K1 长期任务运行 | 10 | ⏳ 0 | K1b 依赖 X-M1 融合 |
| M1 单底座 | 13 | ⏳ 0 | parity 冻结 + push-up 分支已备；store 融合待 lumen 协作 |
| G1 macOS 正式发布 | 10 | ⏳ 0 | **硬 blocker：无 Apple 签名证书/Xcode/公证凭据** |

## 里程碑

| 里程碑 | 内容 | 状态 |
|--------|------|------|
| A 不再漏水 | S0-A/S0-B/F0-1/F0-2/V0/I1-A | ✅ 完成 |
| B 第一条单底座能力 | X-C1 + X-S1（parity 冻结 + push-up 分支） | 🔄 X-S1 已 merge（lumen main 0eb52c10）；science crate 族 zero-drift（94/94 identical，冻结策略）；剩全 agent/ 删复制 |
| C 受控科研 Agent 产品面 | S2a corpus + W0-A catalog | ✅ corpus/catalog 完成；W 产品切片待 API |
| D 可长期使用的 Science 产品 | X-M1 完成 + W 波 + G1 | ⏸ 阻塞（G1 凭据；X-M1 跨仓） |

## 阻塞与外部依赖

- **G1**：Apple Developer 证书 + Xcode + notarytool 凭据（用户侧）。
- **X-M1**：推上去已完成（lumen PR #135，`science/xs1-store-seq-v1`，byte parity + 603 tests 绿）；剩 PR merge + science 侧删复制/cutover。
- ~~I1-B~~：已闭合（9/9 全量采纳 + active lock 晋升，2026-08-06）。
- **X-U**：lumen pin 已刷新 dc563b1e → 48e188e5（v2.2.0 后 main head），drift manifest 如实重算 1388。
- **K1b**：依赖 X-M1 后的 canonical kairos 接线。

## 证据

- SCRATCH 日志：v0-receipts / xc1 / xm1-parity / i1-intake / s2a-w / final-ci
- 机器门：`bash scripts/science-machine-gates.sh`（22 GATE OK）
- 看板历史：本文件 2026-07-20 版为旧 lumen 开发线（A/B/C），已由本文档取代。

## 一句话

**67/100：漏水全止、底座核验、接口正式化、九源全量采纳、影子场景、能力目录全部落地；剩余 33 分卡在 Apple 凭据与 lumen 跨仓融合（K1b/M1/G1）上。**
