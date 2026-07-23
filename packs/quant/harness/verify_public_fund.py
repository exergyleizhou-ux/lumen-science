"""Validation driver: (a) ground-truth single-stock check, (b) public fund replay."""
import warnings, sys, os, csv, tempfile, datetime as dt
warnings.filterwarnings("ignore")
sys.path.insert(0, os.path.expanduser("~/lumen/internal/quant/harness"))

import fetch, dataset, fund
from engine import Engine, BacktestConfig
from replay import ReplayStrategy

START, END = "2024-01-01", "2024-12-31"


def bars_for(symbols, start, end):
    rows = []
    for s in symbols:
        try:
            rows.extend(fetch.fetch_symbol(s, start, end))
        except Exception as e:
            print(f"   (skip {s}: {e})", file=sys.stderr)
    fd, path = tempfile.mkstemp(suffix=".csv")
    with os.fdopen(fd, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["date", "symbol", "open", "high", "low", "close", "volume"])
        w.writeheader()
        for r in rows:
            w.writerow({k: r[k] for k in w.fieldnames})
    bars = dataset.load_csv(path)
    os.remove(path)
    return bars


def real_stock_return(sym, start, end):
    rows = fetch.fetch_symbol(sym, start, end)
    rows.sort(key=lambda r: r["date"])
    return rows[-1]["close"] / rows[0]["open"] - 1.0  # buy first open, mark last close


# ── (a) GROUND TRUTH: once fully invested, the engine's DAILY return must equal
#        the stock's real daily close-to-close return (pure pricing check, immune
#        to execution-timing and cash-drag confounders) ──
print("=" * 64)
print("(a) GROUND-TRUTH PRICING CHECK — engine daily returns vs real 600519.SH daily returns")
SYM = "600519.SH"
gt_bars = bars_for([SYM], START, END)
days = gt_bars.trading_days()
res = Engine(gt_bars, BacktestConfig(initial_cash=10_000_000.0, commission_rate=0.0,
             commission_min=0.0, stamp_duty_rate=0.0, slippage=0.0,
             max_participation=0.0)).run(ReplayStrategy([(days[0], {SYM: 1.0})]))
# engine daily returns, keyed by date
eng = {d: (res.equity[i] / res.equity[i-1] - 1.0) for i, d in enumerate(res.dates) if i > 0}
# real stock daily close/prev_close returns
real = {b.date: (b.close / b.prev_close - 1.0) for b in gt_bars.window(SYM, days[-1], 10**9)
        if b.prev_close > 0}
common = sorted(set(eng) & set(real))
# only days where the engine is fully invested (skip the day-0/day-1 ramp)
inv = [d for d in common if abs(eng[d]) > 1e-9][2:]
import statistics
diffs = [abs(eng[d] - real[d]) for d in inv]
mean_abs = statistics.mean(diffs) if diffs else 1.0
matched = sum(1 for d in inv if abs(eng[d] - real[d]) < 1e-4)
print(f"   invested days compared : {len(inv)}")
print(f"   days engine daily-return == stock daily-return (<1e-4): {matched}/{len(inv)}")
print(f"   mean abs daily diff     : {mean_abs:.6f}  -> pricing {'VALIDATED' if mean_abs < 5e-4 else 'MISMATCH'}")

# ── (b) PUBLIC FUND REPLAY — 易方达蓝筹精选 005827 disclosed A-share holdings ──
print("=" * 64)
print("(b) PUBLIC FUND REPLAY — 005827 易方达蓝筹精选, disclosed A-share holdings (renorm)")
timeline = fund.fetch_fund_holdings("005827", ["2023", "2024"])
syms = sorted({s for _, w in timeline for s in w})
print(f"   disclosures: {len(timeline)} quarters · {len(syms)} A-share names")
fbars = bars_for(syms, START, END)
cfg = BacktestConfig(initial_cash=10_000_000.0, commission_rate=0.0003, commission_min=5.0,
                     stamp_duty_rate=0.0005, slippage=0.001, limit_pct=0.10, max_participation=0.10)
fres = Engine(fbars, cfg).run(ReplayStrategy(timeline))
m = fres.metrics
reported = fund.fund_reported_return("005827", START, END)
print(f"   OUR honest replica  : return {m['total_return']:+.2%}  sharpe {m['sharpe']:.2f}  maxDD {m['max_drawdown']:.2%}")
print(f"   replica vs benchmark: excess {m['excess_return']:+.2%}  (benchmark {m['benchmark_return']:+.2%})  IR {m['information_ratio']:.2f}")
print(f"   FUND reported return: {reported:+.2%}" if reported is not None else "   FUND reported return: (unavailable)")
print("   caveat: disclosed top-holdings only (renorm), quarterly snapshots, HK dropped, lag unmodeled")
