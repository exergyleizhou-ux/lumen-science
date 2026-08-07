# G1 macOS 正式发布 — 交接清单（唯一剩余 gate，10 分 → 100/100）

> 状态：**BLOCKED_ON_CREDENTIALS**（2026-08-07 核验）。G1 的全部工程前置已闭合；
> 唯一缺失 = Apple Developer 签名证书 + 公证凭据（用户侧资产，机器上不存在，无法代取）。

## 已闭合的工程前置（2026-08-07 实测）

| 前置 | 状态 | 证据 |
|------|------|------|
| SINGLE_BASE | ✅ PASS | `scripts/verify-single-base.py` → zero-copy，product tests 消费 pinned Lumen a695dec5（v2.3.1 source） |
| lumen NG10 release foundation | ✅ PASS | v2.3.1 已发布：签名 tag + 20 资产 + minisig + SPDX SBOM（`scripts/release.sh` 两段事务） |
| Desktop 全套（sender identity、ACP registry、project/evidence/preview/review/notebook/skills/compute UI） | ✅ PASS | desktop-ci `Desktop macOS full package` + `Desktop headed E2E` + `Desktop live E2E (real engine)` + `Desktop authority suite` 全绿（最新 head 78b44ac） |
| W 能力 E4 | ✅ PASS | science-ci built-binary 权威证明（offline Science proofs） |
| UPDATER_TRUST（fail-closed） | ✅ 满足 | electron-builder 显式 `publish: omitted`，auto-update DISABLED——无信任 feed 时 G1 按 fail-closed 出货，不引第三方源 |

## 硬阻塞：缺失的凭据（本机实测）

```
security find-identity -v -p codesigning   → 0 valid identities
xcodebuild -version                        → 无 Xcode（仅 CommandLineTools）
~/Library/MobileDevice/Provisioning Profiles → 空
xcrun notarytool                            → 无存储凭据；env 无 APPLE_*/NOTARY_*
electron-builder.yml                        → notarize: false（等待 org certs）
```

**需要用户提供（任选其一组合）：**

1. **Developer ID Application 证书**（`Developer ID Application: <org> (<TEAMID>)`，.p12 + 密码，或已在登录钥匙串）
2. **notarytool 凭据**：
   - App Store Connect API key（`.p8` + key id + issuer id）— 推荐，可写文件
   - 或 Apple ID + App 专用密码 + Team ID（交互式）
3. 建议同时装 Xcode（`xcode-select --install` 不够；签名公证可用 CLT，但完整验证建议 Xcode）

## 凭据就绪后的一键路径（无需再设计）

```bash
# 1) 导入证书（若 .p12）
security import DeveloperID.p12 -k ~/Library/Keychains/login.keychain-db

# 2) 配置签名环境（写入 ~/.zshrc 或 CI secret）
export CSC_LINK=/path/to/DeveloperID.p12
export CSC_KEY_PASSWORD=***
export APPLE_ID=***            # notarytool（或 API key: APPLE_API_KEY/APPLE_API_ISSUER/APPLE_API_KEY_PATH）
export APPLE_TEAM_ID=***

# 3) 打开签名 + 公证（改 electron-builder.yml 一处 + 构建）
#    mac.notarize: true；CI 里注入 CSC_LINK 后 electron-builder 自动签名+公证
cd packs/science-desktop
npm run build:mac   # → lumen-science-desktop-<ver>-mac-arm64.dmg + .zip（公证后）

# 4) 验证公证
xcrun stapler validate dist/*.dmg
spctl -a -vv -t install dist/*.app

# 5) 上传发布（GitHub Release / 自建 feed），更新 BOARD G1 → ✅ 10
```

## 完成标准（DoD）

- [ ] DMG/ZIP 经 `spctl` 通过（Developer ID 有效）
- [ ] `xcrun stapler validate` 通过（已公证，Gatekeeper 无警告）
- [ ] 安装后 launch + live-engine E2E 通过（桌面连 pinned lumen 引擎）
- [ ] 发布物附 SHA256SUMS + SBOM（复用 lumen 发布纪律）
- [ ] BOARD G1 行 → ✅ 10，总进度 100/100
