// Package research seeds and verifies the full Claude Science research asset
// stack: bio-tools MCP fleet (all domains/databases), ketcher-chemistry, skills,
// seed examples, and conda/python runtimes — cloned from ~/.claude-science.
package research

// Skill describes a bundled Science skill under runtime/skills/.
type Skill struct {
	ID          string `json:"id"`
	Label       string `json:"label"`
	Category    string `json:"category"`
	Description string `json:"description,omitempty"`
}

// Domain describes one bio-tools MCP domain from mcp_bio/domains.json.
type Domain struct {
	Slug       string   `json:"slug"`
	Label      string   `json:"label"`
	BundledID  string   `json:"bundled_id"`
	ToolCount  int      `json:"tool_count,omitempty"`
	Highlights []string `json:"highlights,omitempty"`
}

// MCPServer is a top-level MCP server directory under runtime/mcp-servers/.
type MCPServer struct {
	ID          string `json:"id"`
	Label       string `json:"label"`
	Description string `json:"description,omitempty"`
}

// WorkspaceDirs are _mcp-* stubs on real Science installs (all research domains).
var WorkspaceDirs = []string{
	"_mcp-biomart",
	"_mcp-cancer-models",
	"_mcp-cellguide",
	"_mcp-chemistry",
	"_mcp-clinical-genomics",
	"_mcp-drug-regulatory",
	"_mcp-expression",
	"_mcp-genes-ontologies",
	"_mcp-genomes",
	"_mcp-human-genetics",
	"_mcp-ketcher-chemistry",
	"_mcp-literature",
	"_mcp-omics-archives",
	"_mcp-protein-annotation",
	"_mcp-regulation",
	"_mcp-research-resources",
	"_mcp-rna",
	"_mcp-structures-interactions",
	"_mcp-variants",
	"_mcp-zinc",
}

// BundledMCPIDs — all bundled connectors Science registers (incl. split bio domains).
var BundledMCPIDs = []string{
	"bundled:bio",
	"bundled:biomart",
	"bundled:pubmed",
	"bundled:clinical-trials",
	"bundled:chembl",
	"bundled:biorxiv",
	"bundled:npi-registry",
	"bundled:cms-coverage",
	"bundled:variants",
	"bundled:clinical-genomics",
	"bundled:expression",
	"bundled:regulation",
	"bundled:protein-annotation",
	"bundled:rna",
	"bundled:structures-interactions",
	"bundled:omics-archives",
	"bundled:genes-ontologies",
	"bundled:drug-regulatory",
	"bundled:research-resources",
	"bundled:cellguide",
	"bundled:cancer-models",
	"bundled:icd-10-codes",
	"bundled:chemistry",
	"bundled:genomes",
	"bundled:human-genetics",
	"bundled:literature",
	"bundled:zinc",
	"bundled:ketcher-chemistry",
}

// TopLevelMCPServers shipped beside bio-tools.
var TopLevelMCPServers = []MCPServer{
	{ID: "bio-tools", Label: "Bio-tools 数据库舰队", Description: "87+ 公共数据库客户端，23 个领域 ~247 工具"},
	{ID: "ketcher-chemistry", Label: "Ketcher 化学结构编辑器", Description: "交互式 2D 分子结构与反应式编辑"},
}

