package quant

import "testing"

func TestDefaultManifestRoundTrips(t *testing.T) {
	m := DefaultManifest("alpha")
	parsed, err := ParseManifest(FormatTOML(m))
	if err != nil {
		t.Fatalf("ParseManifest: %v", err)
	}
	if parsed.Name != "alpha" {
		t.Errorf("Name = %q, want alpha", parsed.Name)
	}
	if parsed.InitialCash != m.InitialCash {
		t.Errorf("InitialCash = %v, want %v", parsed.InitialCash, m.InitialCash)
	}
	if parsed.LimitPct != m.LimitPct {
		t.Errorf("LimitPct = %v, want %v", parsed.LimitPct, m.LimitPct)
	}
	if parsed.DataSource != m.DataSource {
		t.Errorf("DataSource = %q, want %q", parsed.DataSource, m.DataSource)
	}
}

func TestParseManifestReadsValues(t *testing.T) {
	raw := `name = "mom"
universe = "csi300"
start = "2020-01-01"
end = "2023-12-31"
initial_cash = 500000
commission_rate = 0.0003
commission_min = 5
stamp_duty_rate = 0.0005
slippage = 0.001
limit_pct = 0.1
data_source = "akshare"
`
	m, err := ParseManifest(raw)
	if err != nil {
		t.Fatalf("ParseManifest: %v", err)
	}
	if m.Universe != "csi300" {
		t.Errorf("Universe = %q", m.Universe)
	}
	if m.InitialCash != 500000 {
		t.Errorf("InitialCash = %v", m.InitialCash)
	}
	if m.Slippage != 0.001 {
		t.Errorf("Slippage = %v", m.Slippage)
	}
	if m.Start != "2020-01-01" || m.End != "2023-12-31" {
		t.Errorf("dates = %q..%q", m.Start, m.End)
	}
}

func TestParseManifestRejectsMissingName(t *testing.T) {
	if _, err := ParseManifest(`universe = "csi300"`); err == nil {
		t.Fatal("expected error for missing name")
	}
}
