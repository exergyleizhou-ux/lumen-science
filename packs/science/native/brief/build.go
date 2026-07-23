// Package brief synthesizes provenance-linked research briefs from native MCP tools.
package brief

import (
	"fmt"
	"strings"
	"time"
)

// Claim is one evidence-backed statement in a brief.
type Claim struct {
	Text   string `json:"text"`
	Source string `json:"source"`
	Ref    string `json:"ref"`
}

// ArticleRow is a compact PubMed hit for brief rendering.
type ArticleRow struct {
	PMID    string `json:"pmid"`
	Title   string `json:"title"`
	Journal string `json:"journal,omitempty"`
	Year    string `json:"year,omitempty"`
	DOI     string `json:"doi,omitempty"`
}

// DatasetRow is a compact Oasis dataset hit.
type DatasetRow struct {
	ID               string `json:"id"`
	Title            string `json:"title"`
	QualityVerified  bool   `json:"quality_verified"`
	AuthenticityBand string `json:"authenticity_band,omitempty"`
	Domain           string `json:"domain,omitempty"`
}

// CompoundRow is a ChEMBL hit for brief rendering.
type CompoundRow struct {
	ChemblID string  `json:"chembl_id"`
	PrefName string  `json:"pref_name,omitempty"`
	MaxPhase float64 `json:"max_phase,omitempty"`
}

// GEORow is a GEO DataSet hit for brief rendering.
type GEORow struct {
	GDSID     string `json:"gds_id"`
	Accession string `json:"accession,omitempty"`
	Title     string `json:"title"`
	Organism  string `json:"organism,omitempty"`
	Samples   int    `json:"n_samples,omitempty"`
}

// Input is structured evidence gathered before markdown synthesis.
type Input struct {
	Topic       string
	Gathered    time.Time
	Articles    []ArticleRow
	Compounds   []CompoundRow
	GEODatasets []GEORow
	Datasets    []DatasetRow
	BaseURL     string
}

// Result is the full research brief output.
type Result struct {
	Topic       string         `json:"topic"`
	Markdown    string         `json:"markdown"`
	Claims      []Claim        `json:"claims"`
	Provenance  map[string]any `json:"provenance"`
	GeneratedAt time.Time      `json:"generated_at"`
}

// Build synthesizes a markdown brief and claim list from gathered evidence.
func Build(in Input) Result {
	if in.Gathered.IsZero() {
		in.Gathered = time.Now().UTC()
	}
	claims := make([]Claim, 0, len(in.Articles)+len(in.Datasets))
	var b strings.Builder

	b.WriteString("# Research Brief: ")
	b.WriteString(in.Topic)
	b.WriteString("\n\n")
	b.WriteString(fmt.Sprintf("*Generated %s · Oasis Science native workbench*\n\n", in.Gathered.Format(time.RFC3339)))

	b.WriteString("## 文献证据 (PubMed)\n\n")
	if len(in.Articles) == 0 {
		b.WriteString("_未检索到相关文献。_\n\n")
	} else {
		for i, a := range in.Articles {
			line := fmt.Sprintf("%d. **%s**", i+1, a.Title)
			if a.Journal != "" || a.Year != "" {
				line += fmt.Sprintf(" — %s %s", a.Journal, a.Year)
			}
			b.WriteString(line + "\n")
			ref := "https://pubmed.ncbi.nlm.nih.gov/" + a.PMID + "/"
			if a.DOI != "" {
				ref = "https://doi.org/" + a.DOI
			}
			b.WriteString(fmt.Sprintf("   - 溯源: PMID:%s · %s\n", a.PMID, ref))
			claims = append(claims, Claim{
				Text:   a.Title,
				Source: "pubmed",
				Ref:    ref,
			})
		}
		b.WriteString("\n")
	}

	b.WriteString("## 化合物 (ChEMBL)\n\n")
	if len(in.Compounds) == 0 {
		b.WriteString("_未检索到相关化合物。_\n\n")
	} else {
		for i, c := range in.Compounds {
			b.WriteString(fmt.Sprintf("%d. **%s** (%s)", i+1, c.PrefName, c.ChemblID))
			if c.MaxPhase > 0 {
				b.WriteString(fmt.Sprintf(" · phase %.0f", c.MaxPhase))
			}
			b.WriteString("\n")
			ref := "https://www.ebi.ac.uk/chembl/compound_report_card/" + c.ChemblID + "/"
			b.WriteString(fmt.Sprintf("   - 溯源: %s · %s\n", c.ChemblID, ref))
			claims = append(claims, Claim{Text: c.PrefName, Source: "chembl", Ref: ref})
		}
		b.WriteString("\n")
	}

	b.WriteString("## 表达数据 (GEO)\n\n")
	if len(in.GEODatasets) == 0 {
		b.WriteString("_未检索到相关 GEO 数据集。_\n\n")
	} else {
		for i, g := range in.GEODatasets {
			acc := g.Accession
			if acc == "" {
				acc = "GDS" + g.GDSID
			}
			b.WriteString(fmt.Sprintf("%d. **%s** (%s)", i+1, g.Title, acc))
			if g.Organism != "" {
				b.WriteString(" · " + g.Organism)
			}
			if g.Samples > 0 {
				b.WriteString(fmt.Sprintf(" · %d samples", g.Samples))
			}
			b.WriteString("\n")
			ref := "https://www.ncbi.nlm.nih.gov/geo/query/acc.cgi?acc=" + acc
			b.WriteString(fmt.Sprintf("   - 溯源: %s · %s\n", acc, ref))
			claims = append(claims, Claim{Text: g.Title, Source: "geo", Ref: ref})
		}
		b.WriteString("\n")
	}

	b.WriteString("## 已验证数据集 (绿洲)\n\n")
	if len(in.Datasets) == 0 {
		b.WriteString("_未匹配到 quality_verified 数据集（匿名检索）。登录后可预览样本。_\n\n")
	} else {
		base := strings.TrimRight(in.BaseURL, "/")
		for i, d := range in.Datasets {
			tag := ""
			if d.QualityVerified {
				tag = " ✓已验证"
			}
			b.WriteString(fmt.Sprintf("%d. **%s**%s\n", i+1, d.Title, tag))
			link := base + "/datasets/" + d.ID
			b.WriteString(fmt.Sprintf("   - 溯源: dataset:%s · %s\n", d.ID, link))
			claims = append(claims, Claim{
				Text:   d.Title,
				Source: "oasis",
				Ref:    link,
			})
		}
		b.WriteString("\n")
	}

	b.WriteString("## 下一步\n\n")
	b.WriteString("- 登录绿洲后可 `preview_schema` 查看样本列\n")
	b.WriteString("- 合并文献 + 数据集运行 C2D 因果/完整性分析（需 user token）\n")
	b.WriteString("- 导出本简报作为课题备忘录或上架前说明\n")

	return Result{
		Topic:    in.Topic,
		Markdown: b.String(),
		Claims:   claims,
		Provenance: map[string]any{
			"topic":          in.Topic,
			"article_count":  len(in.Articles),
			"compound_count": len(in.Compounds),
			"geo_count":      len(in.GEODatasets),
			"dataset_count":  len(in.Datasets),
			"geo_source":     "lumen-mcp-geo",
			"pubmed_source":  "lumen-mcp-pubmed",
			"chembl_source":  "lumen-mcp-chembl",
			"oasis_source":   "lumen-mcp-oasis",
			"oasis_base_url": in.BaseURL,
			"gathered_at":    in.Gathered,
		},
		GeneratedAt: in.Gathered,
	}
}
