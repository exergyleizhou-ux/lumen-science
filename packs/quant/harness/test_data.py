"""Tests for the point-in-time bar store — the no-lookahead foundation.

The whole verifiable-backtest claim rests on one invariant: on day t the engine
can only ever see data dated <= t. These tests pin that invariant at the lowest
level, before any strategy or portfolio logic exists to muddy it.
"""
import datetime as dt

from data import Bar, Bars


def _d(s):
    return dt.date.fromisoformat(s)


def _series(symbol, rows):
    # rows: list of (date_str, close) -> Bars with open=high=low=close, prev_close chained
    bars = []
    prev = None
    for date_str, close in rows:
        bars.append(Bar(
            date=_d(date_str), open=close, high=close, low=close,
            close=close, volume=1000, prev_close=prev if prev is not None else close,
        ))
        prev = close
    return Bars({symbol: bars})


def test_window_never_returns_future_bars():
    bars = _series("600519.SH", [
        ("2024-01-02", 100.0),
        ("2024-01-03", 101.0),
        ("2024-01-04", 102.0),
        ("2024-01-05", 103.0),
    ])
    w = bars.window("600519.SH", as_of=_d("2024-01-04"), lookback=10)
    dates = [b.date for b in w]
    assert _d("2024-01-05") not in dates, "leaked a future bar"
    assert dates == [_d("2024-01-02"), _d("2024-01-03"), _d("2024-01-04")]


def test_window_respects_lookback_length():
    bars = _series("600519.SH", [
        ("2024-01-02", 100.0),
        ("2024-01-03", 101.0),
        ("2024-01-04", 102.0),
        ("2024-01-05", 103.0),
    ])
    w = bars.window("600519.SH", as_of=_d("2024-01-05"), lookback=2)
    assert [b.close for b in w] == [102.0, 103.0]


def test_window_empty_before_listing():
    bars = _series("600519.SH", [("2024-01-04", 102.0)])
    assert bars.window("600519.SH", as_of=_d("2024-01-02"), lookback=5) == []
