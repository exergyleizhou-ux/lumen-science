"""Tests for ReplayStrategy — replaying a disclosed holdings timeline.

This is how we independently re-compute a public track record (a fund's disclosed
quarterly holdings, a 雪球 portfolio's rebalance history, positions someone posted):
the engine drives the *disclosed* weights through honest, point-in-time pricing
with real costs, and we compare our number to the claimed one.

The point-in-time invariant matters here too: on day t the strategy may only use
the most recent holdings disclosed on/before t — never a future rebalance.
"""
import datetime as dt

from replay import ReplayStrategy


class _Ctx:
    """Minimal stand-in for the engine Context (ReplayStrategy only needs today)."""
    def __init__(self, day):
        self.today = day


def _d(n):
    return dt.date(2024, 1, n)


def _timeline():
    return [
        (_d(2), {"A": 1.0}),
        (_d(4), {"A": 0.5, "B": 0.5}),
    ]


def test_uses_most_recent_disclosure_as_of_today():
    s = ReplayStrategy(_timeline())
    assert s.on_bar(_Ctx(_d(2))) == {"A": 1.0}
    assert s.on_bar(_Ctx(_d(3))) == {"A": 1.0}            # holds between disclosures
    assert s.on_bar(_Ctx(_d(4))) == {"A": 0.5, "B": 0.5}  # rebalance applies
    assert s.on_bar(_Ctx(_d(5))) == {"A": 0.5, "B": 0.5}


def test_all_cash_before_first_disclosure():
    s = ReplayStrategy(_timeline())
    assert s.on_bar(_Ctx(_d(1))) == {}


def test_never_uses_a_future_rebalance():
    # On day 3 the day-4 rebalance must NOT leak in.
    s = ReplayStrategy(_timeline())
    assert "B" not in s.on_bar(_Ctx(_d(3)))


def test_unsorted_timeline_is_handled():
    s = ReplayStrategy([(_d(4), {"A": 0.5, "B": 0.5}), (_d(2), {"A": 1.0})])
    assert s.on_bar(_Ctx(_d(3))) == {"A": 1.0}
