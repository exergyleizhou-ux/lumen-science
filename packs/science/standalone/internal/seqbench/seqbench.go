// Package seqbench is a zero-dependency Motif-class sequence workbench core.
//
// It is intentionally pure Go (no shell, no network, no external MSA runners).
// SessionActor / lumen-science CLI own execution authority; this package only
// computes deterministic analyses over registered bytes.
package seqbench

import (
	"fmt"
	"strings"
	"unicode"
)

// Record is one sequence record (FASTA-style).
type Record struct {
	ID          string `json:"id"`
	Description string `json:"description,omitempty"`
	Sequence    string `json:"sequence"` // upper-case, no whitespace
	Kind        string `json:"kind"`     // dna | rna | protein | unknown
}

// Analysis is a durable, hashable report over one or more records.
type Analysis struct {
	SchemaVersion int              `json:"schema_version"`
	Tool          string           `json:"tool"`
	ToolVersion   string           `json:"tool_version"`
	Records       []RecordSummary  `json:"records"`
	Notes         []string         `json:"notes,omitempty"`
}

// RecordSummary is per-sequence productivity output.
type RecordSummary struct {
	ID              string             `json:"id"`
	Kind            string             `json:"kind"`
	Length          int                `json:"length"`
	GCFraction      *float64           `json:"gc_fraction,omitempty"`
	GCPercent       *float64           `json:"gc_percent,omitempty"`
	ReverseComplement string           `json:"reverse_complement,omitempty"`
	Translation     map[string]string  `json:"translation_frames,omitempty"` // frame+1..+3, -1..-3 short ORF peptide
	ORFs            []ORF              `json:"orfs,omitempty"`
	RestrictionHits []RestrictionHit   `json:"restriction_hits,omitempty"`
	Composition     map[string]int     `json:"composition,omitempty"`
}

// ORF is an open reading frame candidate.
type ORF struct {
	Frame      int    `json:"frame"` // +1..+3 or -1..-3
	Start      int    `json:"start"` // 0-based on the analyzed strand sequence
	End        int    `json:"end"`   // exclusive
	LengthAA   int    `json:"length_aa"`
	Protein    string `json:"protein"`
	Truncated  bool   `json:"truncated,omitempty"`
}

// RestrictionHit is a common enzyme cut site.
type RestrictionHit struct {
	Enzyme   string `json:"enzyme"`
	Site     string `json:"site"`
	Position int    `json:"position"` // 0-based on forward sequence
}

const ToolVersion = "1.0.0"

// ParseFASTA parses multi-FASTA text. Fail-closed on empty input.
func ParseFASTA(raw string) ([]Record, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, fmt.Errorf("empty sequence input")
	}
	// Allow raw sequence without header.
	if !strings.HasPrefix(raw, ">") {
		seq := normalizeSeq(raw)
		if seq == "" {
			return nil, fmt.Errorf("no sequence characters found")
		}
		return []Record{{
			ID: "unnamed", Sequence: seq, Kind: detectKind(seq),
		}}, nil
	}
	var out []Record
	var cur *Record
	var b strings.Builder
	flush := func() {
		if cur == nil {
			return
		}
		cur.Sequence = normalizeSeq(b.String())
		b.Reset()
		if cur.Sequence == "" {
			return
		}
		cur.Kind = detectKind(cur.Sequence)
		out = append(out, *cur)
		cur = nil
	}
	for _, line := range strings.Split(raw, "\n") {
		line = strings.TrimRight(line, "\r")
		if strings.HasPrefix(line, ">") {
			flush()
			header := strings.TrimSpace(line[1:])
			id, desc, _ := strings.Cut(header, " ")
			if id == "" {
				id = "unnamed"
			}
			cur = &Record{ID: id, Description: strings.TrimSpace(desc)}
			continue
		}
		if cur == nil {
			// Skip preamble noise before first header.
			continue
		}
		b.WriteString(line)
	}
	flush()
	if len(out) == 0 {
		return nil, fmt.Errorf("no FASTA records parsed")
	}
	return out, nil
}

