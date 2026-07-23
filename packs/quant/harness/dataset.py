"""Pinned-CSV dataset loading and canonical content hashing.

A backtest is only reproducible if its input data is pinned. ``dataset_hash``
produces a content hash over the *normalized* bars (sorted, fixed-precision), so
the same data hashes identically regardless of CSV row order or whitespace — and
flips the moment any value changes. That hash is the cert's ``data_hash``.

Expected CSV columns: date,symbol,open,high,low,close,volume. ``prev_close`` is
derived as the prior session's close per symbol (the first session falls back to
its own open).
"""
from __future__ import annotations

import csv
import datetime as dt
import hashlib
from typing import Dict, List, Tuple

from data import Bar, Bars

_FIELDS = ("date", "symbol", "open", "high", "low", "close", "volume")


def _read_rows(path: str) -> List[dict]:
    rows = []
    with open(path, newline="") as f:
        for raw in csv.DictReader(f):
            if not raw.get("symbol") or not raw.get("date"):
                continue
            rows.append({
                "date": dt.date.fromisoformat(raw["date"].strip()),
                "symbol": raw["symbol"].strip(),
                "open": float(raw["open"]),
                "high": float(raw["high"]),
                "low": float(raw["low"]),
                "close": float(raw["close"]),
                "volume": float(raw["volume"]),
            })
    return rows


def load_csv(path: str) -> Bars:
    rows = _read_rows(path)
    by_symbol: Dict[str, List[dict]] = {}
    for r in rows:
        by_symbol.setdefault(r["symbol"], []).append(r)

    series: Dict[str, List[Bar]] = {}
    for sym, srows in by_symbol.items():
        srows.sort(key=lambda r: r["date"])
        bars = []
        prev_close = None
        for r in srows:
            bars.append(Bar(
                date=r["date"], open=r["open"], high=r["high"], low=r["low"],
                close=r["close"], volume=r["volume"],
                prev_close=prev_close if prev_close is not None else r["open"],
            ))
            prev_close = r["close"]
        series[sym] = bars
    return Bars(series)


def _canonical_rows(path: str) -> List[Tuple]:
    rows = _read_rows(path)
    # Sort by (symbol, date) and format floats at fixed precision so cosmetic
    # differences in the source file do not change the hash.
    norm = sorted(
        (
            r["symbol"], r["date"].isoformat(),
            f"{r['open']:.6f}", f"{r['high']:.6f}", f"{r['low']:.6f}",
            f"{r['close']:.6f}", f"{r['volume']:.6f}",
        )
        for r in rows
    )
    return norm


def dataset_hash(path: str) -> str:
    h = hashlib.sha256()
    for row in _canonical_rows(path):
        h.update(("|".join(row) + "\n").encode("utf-8"))
    return h.hexdigest()
