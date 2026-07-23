"""Tests for out-of-sample (walk-forward) evaluation.

The point of OOS: a strategy that only looks good in-sample is overfit. split_
evaluate runs the SAME strategy on an in-sample window and a held-out out-of-
sample window and reports both, so we can see whether an edge persists.
"""
import datetime as dt

from data import Bar, Bars
from engine import BacktestConfig
import oos


def _d(n):
    return dt.date(2024, 1, n)


class _AllInA:
    def on_bar(self, ctx):
        return {"A": 1.0}


def _cfg():
    return BacktestConfig(initial_cash=100000.0, commission_rate=0.0, commission_min=0.0,
                          stamp_duty_rate=0.0, slippage=0.0, limit_pct=0.10, max_participation=0.0)


def _rising_then_falling():
    # A rises through the in-sample half, falls through the out-of-sample half.
    rows = [
        (2, 10.0, 10.0),   # (day, open, close)
        (3, 10.0, 12.0),   # IS: +
        (4, 12.0, 12.0),
        (5, 12.0, 9.0),    # OOS: -
        (8, 9.0, 8.0),
    ]
    bars = [Bar(date=_d(n), open=o, high=max(o, c), low=min(o, c), close=c,
                volume=1e9, prev_close=o) for (n, o, c) in rows]
    return Bars({"A": bars})


def test_split_evaluate_separates_in_and_out_of_sample():
    r = oos.split_evaluate(_rising_then_falling(), _AllInA(), split_date=_d(3), cfg=_cfg())
    assert r["is"]["total_return"] > 0      # made money in-sample
    assert r["oos"]["total_return"] < 0     # lost money out-of-sample
    assert "persisted" in r


def test_persisted_flag_requires_positive_excess_in_both():
    r = oos.split_evaluate(_rising_then_falling(), _AllInA(), split_date=_d(3), cfg=_cfg())
    # single-symbol universe -> strategy ~ benchmark -> excess ~0 -> not persisted
    assert r["persisted"] is False
