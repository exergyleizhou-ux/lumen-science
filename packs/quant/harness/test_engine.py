"""Tests for the backtest engine day-loop: accounting, execution, limits.

Execution model under test: a strategy decides target weights at day d's close
(seeing only data <= d) and the resulting orders fill at day d+1's OPEN. Equity
is marked at each day's close. The buy-and-hold scenario below is hand-computed.
"""
import datetime as dt

import pytest

from data import Bar, Bars
from engine import Engine, BacktestConfig


def _d(n):
    return dt.date(2024, 1, n)


def _bars_A():
    # (day, open, high, low, close, prev_close)
    rows = [
        (2, 10.0, 10.0, 10.0, 10.0, 10.0),
        (3, 10.0, 11.2, 9.8, 11.0, 10.0),
        (4, 11.0, 12.2, 10.8, 12.0, 11.0),
        (5, 12.0, 12.6, 11.9, 12.5, 12.0),
    ]
    bars = [Bar(date=_d(n), open=o, high=h, low=l, close=c, volume=1e6, prev_close=p)
            for (n, o, h, l, c, p) in rows]
    return Bars({"A": bars})


class _AllInA:
    """Always target 100% of symbol A."""
    def on_bar(self, ctx):
        return {"A": 1.0}


def _cfg(**kw):
    base = dict(initial_cash=100000.0, commission_rate=0.0, commission_min=0.0,
                stamp_duty_rate=0.0, slippage=0.0, limit_pct=0.10)
    base.update(kw)
    return BacktestConfig(**base)


def test_fill_capped_by_volume_participation():
    # A is wildly liquid-constrained: only 5000 shares trade per day. With a
    # 10% participation cap, a fill can be at most 500 shares no matter how much
    # cash wants in — the realism that stops a backtest "buying" the whole float.
    rows = [
        (2, 10.0, 10.0, 10.0, 10.0, 10.0),
        (3, 10.0, 10.2, 9.8, 10.1, 10.0),
        (4, 10.1, 10.3, 9.9, 10.2, 10.1),
    ]
    bars = Bars({"A": [Bar(date=_d(n), open=o, high=h, low=l, close=c, volume=5000, prev_close=p)
                       for (n, o, h, l, c, p) in rows]})
    cfg = _cfg(initial_cash=10_000_000.0, max_participation=0.10)
    result = Engine(bars, cfg).run(_AllInA())
    first_buy = [t for t in result.trades if t.side == "buy"][0]
    assert first_buy.qty == 500  # 10% of 5000, lot-rounded — not the ~1M cash wants


def test_benchmark_and_excess_return_reported():
    # Equal-weight buy&hold benchmark of the two names: A +20%, B +10% -> +15%.
    # A flat (all-cash) strategy returns 0, so excess = 0 - 0.15 = -0.15.
    def series(sym, closes):
        prev = None
        out = []
        for i, c in enumerate(closes):
            out.append(Bar(date=_d(2 + i), open=c, high=c, low=c, close=c, volume=1e6,
                           prev_close=prev if prev is not None else c))
            prev = c
        return out

    bars = Bars({"A": series("A", [100.0, 110.0, 120.0]),
                 "B": series("B", [100.0, 105.0, 110.0])})

    class _Flat:
        def on_bar(self, ctx):
            return {}

    m = Engine(bars, _cfg()).run(_Flat()).metrics
    assert m["benchmark_return"] == pytest.approx(0.15)
    assert m["excess_return"] == pytest.approx(m["total_return"] - 0.15)
    # a flat strategy that trails a rising benchmark has a negative information ratio
    assert "information_ratio" in m
    assert m["information_ratio"] < 0


def test_turnover_zero_when_flat_and_positive_when_trading():
    class _Flat:
        def on_bar(self, ctx):
            return {}

    flat = Engine(_bars_A(), _cfg()).run(_Flat()).metrics
    assert flat["turnover"] == 0.0

    held = Engine(_bars_A(), _cfg()).run(_AllInA()).metrics
    # buy-and-hold: 100000 traded notional / mean equity 113750 ~= 0.879
    assert held["turnover"] == pytest.approx(0.879, abs=1e-2)


def test_run_restricts_trading_to_window():
    # start=d3 -> the equity curve covers only d3..d5 (out-of-sample window).
    result = Engine(_bars_A(), _cfg()).run(_AllInA(), start=_d(3))
    assert result.dates == [_d(3), _d(4), _d(5)]
    result2 = Engine(_bars_A(), _cfg()).run(_AllInA(), end=_d(3))
    assert result2.dates == [_d(2), _d(3)]


def test_buy_and_hold_equity_curve_is_exact():
    result = Engine(_bars_A(), _cfg()).run(_AllInA())
    assert result.dates == [_d(2), _d(3), _d(4), _d(5)]
    # day2: all cash (decision made, no fill yet) -> 100000
    # day3 open: buy 10000 @ 10 = 100000, cash 0; close 11 -> 110000
    # day4 close: 10000 @ 12 -> 120000 ; day5 close: 10000 @ 12.5 -> 125000
    assert result.equity == [100000.0, 110000.0, 120000.0, 125000.0]


def test_buy_and_hold_makes_exactly_one_trade():
    result = Engine(_bars_A(), _cfg()).run(_AllInA())
    buys = [t for t in result.trades if t.side == "buy"]
    assert len(buys) == 1
    assert buys[0].symbol == "A"
    assert buys[0].qty == 10000
    assert buys[0].price == 10.0
    assert buys[0].date == _d(3)


def test_buy_blocked_when_limit_locked_up():
    # Day 3 opens locked at the +10% ceiling (low >= upper limit) -> no fill.
    rows = [
        (2, 10.0, 10.0, 10.0, 10.0, 10.0),
        (3, 11.0, 11.0, 11.0, 11.0, 10.0),   # locked up all session
        (4, 11.0, 11.5, 10.8, 11.2, 11.0),
    ]
    bars = Bars({"A": [Bar(date=_d(n), open=o, high=h, low=l, close=c, volume=1e6, prev_close=p)
                       for (n, o, h, l, c, p) in rows]})
    result = Engine(bars, _cfg()).run(_AllInA())
    # No buy could fill on day3; first fill is day4 open @ 11.0
    assert [t.date for t in result.trades] == [_d(4)]


def test_commission_charged_on_buy_and_reduces_equity():
    cfg = _cfg(commission_rate=0.0003, commission_min=5.0, stamp_duty_rate=0.0005)
    result = Engine(_bars_A(), cfg).run(_AllInA())
    buys = [t for t in result.trades if t.side == "buy"]
    # A commission was charged on the buy (no stamp duty on the buy side).
    assert buys[0].commission > 0.0
    assert buys[0].stamp_duty == 0.0
    # Fees make the held position worth strictly less than the fee-free 110000.
    assert result.equity[1] < 110000.0
