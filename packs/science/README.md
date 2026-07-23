# Lumen Science Pack

Science private-beta 默认垂直。仓库保留了旧 Go Lumen 的 proxy、Lab、MCP、native brief
等源代码资产；它们仍引用旧仓共享的 `lumen/internal/*` 包，不能把“代码已在目录里”
冒充“整个旧 Science 控制面已成为独立二进制”。

当前可独立复跑的产品路径是一个无第三方 Go 依赖的 provenance-first research brief：
PubMed 是必须成功的证据源，ChEMBL 是可降级且会显式写 warning 的补充源。它只汇总
来源元数据，不生成医学结论。

## 三步 dogfood

```bash
# 1. 构建独立入口
cd packs/science
go build -C standalone -o ../lumen-science ./cmd/science

# 2. 只读诊断（不写 key、不启动服务）
./lumen-science doctor

# 3. 真实网络路径：生成带 PMID / ChEMBL 链接和时间戳的 brief
./lumen-science brief --out ../../SCRATCH/science-aspirin.md aspirin
```

验收时打开 `SCRATCH/science-aspirin.md`，逐条点击 PubMed / ChEMBL 来源；网络失败时
命令必须非零退出或写出明确 warning，不能生成假成功证据。

## 开发验证

```bash
go test -C standalone ./...
go vet -C standalone ./...

# 一次完成三步并把 live 证据写入 gitignored SCRATCH/
../../scripts/dogfood-science.sh aspirin
```

`standalone/` 单独成 Go module，是为了让这条 private-beta 路径真实可构建，同时不假装
已经完成旧仓所有共享包的迁移。`proxy/`、`lab/`、`native/` 等目录目前仍是后续迁移的
知识/代码资产；完整 GUI、Claude Science sandbox 和全 MCP fleet 不属于这三步已完成宣称。
