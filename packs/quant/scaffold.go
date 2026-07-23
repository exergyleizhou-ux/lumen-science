package quant

import (
	"os"
	"path/filepath"
)

// ScaffoldStrategy writes a runnable strategy package into dir: the manifest, a
// complete (not skeleton) example strategy, a small sample dataset so `quant
// backtest` works immediately, and a .gitignore for generated artifacts.
func ScaffoldStrategy(dir string, m Manifest) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	files := map[string]string{
		ManifestFile:  FormatTOML(m),
		"strategy.py": strategyTemplate,
		"data.csv":    sampleData,
		".gitignore":  gitignoreTemplate,
		"README.md":   readmeTemplate,
	}
	for name, body := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(body), 0o644); err != nil {
			return err
		}
	}
	return nil
}

const strategyTemplate = `"""Example strategy: equal-weight the top-N names by N-day momentum.

The engine enforces no-lookahead structurally: ` + "`ctx`" + ` can only expose data
dated <= today, and ` + "`ctx.universe`" + ` lists only names that actually traded today
(survivorship-correct). Edit on_bar with your own logic.
"""


class Strategy:
    N = 2          # number of names to hold
    LOOKBACK = 3   # momentum window (trading days)

    def on_bar(self, ctx):
        scores = {}
        for sym in ctx.universe:
            h = ctx.history(sym, "close", self.LOOKBACK)
            if len(h) == self.LOOKBACK and h[0] > 0:
                scores[sym] = h[-1] / h[0] - 1.0  # simple momentum
        winners = sorted(scores, key=scores.get, reverse=True)[: self.N]
        if not winners:
            return {}            # all cash
        w = 1.0 / len(winners)
        return {s: w for s in winners}  # target weights; engine handles T+1/limits/fees
`

// A tiny two-symbol sample so a freshly-scaffolded strategy backtests out of the
// box. Replace with real data via `lumen quant data` (data_source = "akshare").
const sampleData = `date,symbol,open,high,low,close,volume
2024-01-02,600519.SH,1700.0,1715.0,1695.0,1710.0,30000
2024-01-03,600519.SH,1710.0,1730.0,1705.0,1725.0,32000
2024-01-04,600519.SH,1725.0,1740.0,1718.0,1738.0,31000
2024-01-05,600519.SH,1738.0,1745.0,1725.0,1730.0,29000
2024-01-08,600519.SH,1730.0,1755.0,1728.0,1750.0,33000
2024-01-09,600519.SH,1750.0,1768.0,1745.0,1762.0,34000
2024-01-10,600519.SH,1762.0,1775.0,1755.0,1770.0,32000
2024-01-11,600519.SH,1770.0,1772.0,1750.0,1758.0,30000
2024-01-12,600519.SH,1758.0,1780.0,1756.0,1778.0,35000
2024-01-15,600519.SH,1778.0,1795.0,1772.0,1790.0,36000
2024-01-16,600519.SH,1790.0,1805.0,1786.0,1800.0,34000
2024-01-17,600519.SH,1800.0,1812.0,1792.0,1808.0,33000
2024-01-02,000858.SZ,150.0,152.0,149.0,151.0,50000
2024-01-03,000858.SZ,151.0,151.5,148.0,148.5,52000
2024-01-04,000858.SZ,148.5,150.0,147.5,149.8,51000
2024-01-05,000858.SZ,149.8,153.0,149.5,152.5,53000
2024-01-08,000858.SZ,152.5,153.0,150.0,150.8,54000
2024-01-09,000858.SZ,150.8,152.0,149.5,151.6,52000
2024-01-10,000858.SZ,151.6,154.0,151.0,153.8,55000
2024-01-11,000858.SZ,153.8,154.0,151.5,152.2,51000
2024-01-12,000858.SZ,152.2,155.0,152.0,154.6,56000
2024-01-15,000858.SZ,154.6,156.0,153.8,155.4,54000
2024-01-16,000858.SZ,155.4,157.0,154.9,156.6,55000
2024-01-17,000858.SZ,156.6,158.0,155.5,157.2,53000
`

const gitignoreTemplate = `__pycache__/
*.pyc
/quant-lock.json
/results.json
`

const readmeTemplate = `# quant strategy

` + "```" + `
lumen quant backtest .   # run the pinned backtest -> quant-cert.json (VQ-xxxx)
lumen quant verify .     # re-run and confirm the cert reproduces bit-identically
` + "```" + `

A verified certificate proves the equity curve really came from this code on
this data with no future-data leakage. It makes no claim about future returns.
`
