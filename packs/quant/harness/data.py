"""Point-in-time daily bar store for the backtest engine.

`Bars` is the *only* market-data surface the engine exposes to a strategy. Its
``window`` method is the structural guarantee against lookahead bias: on day t it
returns bars dated <= t and nothing later, no matter what the strategy asks for.

Pure stdlib, no pandas/numpy — so a backtest is bit-identical across machines
and library versions, which is what makes the resulting certificate verifiable.
"""
from __future__ import annotations

import datetime as dt
from typing import Dict, List, NamedTuple


class Bar(NamedTuple):
    date: dt.date
    open: float
    high: float
    low: float
    close: float
    volume: float
    # Previous session's close — the reference for A-shares ±limit calculation.
    prev_close: float


class Bars:
    """Immutable per-symbol daily bar series.

    Bars are sorted by date once at construction so ``window`` is a cheap suffix
    slice rather than a re-sort on every call.
    """

    def __init__(self, series: Dict[str, List[Bar]]):
        self._series: Dict[str, List[Bar]] = {
            sym: sorted(rows, key=lambda b: b.date) for sym, rows in series.items()
        }

    def window(self, symbol: str, as_of: dt.date, lookback: int) -> List[Bar]:
        """Last ``lookback`` bars for ``symbol`` with ``bar.date <= as_of``.

        Returns ``[]`` for an unknown symbol or for an ``as_of`` before the
        symbol's first bar. Never returns a bar dated after ``as_of``.
        """
        rows = self._series.get(symbol)
        if not rows:
            return []
        visible = [b for b in rows if b.date <= as_of]
        if lookback <= 0:
            return []
        return visible[-lookback:]

    def symbols(self) -> List[str]:
        """All symbols in the dataset, sorted for deterministic iteration."""
        return sorted(self._series)

    def bar_on(self, symbol: str, day: dt.date):
        """The bar for ``symbol`` dated exactly ``day``, or ``None`` if it did
        not trade that session (not listed, or halted)."""
        for b in self._series.get(symbol, ()):  # series are short; linear is fine
            if b.date == day:
                return b
        return None

    def trading_days(self) -> List[dt.date]:
        """Sorted union of every session present anywhere in the dataset."""
        days = set()
        for rows in self._series.values():
            days.update(b.date for b in rows)
        return sorted(days)
