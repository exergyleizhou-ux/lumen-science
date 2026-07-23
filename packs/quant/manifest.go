// Package quant implements `lumen quant` — a verifiable A-shares backtest
// toolchain. It mirrors the lumen oasis vertical (init → backtest → verify) and
// its provenance model: a strategy is run by a deterministic engine in a
// hardened sandbox against a hash-pinned dataset, producing a re-computable
// backtest certificate (VQ-xxxx). See docs/superpowers/specs for the design.
package quant

import (
	"fmt"
	"strconv"
	"strings"
)

// ManifestFile is the per-strategy manifest scaffolded by `lumen quant init`.
const ManifestFile = "quant.toml"

// Manifest is the decoded quant.toml — the strategy's identity, its backtest
// universe/window, and the A-shares market rules used at fill time.
type Manifest struct {
	Name           string  `toml:"name"`
	Universe       string  `toml:"universe"`        // e.g. "csi300" or a comma list
	Start          string  `toml:"start"`           // YYYY-MM-DD
	End            string  `toml:"end"`             // YYYY-MM-DD
	InitialCash    float64 `toml:"initial_cash"`
	CommissionRate float64 `toml:"commission_rate"`
	CommissionMin  float64 `toml:"commission_min"`
	StampDutyRate  float64 `toml:"stamp_duty_rate"` // sell side only
	Slippage       float64 `toml:"slippage"`
	LimitPct       float64 `toml:"limit_pct"` // daily price-limit band
	DataSource     string  `toml:"data_source"`
}

// DefaultManifest returns sensible A-shares defaults for a new strategy.
func DefaultManifest(name string) Manifest {
	return Manifest{
		Name:           name,
		Universe:       "csi300",
		Start:          "2020-01-01",
		End:            "2023-12-31",
		InitialCash:    1_000_000,
		CommissionRate: 0.0003,
		CommissionMin:  5,
		StampDutyRate:  0.0005,
		Slippage:       0.001,
		LimitPct:       0.10,
		DataSource:     "csv", // reads ./data.csv (scaffolded); "akshare" to fetch
	}
}

// Config returns the engine config dict (matches BacktestConfig in run.py).
func (m Manifest) Config() map[string]any {
	return map[string]any{
		"initial_cash":    m.InitialCash,
		"commission_rate": m.CommissionRate,
		"commission_min":  m.CommissionMin,
		"stamp_duty_rate": m.StampDutyRate,
		"slippage":        m.Slippage,
		"limit_pct":       m.LimitPct,
	}
}

// FormatTOML renders a Manifest as flat key=value TOML (mirrors oasis).
func FormatTOML(m Manifest) string {
	var b strings.Builder
	fmt.Fprintf(&b, "name = %q\n", m.Name)
	fmt.Fprintf(&b, "universe = %q\n", m.Universe)
	fmt.Fprintf(&b, "start = %q\n", m.Start)
	fmt.Fprintf(&b, "end = %q\n", m.End)
	fmt.Fprintf(&b, "initial_cash = %s\n", trimFloat(m.InitialCash))
	fmt.Fprintf(&b, "commission_rate = %s\n", trimFloat(m.CommissionRate))
	fmt.Fprintf(&b, "commission_min = %s\n", trimFloat(m.CommissionMin))
	fmt.Fprintf(&b, "stamp_duty_rate = %s\n", trimFloat(m.StampDutyRate))
	fmt.Fprintf(&b, "slippage = %s\n", trimFloat(m.Slippage))
	fmt.Fprintf(&b, "limit_pct = %s\n", trimFloat(m.LimitPct))
	fmt.Fprintf(&b, "data_source = %q\n", m.DataSource)
	return b.String()
}

// ParseManifest parses flat key=value TOML into a Manifest. Name is required.
func ParseManifest(raw string) (Manifest, error) {
	var m Manifest
	for _, line := range strings.Split(raw, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		key, val, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		key = strings.TrimSpace(key)
		val = strings.TrimSpace(val)
		sval := strings.Trim(val, `"`)
		switch key {
		case "name":
			m.Name = sval
		case "universe":
			m.Universe = sval
		case "start":
			m.Start = sval
		case "end":
			m.End = sval
		case "data_source":
			m.DataSource = sval
		case "initial_cash":
			m.InitialCash = parseFloat(val)
		case "commission_rate":
			m.CommissionRate = parseFloat(val)
		case "commission_min":
			m.CommissionMin = parseFloat(val)
		case "stamp_duty_rate":
			m.StampDutyRate = parseFloat(val)
		case "slippage":
			m.Slippage = parseFloat(val)
		case "limit_pct":
			m.LimitPct = parseFloat(val)
		}
	}
	if m.Name == "" {
		return Manifest{}, fmt.Errorf("quant.toml: missing required field 'name'")
	}
	return m, nil
}

func parseFloat(s string) float64 {
	f, _ := strconv.ParseFloat(strings.TrimSpace(s), 64)
	return f
}

func trimFloat(f float64) string {
	return strconv.FormatFloat(f, 'g', -1, 64)
}