// KnownSkills is the full Science skill roster (runtime/skills); discovery fills gaps at runtime.
var KnownSkills = []Skill{
	{ID: "alphafold2", Label: "AlphaFold2", Category: "结构预测"},
	{ID: "boltz", Label: "Boltz-2", Category: "结构预测"},
	{ID: "borzoi", Label: "Borzoi", Category: "基因组功能轨迹"},
	{ID: "chai1", Label: "Chai-1", Category: "结构预测"},
	{ID: "diffdock", Label: "DiffDock", Category: "分子对接"},
	{ID: "esmfold2", Label: "ESMFold2", Category: "结构预测"},
	{ID: "evo2", Label: "Evo2", Category: "DNA 基础模型"},
	{ID: "fair-esm2", Label: "ESM-2", Category: "蛋白嵌入"},
	{ID: "ligandmpnn", Label: "LigandMPNN", Category: "蛋白设计"},
	{ID: "proteinmpnn", Label: "ProteinMPNN", Category: "蛋白设计"},
	{ID: "solublempnn", Label: "SolubleMPNN", Category: "蛋白设计"},
	{ID: "openfold3", Label: "OpenFold3", Category: "结构预测"},
	{ID: "scgpt", Label: "scGPT", Category: "单细胞"},
	{ID: "scvi-tools", Label: "scVI-tools", Category: "单细胞"},
	{ID: "literature-review", Label: "文献综述", Category: "科研写作"},
	{ID: "pdf-explore", Label: "PDF 深度解析", Category: "文献"},
	{ID: "paper-narrative", Label: "论文叙事", Category: "科研写作"},
	{ID: "figure-composer", Label: "组图排版", Category: "可视化"},
	{ID: "figure-style", Label: "图表规范", Category: "可视化"},
	{ID: "indication-dossier", Label: "适应症档案", Category: "药物研发"},
	{ID: "compute-env-setup", Label: "计算环境搭建", Category: "基础设施"},
	{ID: "remote-compute-ssh", Label: "SSH/SLURM 远程计算", Category: "基础设施"},
	{ID: "remote-compute-modal", Label: "Modal 远程计算", Category: "基础设施"},
	{ID: "managed-model-endpoints", Label: "模型端点注册", Category: "基础设施"},
	{ID: "using-model-endpoint", Label: "调用模型端点", Category: "基础设施"},
	{ID: "customize", Label: "Agent/Skill 定制", Category: "平台"},
	{ID: "skill-creator", Label: "Skill 创作器", Category: "平台"},
	{ID: "self-awareness", Label: "会话自省", Category: "平台"},
	{ID: "product-self-knowledge", Label: "产品知识库", Category: "平台"},
}

// SeedExamples are bundled example research projects in runtime/seed + seed-assets.
var SeedExamples = []string{
	"example_crispr_screen",
	"example_enzyme_engineering",
	"example_extremophile",
	"example_immunotherapy",
}

// DomainLabels maps domain slugs to Chinese labels (23-domain partition).
var DomainLabels = map[string]string{
	"pubmed":                  "PubMed 文献",
	"clinical-trials":         "临床试验",
	"chembl":                  "ChEMBL 药物",
	"biorxiv":                 "bioRxiv 预印本",
	"biomart":                 "BioMart",
	"genomes":                 "基因组 Ensembl/UCSC",
	"human-genetics":          "人类遗传学 GWAS/eQTL",
	"variants":                "变异 ClinVar/gnomAD",
	"clinical-genomics":       "临床基因组",
	"expression":              "表达谱 GTEx",
	"genes-ontologies":        "基因本体 GO/KEGG",
	"protein-annotation":      "蛋白注释",
	"rna":                     "RNA Rfam",
	"structures-interactions": "结构蛋白互作",
	"omics-archives":          "组学存档 GEO/PRIDE",
	"cancer-models":           "癌症模型",
	"regulation":              "调控 ENCODE/JASPAR",
	"chemistry":               "化学 PubChem/ChEBI",
	"drug-regulatory":         "药监 FDA",
	"research-resources":      "科研资源 抗体/基金",
	"cellguide":               "单细胞 CellGuide",
	"literature":              "文献 OpenAlex/arXiv",
	"zinc":                    "化合物库 ZINC",
	"npi-registry":            "NPI 医疗提供者",
	"cms-coverage":            "CMS 医保覆盖",
	"icd-10-codes":            "ICD-10 编码",
}

// CloneAssets are top-level dirs cloned from real ~/.claude-science into sandbox.
var CloneAssets = []string{"bin", "conda", "runtime", "seed-assets"}

// SkipMCPApprovalIDs returns bundled server IDs to auto-approve in virtual sandbox.
func SkipMCPApprovalIDs() []string {
	return append([]string(nil), BundledMCPIDs...)
}
