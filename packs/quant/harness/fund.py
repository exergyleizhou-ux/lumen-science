"""Fetch a public fund's legally-disclosed holdings and its reported return.

A host-side acquisition step (uses akshare/pandas, like fetch.py). Public funds
must disclose top holdings quarterly — a free, structured, real-world track
record we can independently recompute with the honest engine.

Honest caveats (surfaced to the user, not hidden):
  * Only the disclosed TOP holdings (~top-10), renormalized to 100% — ignores the
    rest of the book and cash drag. This is a "disclosed A-share picks" replica,
    not the whole fund.
  * Quarterly snapshots held constant between disclosures — ignores intra-quarter
    trades.
  * HK / non-A-share holdings (港股通, 5-digit codes) are dropped (the A-share
    engine can't price them).
  * Disclosure lag (~3 weeks after quarter-end) is not modelled — mild lookahead.
"""
from __future__ import annotations

import datetime as dt
from typing import Dict, List, Tuple

from fetch import with_suffix

_QUARTER_END = {"1": (3, 31), "2": (6, 30), "3": (9, 30), "4": (12, 31)}


def _is_tradable_stock(code: str) -> bool:
    code = str(code).strip()
    if len(code) == 6 and code[0] in ("0", "3", "6", "4", "8"):
        return True       # A-share
    return len(code) == 5 and code.isdigit()  # Hong Kong (港股通)


def fetch_fund_holdings(code: str, years: List[str]) -> List[Tuple[dt.date, Dict[str, float]]]:
    """Return [(quarter_end_date, {symbol: weight})] for a fund, A-shares + HK
    (港股通), weights renormalized to sum to 1.0 within each quarter."""
    import akshare as ak

    by_quarter: Dict[dt.date, Dict[str, float]] = {}
    for y in years:
        try:
            df = ak.fund_portfolio_hold_em(symbol=code, date=str(y))
        except Exception:
            continue
        for _, r in df.iterrows():
            code_raw = str(r["股票代码"]).strip()
            if not _is_tradable_stock(code_raw):
                continue
            q = str(r["季度"])
            qi = next((k for k in _QUARTER_END if f"{k}季度" in q), None)
            if qi is None:
                continue
            mm, dd = _QUARTER_END[qi]
            qe = dt.date(int(y), mm, dd)
            w = float(r["占净值比例"]) / 100.0
            by_quarter.setdefault(qe, {})[with_suffix(code_raw)] = w

    timeline: List[Tuple[dt.date, Dict[str, float]]] = []
    for qe in sorted(by_quarter):
        weights = by_quarter[qe]
        total = sum(weights.values())
        if total > 0:
            timeline.append((qe, {s: w / total for s, w in weights.items()}))
    return timeline


def fund_reported_return(code: str, start: str, end: str) -> float | None:
    """The fund's own reported cumulative-NAV total return over [start, end]."""
    import akshare as ak

    try:
        df = ak.fund_open_fund_info_em(symbol=code, indicator="累计净值走势")
    except Exception:
        try:
            df = ak.fund_open_fund_info_em(fund=code, indicator="累计净值走势")
        except Exception:
            return None
    dcol = "净值日期" if "净值日期" in df.columns else df.columns[0]
    vcol = "累计净值" if "累计净值" in df.columns else df.columns[1]
    s, e = dt.date.fromisoformat(start), dt.date.fromisoformat(end)
    rows = []
    for _, r in df.iterrows():
        d = r[dcol]
        d = d if isinstance(d, dt.date) else dt.date.fromisoformat(str(d)[:10])
        if s <= d <= e:
            rows.append((d, float(r[vcol])))
    if len(rows) < 2:
        return None
    rows.sort()
    return rows[-1][1] / rows[0][1] - 1.0
