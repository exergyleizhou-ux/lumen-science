# 10 分钟新手路径：证据清单

目标不是写一份“看起来完成”的教程，而是让一名第一次使用 Lumen 的人，在 10 分钟内
完成：安装当前提交 → 配置一个真实 key → 让 agent 用工具改文件 → 看到验证/安全反馈。

每次 private-beta 测试复制本页到 `SCRATCH/onboarding-<时间>/checklist.md` 再填写。
`SCRATCH/` 不进 git，避免录屏、终端日志或环境信息误入仓库；最终只提交脱敏结论。

## 测试元数据（先填）

- 测试人：
- 日期 / 时区：
- macOS / 终端：
- Lumen commit：
- 使用模型：
- 计时开始：
- 计时结束：
- 录像或截图目录：

## 0:00–2:30 安装当前提交

```bash
cd ~/code/lumen
mkdir -p SCRATCH/onboarding
./scripts/install-local.sh 2>&1 | tee SCRATCH/onboarding/install.log

expected="$(git rev-parse --short HEAD)"
lumen --version | tee SCRATCH/onboarding/version.txt
lumen --version | grep -F "($expected)"
```

- [ ] 安装命令退出 0
- [ ] 版本括号里的 commit 等于当前 `HEAD`，不是旧构建
- [ ] `install.log` 有 `binary_sha256=`，不含 key
- 截图 / 时间：

## 2:30–4:00 配置 key（不记录值）

选择一个实际可用的模型，只把 key 放环境变量：

```bash
export DEEPSEEK_API_KEY='...'
test -n "$DEEPSEEK_API_KEY" && echo 'DEEPSEEK_API_KEY present (value hidden)'
```

- [ ] 终端、录像和提交内容没有显示 key 值
- [ ] 使用本地模型时先跑 `scripts/probe-local.sh`，且 `can_tool_call=true`
- 截图 / 时间：

## 4:00–8:30 真实 read → edit → verify

在一次性目录里创建一个确定失败的测试，再要求 Lumen 读取、修改并复跑：

```bash
work="$(mktemp -d)"
cd "$work"
git init -q
printf 'def add(a, b):\n    return a - b\n' > calc.py
printf 'from calc import add\nassert add(2, 3) == 5\n' > test_calc.py
python3 test_calc.py || true

lumen -m deepseek-v4-pro --single \
  "读取 calc.py 和 test_calc.py，修复实现，然后实际运行 python3 test_calc.py 验证。" \
  --always-approve --max-turns 8 \
  2>&1 | tee ~/code/lumen/SCRATCH/onboarding/agent.log

python3 test_calc.py
git diff --no-index /dev/null calc.py || true
```

- [ ] 失败测试在 Lumen 前真实失败
- [ ] `agent.log` 有结构化工具调用/命令证据，不是只回复代码块
- [ ] 文件确实改变，最终测试退出 0
- [ ] 若模型只输出散文，本项记 FAIL，不能手改后补绿
- 截图 / 时间：

## 8:30–10:00 安全反馈与结论

```bash
cd ~/code/lumen
./scripts/smoke-security.sh 2>&1 | tee SCRATCH/onboarding/security.log
```

- [ ] hard-deny 测试退出 0
- [ ] 新手能用一句话解释“为什么某个危险操作被拦”
- [ ] 总耗时 ≤10 分钟
- [ ] 想切走到其它 agent 的次数与原因已记录

## 结论（测试人填写）

- 结果：PASS / FAIL
- 首个阻塞点：
- 最困惑的一步：
- 是否愿意把它作为下一次真实 coding task 的主用工具：是 / 否
- 需要修复后重测的事项：

未填写的 checkbox、缺失的录像/日志、无真实 key 或无真实工具调用，都只能记为“模板已备”，
不能记作 M5/M6 真人证据。
