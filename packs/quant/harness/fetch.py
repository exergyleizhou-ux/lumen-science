"""A-shares daily-bar fetcher for `lumen quant data` (default source: akshare).

This is a *host-side data acquisition* step, not part of the reproducible engine:
its output — a normalized data.csv — is what gets hash-pinned and fed to the
sandbox. So unlike the engine, fetch.py may use pandas/akshare. The pure parts
(column normalization, exchange-suffix inference, explicit-universe parsing) are
unit-tested; the network fetch is integration-only.

CLI:
    python fetch.py --universe csi300 --start 2020-01-01 --end 2023-12-31 --out data.csv
    python fetch.py --symbols 600519.SH,000858.SZ --start ... --end ... --out data.csv
"""
from __future__ import annotations

import argparse
import csv
import sys
from typing import Dict, List

# akshare column (Chinese) -> canonical field
_COLMAP = {"开盘": "open", "收盘": "close", "最高": "high", "最低": "low", "成交量": "volume"}


def with_suffix(code: str) -> str:
    """Infer the exchange suffix for a bare code. 5-digit = Hong Kong (港股通);
    6-digit = A-shares by board."""
    if "." in code:
        return code
    c = code.strip()
    if len(c) == 5:  # Hong Kong (held by mainland funds via 港股通)
        return c + ".HK"
    if c[:1] == "6":  # main board (incl. 688 STAR) lists on Shanghai
        return c + ".SH"
    if c[:1] in ("0", "3"):  # SZ main board + ChiNext(300)
        return c + ".SZ"
    if c[:1] in ("4", "8"):  # Beijing Stock Exchange
        return c + ".BJ"
    return c + ".SH"


def is_hk(symbol: str) -> bool:
    """True for a Hong Kong listing (suffix .HK, or a bare 5-digit code)."""
    s = symbol.strip().upper()
    return s.endswith(".HK") or ("." not in s and len(s) == 5)


def bare_code(symbol: str) -> str:
    """The numeric code akshare expects (suffix stripped)."""
    return symbol.split(".", 1)[0]


def resolve_universe(universe: str) -> List[str]:
    """Resolve a manifest universe string to a concrete symbol list.

    Explicit comma lists are returned (suffix-normalized). Named indices like
    "csi300"/"沪深300" are resolved to their *current* constituents via akshare
    — a survivorship caveat the caller should surface.
    """
    u = universe.strip()
    if u.lower() in ("csi300", "hs300", "沪深300"):
        return _csi300_constituents()
    return [with_suffix(s.strip()) for s in u.split(",") if s.strip()]


def _csi300_constituents() -> List[str]:  # pragma: no cover - network
    import akshare as ak
    df = ak.index_stock_cons_csindex(symbol="000300")
    col = "成分券代码" if "成分券代码" in df.columns else df.columns[4]
    return [with_suffix(str(c).zfill(6)) for c in df[col].tolist()]


def normalize_rows(symbol: str, df) -> List[Dict]:
    """Map an akshare daily-history frame to canonical rows, sorted by date.

    A-shares 成交量 is in 手 (1 手 = 100 shares) → convert to shares so the engine's
    volume/participation cap is in the same unit as order sizes. HK 成交量 is
    already in shares, so it is left as-is.
    """
    factor = 1.0 if is_hk(symbol) else 100.0
    rows: List[Dict] = []
    for _, r in df.iterrows():
        row = {"date": str(r["日期"]), "symbol": symbol}
        for zh, en in _COLMAP.items():
            row[en] = float(r[zh])
        row["volume"] *= factor
        rows.append(row)
    rows.sort(key=lambda x: x["date"])
    return rows


def fetch_symbol(symbol: str, start: str, end: str) -> List[Dict]:  # pragma: no cover - network
    import akshare as ak
    code = bare_code(symbol)
    sd, ed = start.replace("-", ""), end.replace("-", "")
    if is_hk(symbol):
        df = ak.stock_hk_hist(symbol=code, period="daily", start_date=sd, end_date=ed, adjust="qfq")
    else:
        df = ak.stock_zh_a_hist(symbol=code, period="daily", start_date=sd, end_date=ed, adjust="qfq")
    return normalize_rows(symbol, df)


def write_csv(path: str, rows: List[Dict]) -> None:
    fields = ["date", "symbol", "open", "high", "low", "close", "volume"]
    with open(path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        for r in rows:
            w.writerow({k: r[k] for k in fields})


def main(argv=None) -> int:  # pragma: no cover - integration
    ap = argparse.ArgumentParser(description="Fetch pinned A-shares daily bars.")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--universe")
    g.add_argument("--symbols")
    ap.add_argument("--start", required=True)
    ap.add_argument("--end", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    symbols = resolve_universe(args.symbols or args.universe)
    if args.universe and args.universe.strip().lower() in ("csi300", "hs300", "沪深300"):
        print("note: using CURRENT index constituents (membership survivorship not "
              "reconstructed); pin an explicit symbol list for a frozen universe.",
              file=sys.stderr)

    all_rows: List[Dict] = []
    for sym in symbols:
        rows = fetch_symbol(sym, args.start, args.end)
        all_rows.extend(rows)
        print(f"  {sym}: {len(rows)} bars", file=sys.stderr)
    write_csv(args.out, all_rows)
    print(f"wrote {len(all_rows)} bars for {len(symbols)} symbols -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
