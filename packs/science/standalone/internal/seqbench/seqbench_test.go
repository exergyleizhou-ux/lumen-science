package seqbench

import (
	"strings"
	"testing"
)

func TestParseFASTAMulti(t *testing.T) {
	raw := `>geneA BRCA2 fragment
ATGCGTGAATTCGCGGCCGC
>geneB
GGATCCAAAA`
	recs, err := ParseFASTA(raw)
	if err != nil {
		t.Fatal(err)
	}
	if len(recs) != 2 {
		t.Fatalf("want 2 records, got %d", len(recs))
	}
	if recs[0].ID != "geneA" || !strings.Contains(recs[0].Description, "BRCA2") {
		t.Fatalf("header parse: %+v", recs[0])
	}
	if recs[0].Kind != "dna" {
		t.Fatalf("kind=%s", recs[0].Kind)
	}
}

func TestParseRawSequence(t *testing.T) {
	recs, err := ParseFASTA("atgcatgcatgc")
	if err != nil {
		t.Fatal(err)
	}
	if recs[0].Sequence != "ATGCATGCATGC" {
		t.Fatalf("got %s", recs[0].Sequence)
	}
}

func TestEmptyFailsClosed(t *testing.T) {
	if _, err := ParseFASTA("   "); err == nil {
		t.Fatal("expected error")
	}
}

func TestAnalyzeFindsEcoRIAndORF(t *testing.T) {
	// ATG + long ORF ending with TAA, includes EcoRI GAATTC
	// Build a synthetic ORF: ATG + (AAA)*40 + TAA = 42 aa including M and stop-excluded
	var body strings.Builder
	body.WriteString("ATG")
	for i := 0; i < 40; i++ {
		body.WriteString("AAA") // K
	}
	body.WriteString("TAA")
	// prefix EcoRI
	seq := "GAATTC" + body.String()
	recs, err := ParseFASTA(">syn\n" + seq)
	if err != nil {
		t.Fatal(err)
	}
	a := Analyze(recs)
	if len(a.Records) != 1 {
		t.Fatal("expected 1 record")
	}
	r := a.Records[0]
	if r.GCPercent == nil {
		t.Fatal("expected GC")
	}
	foundEco := false
	for _, h := range r.RestrictionHits {
		if h.Enzyme == "EcoRI" && h.Position == 0 {
			foundEco = true
		}
	}
	if !foundEco {
		t.Fatalf("EcoRI not found: %+v", r.RestrictionHits)
	}
	if len(r.ORFs) == 0 {
		t.Fatal("expected at least one ORF")
	}
	md := MarkdownReport(a, "test.fa")
	if !strings.Contains(md, "Sequence analysis report") {
		t.Fatal("markdown missing title")
	}
}

func TestReverseComplement(t *testing.T) {
	if got := reverseComplement("ATGC", false); got != "GCAT" {
		t.Fatalf("got %s", got)
	}
}

func TestDeterministic(t *testing.T) {
	raw := ">x\nATGAATTCGCGGCCGCTAA"
	r1, _ := ParseFASTA(raw)
	r2, _ := ParseFASTA(raw)
	a1 := Analyze(r1)
	a2 := Analyze(r2)
	if a1.Records[0].Length != a2.Records[0].Length {
		t.Fatal("nondeterministic length")
	}
	if MarkdownReport(a1, "a") != MarkdownReport(a2, "a") {
		t.Fatal("nondeterministic markdown")
	}
}