// Analyze runs the default productivity suite over records.
func Analyze(records []Record) Analysis {
	a := Analysis{
		SchemaVersion: 1,
		Tool:          "lumen-seqbench",
		ToolVersion:   ToolVersion,
		Notes: []string{
			"Deterministic offline analysis. Not a substitute for wet-lab validation.",
			"Restriction sites are recognition-pattern hits only (no overhang simulation).",
			"ORFs use standard genetic code; min length 30 aa; ATG start.",
		},
	}
	for _, r := range records {
		a.Records = append(a.Records, summarize(r))
	}
	return a
}

func summarize(r Record) RecordSummary {
	s := RecordSummary{
		ID:          r.ID,
		Kind:        r.Kind,
		Length:      len(r.Sequence),
		Composition: composition(r.Sequence),
	}
	switch r.Kind {
	case "dna", "rna":
		gc := gcFraction(r.Sequence)
		pct := gc * 100
		s.GCFraction = &gc
		s.GCPercent = &pct
		s.ReverseComplement = reverseComplement(r.Sequence, r.Kind == "rna")
		s.Translation = map[string]string{}
		for frame := 1; frame <= 3; frame++ {
			pep := translate(r.Sequence, frame)
			if len(pep) > 80 {
				pep = pep[:80] + "…"
			}
			s.Translation[fmt.Sprintf("+%d", frame)] = pep
			// negative frames on reverse complement
			rc := reverseComplement(r.Sequence, r.Kind == "rna")
			pepN := translate(rc, frame)
			if len(pepN) > 80 {
				pepN = pepN[:80] + "…"
			}
			s.Translation[fmt.Sprintf("-%d", frame)] = pepN
		}
		s.ORFs = findORFs(r.Sequence, r.Kind == "rna", 30)
		if r.Kind == "dna" {
			s.RestrictionHits = findRestrictionSites(r.Sequence)
		}
	}
	return s
}

