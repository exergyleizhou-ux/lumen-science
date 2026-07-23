"""Screen public funds for PERSISTENT stock-selection skill.

For each fund: replay its disclosed A-share top holdings through the honest
engine, split 2024 into in-sample (H1) and out-of-sample (H2), and measure excess
return vs CSI300 in BOTH halves. A fund whose excess persists into H2 has a skill
signal that didn't just come from one lucky regime.

Honest caveats: top-holdings-only (renormalized, HK dropped), quarterly snapshots.
So this measures 'did their disclosed A-share top picks beat CSI300 in both
halves', a narrow but real read — not the full fund.
"""
import sys, os, csv, tempfile, datetime as dt, warnings
warnings.filterwarnings("ignore")
sys.path.insert(0, os.path.expanduser("~/lumen/internal/quant/harness"))

import fetch, dataset, fund
from engine import Engine, BacktestConfig
from replay import ReplayStrategy

IS_END = dt.date(2024, 6, 30)
OOS_START = dt.date(2024, 7, 1)
START, END = "2024-01-01", "2024-12-31"

# A mix of well-known ACTIVE A-share funds (some carry HK -> bigger coverage gap).
FUNDS = {
    "110022": "易方达消费行业(萧楠)",
    "163406": "兴全合润(谢治宇)",
    "260108": "景顺长城新兴成长(刘彦春)",
    "000083": "汇添富消费行业",
    "005827": "易方达蓝筹(张坤,含港股)",
    "001714": "工银文体产业",
}


def _sina_symbol(sym):
    code = sym.split(".", 1)[0]
    if sym.endswith(".HK"):
        return "hk", code
    if sym.endswith(".SZ"):
        return "a", "sz" + code
    if sym.endswith(".BJ"):
        return "a", "bj" + code
    return "a", "sh" + code


def _sina_fetch(sym):
    """Fetch daily bars from sina (stock_zh_a_daily / stock_hk_daily). sina is
    reachable when the eastmoney push2 endpoints are blocked by a proxy."""
    import akshare as ak
    kind, scode = _sina_symbol(sym)
    if kind == "hk":
        df = ak.stock_hk_daily(symbol=scode, adjust="qfq")
    else:
        df = ak.stock_zh_a_daily(symbol=scode, start_date=START.replace("-", ""),
                                 end_date=END.replace("-", ""), adjust="qfq")
    rows = []
    for _, r in df.iterrows():
        d = str(r["date"])[:10]
        if START <= d <= END:
            rows.append({"date": d, "symbol": sym, "open": float(r["open"]), "high": float(r["high"]),
                         "low": float(r["low"]), "close": float(r["close"]), "volume": float(r["volume"])})
    return rows


def bars_for(symbols):
    """Fetch bars for every symbol, retrying transient network drops. Returns
    (bars, priced_count) so the caller can refuse to report a fund whose data
    only partially loaded (otherwise a network-starved replay looks 'flat' and
    fabricates an excess vs the benchmark)."""
    rows = []
    priced = 0
    for s in symbols:
        for _ in range(3):
            try:
                got = _sina_fetch(s)
                if got:
                    rows.extend(got); priced += 1
                break
            except Exception:
                continue
    fd, path = tempfile.mkstemp(suffix=".csv")
    with os.fdopen(fd, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["date", "symbol", "open", "high", "low", "close", "volume"])
        w.writeheader()
        for r in rows:
            w.writerow({k: r[k] for k in w.fieldnames})
    b = dataset.load_csv(path); os.remove(path)
    return b, priced


def csi300_window_returns():
    import akshare as ak
    # sina index endpoint (stock_zh_index_daily) — avoids the eastmoney push2
    # index-code-map host that some networks/proxies block.
    df = ak.stock_zh_index_daily(symbol="sh000300")
    ser = [(dt.date.fromisoformat(str(r["date"])[:10]), float(r["close"])) for _, r in df.iterrows()]
    ser = [(d, v) for d, v in ser if dt.date(2024, 1, 1) <= d <= dt.date(2024, 12, 31)]
    ser.sort()
    is_pts = [v for d, v in ser if d <= IS_END]
    oos_pts = [v for d, v in ser if d >= OOS_START]
    return (is_pts[-1] / is_pts[0] - 1.0, oos_pts[-1] / oos_pts[0] - 1.0)


cfg = BacktestConfig(initial_cash=10_000_000.0, commission_rate=0.0003, commission_min=5.0,
                     stamp_duty_rate=0.0005, slippage=0.001, limit_pct=0.10, max_participation=0.0)

csi_is, csi_oos = csi300_window_returns()
print(f"CSI300  IS(H1) {csi_is:+.2%}   OOS(H2) {csi_oos:+.2%}\n")
print(f"{'fund':<28}{'IS excess':>11}{'OOS excess':>12}{'persisted':>11}")
print("-" * 62)

rows = []
for code, name in FUNDS.items():
    try:
        timeline = fund.fetch_fund_holdings(code, ["2023", "2024"])
        syms = sorted({s for _, w in timeline for s in w})
        if not syms:
            print(f"{name:<28}  (no disclosed holdings)")
            continue
        b, priced = bars_for(syms)
        if priced < len(syms) * 0.8:
            print(f"{name:<28}  DATA FAIL ({priced}/{len(syms)} priced — network, not a result)")
            continue
        is_ret = Engine(b, cfg).run(ReplayStrategy(timeline), end=IS_END).metrics["total_return"]
        oos_ret = Engine(b, cfg).run(ReplayStrategy(timeline), start=OOS_START).metrics["total_return"]
        is_x, oos_x = is_ret - csi_is, oos_ret - csi_oos
        persisted = is_x > 0 and oos_x > 0
        rows.append((name, is_x, oos_x, persisted))
        print(f"{name:<28}{is_x:>+10.2%}{oos_x:>+11.2%}{('YES' if persisted else 'no'):>11}")
    except Exception as e:
        print(f"{name:<28}  (error: {str(e)[:30]})")

print("\nPersistent (skill in BOTH halves):")
for name, isx, oosx, p in sorted(rows, key=lambda r: r[2], reverse=True):
    if p:
        print(f"  ✓ {name}  OOS excess {oosx:+.2%}")
