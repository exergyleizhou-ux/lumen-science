package brief

import (
	"strings"
	"testing"
	"time"
)

func TestBuildMarkdownAndClaims(t *testing.T) {
	res := Build(Input{
		Topic:       "EGFR inhibitor",
		Gathered:    time.Date(2026, 7, 3, 12, 0, 0, 0, time.UTC),
		Articles:    []ArticleRow{{PMID: "123", Title: "EGFR paper", Journal: "Nature", Year: "2024", DOI: "10.1/egfr"}},
		Compounds:   []CompoundRow{{ChemblID: "CHEMBL203", PrefName: "Gefitinib", MaxPhase: 4}},
		GEODatasets: []GEORow{{GDSID: "200123", Accession: "GDS1234", Title: "EGFR expression", Organism: "Homo sapiens", Samples: 24}},
		Datasets:    []DatasetRow{{ID: "ds1", Title: "Expression DS", QualityVerified: true}},
		BaseURL:     "https://demo.oasisdata2026.xyz",
	})
	if !strings.Contains(res.Markdown, "EGFR inhibitor") {
		t.Fatal("missing topic")
	}
	if !strings.Contains(res.Markdown, "ChEMBL") {
		t.Fatal("missing chembl section")
	}
	if !strings.Contains(res.Markdown, "GEO") {
		t.Fatal("missing geo section")
	}
	if len(res.Claims) != 4 {
		t.Fatalf("claims: %d", len(res.Claims))
	}
	if res.Provenance["article_count"] != 1 {
		t.Fatalf("provenance: %v", res.Provenance)
	}
}