func normalizeSeq(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		if unicode.IsSpace(r) || r == '-' || r == '.' {
			continue
		}
		if r >= 'a' && r <= 'z' {
			r = r - 'a' + 'A'
		}
		if (r >= 'A' && r <= 'Z') || r == '*' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func detectKind(seq string) string {
	var a, c, g, t, u, other, aa int
	for _, r := range seq {
		switch r {
		case 'A':
			a++
		case 'C':
			c++
		case 'G':
			g++
		case 'T':
			t++
		case 'U':
			u++
		case 'N', 'R', 'Y', 'S', 'W', 'K', 'M', 'B', 'D', 'H', 'V':
			// ambiguous nt
		case 'E', 'F', 'I', 'L', 'P', 'Q', '*':
			aa++
		default:
			if r >= 'A' && r <= 'Z' {
				other++
			}
		}
	}
	n := len(seq)
	if n == 0 {
		return "unknown"
	}
	nt := a + c + g + t + u
	if float64(nt)/float64(n) >= 0.85 {
		if u > t {
			return "rna"
		}
		return "dna"
	}
	if aa > 0 || float64(other+aa)/float64(n) > 0.15 {
		return "protein"
	}
	return "unknown"
}

func composition(seq string) map[string]int {
	m := map[string]int{}
	for _, r := range seq {
		m[string(r)]++
	}
	return m
}

func gcFraction(seq string) float64 {
	if len(seq) == 0 {
		return 0
	}
	var gc, n int
	for _, r := range seq {
		switch r {
		case 'G', 'C', 'S':
			gc++
			n++
		case 'A', 'T', 'U', 'W':
			n++
		}
	}
	if n == 0 {
		return 0
	}
	return float64(gc) / float64(n)
}

func reverseComplement(seq string, rna bool) string {
	comp := map[byte]byte{
		'A': 'T', 'T': 'A', 'G': 'C', 'C': 'G',
		'R': 'Y', 'Y': 'R', 'S': 'S', 'W': 'W',
		'K': 'M', 'M': 'K', 'B': 'V', 'D': 'H', 'H': 'D', 'V': 'B',
		'N': 'N', 'U': 'A',
	}
	if rna {
		comp['A'] = 'U'
		comp['T'] = 'A'
	}
	b := make([]byte, len(seq))
	for i := 0; i < len(seq); i++ {
		ch := seq[len(seq)-1-i]
		if c, ok := comp[ch]; ok {
			b[i] = c
		} else {
			b[i] = 'N'
		}
	}
	return string(b)
}

// Standard genetic code (DNA codons).
var geneticCode = map[string]byte{
	"TTT": 'F', "TTC": 'F', "TTA": 'L', "TTG": 'L',
	"TCT": 'S', "TCC": 'S', "TCA": 'S', "TCG": 'S',
	"TAT": 'Y', "TAC": 'Y', "TAA": '*', "TAG": '*',
	"TGT": 'C', "TGC": 'C', "TGA": '*', "TGG": 'W',
	"CTT": 'L', "CTC": 'L', "CTA": 'L', "CTG": 'L',
	"CCT": 'P', "CCC": 'P', "CCA": 'P', "CCG": 'P',
	"CAT": 'H', "CAC": 'H', "CAA": 'Q', "CAG": 'Q',
	"CGT": 'R', "CGC": 'R', "CGA": 'R', "CGG": 'R',
	"ATT": 'I', "ATC": 'I', "ATA": 'I', "ATG": 'M',
	"ACT": 'T', "ACC": 'T', "ACA": 'T', "ACG": 'T',
	"AAT": 'N', "AAC": 'N', "AAA": 'K', "AAG": 'K',
	"AGT": 'S', "AGC": 'S', "AGA": 'R', "AGG": 'R',
	"GTT": 'V', "GTC": 'V', "GTA": 'V', "GTG": 'V',
	"GCT": 'A', "GCC": 'A', "GCA": 'A', "GCG": 'A',
	"GAT": 'D', "GAC": 'D', "GAA": 'E', "GAG": 'E',
	"GGT": 'G', "GGC": 'G', "GGA": 'G', "GGG": 'G',
}

func toDNA(seq string) string {
	return strings.ReplaceAll(seq, "U", "T")
}

func translate(seq string, frame int) string {
	if frame < 1 || frame > 3 {
		return ""
	}
	dna := toDNA(seq)
	var b strings.Builder
	for i := frame - 1; i+3 <= len(dna); i += 3 {
		codon := dna[i : i+3]
		aa, ok := geneticCode[codon]
		if !ok {
			b.WriteByte('X')
			continue
		}
		b.WriteByte(aa)
	}
	return b.String()
}

func findORFs(seq string, rna bool, minAA int) []ORF {
	var out []ORF
	// + frames
	for frame := 1; frame <= 3; frame++ {
		out = append(out, orfsOnStrand(seq, frame, minAA)...)
	}
	// - frames
	rc := reverseComplement(seq, rna)
	for frame := 1; frame <= 3; frame++ {
		for _, o := range orfsOnStrand(rc, frame, minAA) {
			o.Frame = -frame
			out = append(out, o)
		}
	}
	// Cap for productivity reports
	if len(out) > 50 {
		out = out[:50]
	}
	return out
}

func orfsOnStrand(seq string, frame, minAA int) []ORF {
	dna := toDNA(seq)
	var out []ORF
	i := frame - 1
	for i+2 < len(dna) {
		if dna[i:i+3] != "ATG" {
			i += 3
			continue
		}
		start := i
		var pep strings.Builder
		trunc := true
		j := i
		for j+2 < len(dna) {
			codon := dna[j : j+3]
			aa, ok := geneticCode[codon]
			if !ok {
				aa = 'X'
			}
			if aa == '*' {
				trunc = false
				j += 3
				break
			}
			pep.WriteByte(aa)
			j += 3
		}
		protein := pep.String()
		if len(protein) >= minAA {
			out = append(out, ORF{
				Frame:     frame,
				Start:     start,
				End:       j,
				LengthAA:  len(protein),
				Protein:   protein,
				Truncated: trunc,
			})
		}
		i += 3
	}
	return out
}

// Common research enzymes (recognition sites only).
var enzymes = []struct {
	name string
	site string
}{
	{"EcoRI", "GAATTC"},
	{"BamHI", "GGATCC"},
	{"HindIII", "AAGCTT"},
	{"XhoI", "CTCGAG"},
	{"NotI", "GCGGCCGC"},
	{"NdeI", "CATATG"},
	{"NcoI", "CCATGG"},
	{"SacI", "GAGCTC"},
	{"KpnI", "GGTACC"},
	{"PstI", "CTGCAG"},
	{"SalI", "GTCGAC"},
	{"XbaI", "TCTAGA"},
	{"SpeI", "ACTAGT"},
	{"BglII", "AGATCT"},
	{"ClaI", "ATCGAT"},
}

func findRestrictionSites(seq string) []RestrictionHit {
	dna := toDNA(seq)
	var hits []RestrictionHit
	for _, e := range enzymes {
		site := e.site
		for i := 0; i+len(site) <= len(dna); i++ {
			if dna[i:i+len(site)] == site {
				hits = append(hits, RestrictionHit{
					Enzyme: e.name, Site: site, Position: i,
				})
			}
		}
	}
	if len(hits) > 100 {
		hits = hits[:100]
	}
	return hits
}

// MarkdownReport renders a human-readable productivity report.
func MarkdownReport(a Analysis, sourceLabel string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "# Sequence analysis report\n\n")
	fmt.Fprintf(&b, "- tool: `%s` %s\n", a.Tool, a.ToolVersion)
	fmt.Fprintf(&b, "- source: `%s`\n", sourceLabel)
	fmt.Fprintf(&b, "- records: %d\n\n", len(a.Records))
	for _, r := range a.Records {
		fmt.Fprintf(&b, "## %s\n\n", r.ID)
		fmt.Fprintf(&b, "| field | value |\n|---|---|\n")
		fmt.Fprintf(&b, "| kind | %s |\n", r.Kind)
		fmt.Fprintf(&b, "| length | %d |\n", r.Length)
		if r.GCPercent != nil {
			fmt.Fprintf(&b, "| GC%% | %.2f |\n", *r.GCPercent)
		}
		fmt.Fprintf(&b, "\n")
		if len(r.ORFs) > 0 {
			fmt.Fprintf(&b, "### ORFs (min 30 aa)\n\n")
			fmt.Fprintf(&b, "| frame | start | end | aa | truncated |\n|---|---:|---:|---:|---|\n")
			for _, o := range r.ORFs {
				fmt.Fprintf(&b, "| %+d | %d | %d | %d | %v |\n", o.Frame, o.Start, o.End, o.LengthAA, o.Truncated)
			}
			fmt.Fprintf(&b, "\n")
		}
		if len(r.RestrictionHits) > 0 {
			fmt.Fprintf(&b, "### Restriction sites\n\n")
			fmt.Fprintf(&b, "| enzyme | site | position |\n|---|---|---:|\n")
			for _, h := range r.RestrictionHits {
				fmt.Fprintf(&b, "| %s | `%s` | %d |\n", h.Enzyme, h.Site, h.Position)
			}
			fmt.Fprintf(&b, "\n")
		}
	}
	if len(a.Notes) > 0 {
		fmt.Fprintf(&b, "## Notes\n\n")
		for _, n := range a.Notes {
			fmt.Fprintf(&b, "- %s\n", n)
		}
	}
	fmt.Fprintf(&b, "\n## Provenance\n\n")
	fmt.Fprintf(&b, "Generated offline by Lumen Science seqbench. Not medical advice. ")
	fmt.Fprintf(&b, "All claims require registered artifact SHA-256 in the product path.\n")
	return b.String()
}
