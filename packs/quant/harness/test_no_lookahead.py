"""The moat tests: no-lookahead, survivorship correctness, and determinism.

These are the properties that make a backtest *verifiable* rather than just
*reproducible-noise*. If any of these fail, the certificate is meaningless.
"""
import datetime as dt

from data import Bar, Bars
from engine import Engine, BacktestConfig


def _d(n):
    return dt.date(2024, 1, n)


def _mk(symbol_rows):
    series = {}
    for sym, rows in symbol_rows.items():
        prev = None
        bars = []
        for n, close in rows:
            bars.append(Bar(date=_d(n), open=close, high=close, low=close,
                            close=close, volume=1e6,
                            prev_close=prev if prev is not None else close))
            prev = close
        series[sym] = bars
    return Bars(series)


def _cfg():
    return BacktestConfig(initial_cash=100000.0, commission_rate=0.0,
                          commission_min=0.0, stamp_duty_rate=0.0, limit_pct=0.10)


# ── no lookahead ─────────────────────────────────────────────────────────────
class _CheatingStrategy:
    """Tries as hard as possible to see the future via the Context."""
    def __init__(self):
        self.violations = []

    def on_bar(self, ctx):
        # Ask for a huge lookback; the engine must still only hand back <= today.
        closes_with_dates = list(zip(
            ctx.history("A", "date", 9999),
            ctx.history("A", "close", 9999),
        ))
        for d, _c in closes_with_dates:
            if d > ctx.today:
                self.violations.append((ctx.today, d))
        # price() must equal today's close, never a future one.
        if ctx.price("A") is not None and ctx.history("A", "close", 1):
            if ctx.price("A") != ctx.history("A", "close", 1)[-1]:
                self.violations.append(("price-mismatch", ctx.today))
        return {}


def test_strategy_cannot_observe_future_bars():
    bars = _mk({"A": [(2, 10.0), (3, 11.0), (4, 12.0), (5, 13.0)]})
    strat = _CheatingStrategy()
    Engine(bars, _cfg()).run(strat)
    assert strat.violations == [], f"future data leaked to strategy: {strat.violations}"


# ── survivorship ─────────────────────────────────────────────────────────────
class _UniverseRecorder:
    def __init__(self):
        self.seen = {}

    def on_bar(self, ctx):
        self.seen[ctx.today] = set(ctx.universe)
        return {}


def test_universe_excludes_delisted_after_delisting():
    # DEAD trades days 2-3 then delists; LIVE trades throughout.
    bars = _mk({
        "LIVE": [(2, 10.0), (3, 10.0), (4, 10.0), (5, 10.0)],
        "DEAD": [(2, 5.0), (3, 5.0)],
    })
    rec = _UniverseRecorder()
    Engine(bars, _cfg()).run(rec)
    assert "DEAD" in rec.seen[_d(3)]      # still listed
    assert "DEAD" not in rec.seen[_d(4)]  # delisted — not a survivor
    assert "DEAD" not in rec.seen[_d(5)]


def test_universe_excludes_not_yet_listed():
    bars = _mk({
        "OLD": [(2, 10.0), (3, 10.0), (4, 10.0)],
        "IPO": [(4, 8.0)],  # lists on day 4
    })
    rec = _UniverseRecorder()
    Engine(bars, _cfg()).run(rec)
    assert "IPO" not in rec.seen[_d(2)]
    assert "IPO" not in rec.seen[_d(3)]
    assert "IPO" in rec.seen[_d(4)]


# ── determinism ──────────────────────────────────────────────────────────────
class _MomentumStrategy:
    def on_bar(self, ctx):
        # buy A if its 2-day return is positive, else go flat — exercises history
        h = ctx.history("A", "close", 2)
        if len(h) == 2 and h[-1] > h[0]:
            return {"A": 1.0}
        return {}


def test_run_is_bit_identical_across_runs():
    bars = _mk({"A": [(2, 10.0), (3, 11.0), (4, 10.5), (5, 12.0), (8, 12.5)]})
    r1 = Engine(bars, _cfg()).run(_MomentumStrategy())
    r2 = Engine(bars, _cfg()).run(_MomentumStrategy())
    assert r1.equity == r2.equity
    assert r1.metrics == r2.metrics
    assert [(t.date, t.symbol, t.side, t.qty, t.price) for t in r1.trades] == \
           [(t.date, t.symbol, t.side, t.qty, t.price) for t in r2.trades]
