# GUARD_RULES — Lumen L0–L3（与代码同步）

> **实现：** `agent/crates/codegen/lumen-guard`  
> **接线：** `xai-grok-workspace` permission manager（**YOLO / bypass 之前**）  
> **规格源：** `docs/masterplan/02-安全规格-Lumen基因.md` + `~/lumen/internal/guard`  
> **原则：deny 在所有模式生效（含 always-approve / YOLO）。**

---

## 防御纵深

```
tool 请求
  → Lumen L0–L3 (lumen-guard)     ← 硬 deny，不可绕过
  → managed policy deny/ask
  → YOLO / session grants / classifier
  → OS sandbox (叠加，非替代)
  → 执行
```

---

## 层对照

| 层 | 名称 | 入口 | 例子 |
|----|------|------|------|
| L0 | 零宽剥离 | `strip_hidden_chars` | `rm\u200b -rf /` 仍命中 |
| L1 | 外泄 | `check_bash` exfil | `curl -d @.env …` |
| L2 | 敏感读 | sensitive reads | `cat ~/.ssh/id_rsa` |
| L2 | 侦察 | recon | `ps aux`, `netstat -antp` |
| L2 | 破坏 | destructive + rm | `rm -rf /`, home 整目录 |
| L2 | 编码执行 | encoded | `base64 -d \| sh` |
| L2 | 下载执行 | pipe-to-shell | `curl … \| bash` |
| L2 | 分段 | `&&` `\|\|` `;` `\|` | `echo ok && rm -rf /` |
| L3 | 写路径 | `check_write_path` | `~/.ssh/authorized_keys`, `/etc/*`, shell rc |

---

## 验收

```bash
./scripts/smoke-security.sh
```

必须覆盖 masterplan 最低 6 例：

1. `rm -rf /`
2. `curl … | bash`
3. `cat $HOME/.ssh/id_rsa`
4. 写 `~/.ssh/authorized_keys`
5. 带 ZWSP 的破坏命令
6. `echo ok && rm -rf /`

---

## 非目标（本阶段）

- 替代 Grok OS sandbox
- 替代用户自定义 deny 规则（叠加）
- Guardian LLM 审查（v1.1+）
