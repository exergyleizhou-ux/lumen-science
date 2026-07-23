"""Tests for A-shares trading rules — round lots, price limits, fees.

These are pure functions so the matching engine's hardest-to-eyeball arithmetic
(limit rounding, stamp duty being sell-side only) is pinned in isolation.
"""
import datetime as dt

from data import Bar
import rules


def _bar(prev_close, o=None, h=None, l=None, c=None):
    o = prev_close if o is None else o
    c = o if c is None else c
    h = max(o, c) if h is None else h
    l = min(o, c) if l is None else l
    return Bar(date=dt.date(2024, 1, 2), open=o, high=h, low=l, close=c,
               volume=1000, prev_close=prev_close)


# ── round lots ──────────────────────────────────────────────────────────────
def test_round_lot_floors_to_hundred():
    assert rules.round_lot(0) == 0
    assert rules.round_lot(99) == 0
    assert rules.round_lot(150) == 100
    assert rules.round_lot(1999) == 1900


# ── price limits ────────────────────────────────────────────────────────────
def test_limit_prices_main_board_ten_percent():
    up, dn = rules.limit_prices(prev_close=10.00, pct=0.10)
    assert up == 11.00
    assert dn == 9.00


def test_limit_prices_round_to_two_decimals():
    # 10.03 * 1.10 = 11.033 -> 11.03 ; * 0.90 = 9.027 -> 9.03
    up, dn = rules.limit_prices(prev_close=10.03, pct=0.10)
    assert up == 11.03
    assert dn == 9.03


# ── fill eligibility under limit lock ───────────────────────────────────────
def test_cannot_buy_when_locked_up():
    # opens at the +10% limit and never trades below it -> no seller, buy rejected
    bar = _bar(prev_close=10.00, o=11.00, h=11.00, l=11.00, c=11.00)
    assert rules.can_buy_at(bar, price=11.00, pct=0.10) is False


def test_can_buy_when_not_locked():
    bar = _bar(prev_close=10.00, o=10.50, h=10.80, l=10.20, c=10.60)
    assert rules.can_buy_at(bar, price=10.50, pct=0.10) is True


def test_cannot_sell_when_locked_down():
    bar = _bar(prev_close=10.00, o=9.00, h=9.00, l=9.00, c=9.00)
    assert rules.can_sell_at(bar, price=9.00, pct=0.10) is False


def test_can_sell_when_not_locked():
    bar = _bar(prev_close=10.00, o=9.50, h=9.80, l=9.20, c=9.40)
    assert rules.can_sell_at(bar, price=9.50, pct=0.10) is True


# ── fees ────────────────────────────────────────────────────────────────────
def test_commission_has_minimum():
    # 0.0003 rate, min 5 yuan: a tiny trade pays the floor
    assert rules.commission(1000.0, rate=0.0003, minimum=5.0) == 5.0
    # a large trade pays the rate
    assert rules.commission(100000.0, rate=0.0003, minimum=5.0) == 30.0


def test_stamp_duty_is_sell_side_only():
    assert rules.stamp_duty(100000.0, rate=0.0005, side="sell") == 50.0
    assert rules.stamp_duty(100000.0, rate=0.0005, side="buy") == 0.0


def test_board_limit_pct_by_symbol():
    assert rules.board_limit_pct("600519.SH") == 0.10  # main board
    assert rules.board_limit_pct("000858.SZ") == 0.10  # SZ main board
    assert rules.board_limit_pct("300750.SZ") == 0.20  # ChiNext
    assert rules.board_limit_pct("688981.SH") == 0.20  # STAR
    assert rules.board_limit_pct("830799.BJ") == 0.30  # Beijing
    # unknown / test symbols fall back to the supplied default
    assert rules.board_limit_pct("A", default=0.10) == 0.10
