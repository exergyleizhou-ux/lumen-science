"""A-shares trading rules: board lots, daily price limits, and transaction costs.

Kept as pure functions with explicit, documented rounding so the matching
engine's cost and limit arithmetic is reproducible and independently auditable.

Conventions modelled (configurable where they vary by board):
  * Board lot: buys are sized in multiples of 100 shares.
  * Daily price limit: prev_close * (1 ± pct), rounded to 2 decimals (the
    exchange rounds to the nearest fen). Main board pct = 0.10; ST = 0.05;
    STAR/ChiNext = 0.20 — passed in by the caller.
  * Limit lock: a stock that is locked at its upper limit all session has no
    sellers, so a buy cannot fill; locked at the lower limit, a sell cannot fill.
  * Commission: rate on notional with a per-order minimum (default ¥5).
  * Stamp duty: levied on the SELL side only.
"""
from __future__ import annotations

from data import Bar

LOT = 100


def round_lot(shares: float) -> int:
    """Floor a desired share count to a whole board lot (100 shares)."""
    if shares <= 0:
        return 0
    return int(shares // LOT) * LOT


def board_limit_pct(symbol: str, default: float = 0.10) -> float:
    """Daily price-limit band inferred from the symbol's board.

    STAR (688) and ChiNext (300) are ±20%, the Beijing exchange (4../8..) is
    ±30%, everything else (and non-numeric/test symbols) uses ``default`` (the
    ±10% main board). Caveat: ChiNext only moved to ±20% on 2020-08-24 — a
    pre-2020 ChiNext backtest should override this with a fixed ``limit_pct``.
    """
    if symbol.upper().endswith(".HK"):
        return 0.99  # Hong Kong has no daily price-limit band
    code = symbol.split(".", 1)[0]
    if code[:3] in ("688", "300"):
        return 0.20
    if code[:1] in ("4", "8"):
        return 0.30
    return default


def limit_prices(prev_close: float, pct: float) -> tuple[float, float]:
    """(upper_limit, lower_limit) for the session, rounded to 2 decimals."""
    up = round(prev_close * (1.0 + pct), 2)
    dn = round(prev_close * (1.0 - pct), 2)
    return up, dn


def can_buy_at(bar: Bar, price: float, pct: float) -> bool:
    """A buy at ``price`` can fill unless the bar is locked at the upper limit.

    The lock test is ``low >= upper_limit`` — the price never dipped below the
    ceiling, i.e. there were no sellers all session.
    """
    up, _ = limit_prices(bar.prev_close, pct)
    if bar.low >= up:
        return False
    return price <= bar.high


def can_sell_at(bar: Bar, price: float, pct: float) -> bool:
    """A sell at ``price`` can fill unless the bar is locked at the lower limit."""
    _, dn = limit_prices(bar.prev_close, pct)
    if bar.high <= dn:
        return False
    return price >= bar.low


def commission(notional: float, rate: float, minimum: float) -> float:
    """Broker commission on a trade's notional, with a per-order floor."""
    return round(max(abs(notional) * rate, minimum), 2)


def stamp_duty(notional: float, rate: float, side: str) -> float:
    """Stamp duty — sell side only in the A-shares market."""
    if side == "sell":
        return round(abs(notional) * rate, 2)
    return 0.0
