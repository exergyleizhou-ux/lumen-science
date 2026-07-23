"""Deterministic A-shares daily backtest engine.

Execution model (the no-lookahead contract):
  * On day ``d``'s close the strategy's ``on_bar`` runs with a Context that can
    only see data dated <= d.
  * The target weights it returns are translated to orders that fill at day
    ``d+1``'s OPEN — you never trade on information you could not have had.
  * Equity is marked at each day's close.

A-shares realities modelled at fill time (see rules.py): board-lot sizing, daily
price limits (a limit-locked session cannot fill the blocked side), commission
with a floor, and sell-side stamp duty. T+1 is structural here: ``sellable`` is
the quantity held coming into the session, so shares bought at today's open are
not sellable until the next session.

Pure stdlib — no pandas/numpy in the engine — so a run is bit-identical across
machines, which is what the backtest certificate attests to.
"""
from __future__ import annotations

import datetime as dt
from dataclasses import dataclass, field
from typing import Dict, List, Optional

import metrics
import rules
from data import Bars


@dataclass(frozen=True)
class BacktestConfig:
    initial_cash: float = 1_000_000.0
    commission_rate: float = 0.0003
    commission_min: float = 5.0
    stamp_duty_rate: float = 0.0005  # sell side only
    slippage: float = 0.0            # fraction added to buys / subtracted from sells
    limit_pct: float = 0.10          # daily price-limit band
    max_participation: float = 0.10  # max fill as a fraction of the bar's volume (0 = uncapped)


@dataclass(frozen=True)
class Trade:
    date: dt.date
    symbol: str
    side: str          # "buy" | "sell"
    qty: int
    price: float
    commission: float
    stamp_duty: float


@dataclass
class BacktestResult:
    dates: List[dt.date]
    equity: List[float]
    trades: List[Trade]
    metrics: Dict[str, float]


class Context:
    """The point-in-time view handed to the strategy on a given session.

    Every data accessor is bounded by ``as_of``; there is no method that can
    reach a future bar. ``universe`` is survivorship-correct because it lists
    only symbols that actually traded on ``as_of`` (delisted names drop out, and
    not-yet-listed names are absent).
    """

    def __init__(self, as_of: dt.date, bars: Bars, cash: float,
                 holdings: Dict[str, int], portfolio_value: float):
        self._as_of = as_of
        self._bars = bars
        self.cash = cash
        self.holdings = dict(holdings)
        self.portfolio_value = portfolio_value

    @property
    def today(self) -> dt.date:
        return self._as_of

    @property
    def universe(self) -> List[str]:
        return [s for s in self._bars.symbols() if self._bars.bar_on(s, self._as_of)]

    def history(self, symbol: str, field: str, lookback: int) -> List[float]:
        return [getattr(b, field) for b in self._bars.window(symbol, self._as_of, lookback)]

    def price(self, symbol: str) -> Optional[float]:
        w = self._bars.window(symbol, self._as_of, 1)
        return w[-1].close if w else None


class Engine:
    def __init__(self, bars: Bars, config: BacktestConfig):
        self._bars = bars
        self._cfg = config

    def run(self, strategy, start: Optional[dt.date] = None, end: Optional[dt.date] = None) -> BacktestResult:
        """Run the backtest. ``start``/``end`` restrict TRADING to that window
        (for out-of-sample evaluation); the strategy's Context still sees the full
        history, so signals can look back across the window boundary — exactly the
        right semantics for an OOS test (trade only OOS, but use prior data for
        the signal)."""
        cfg = self._cfg
        bars = self._bars
        days = [d for d in bars.trading_days()
                if (start is None or d >= start) and (end is None or d <= end)]
        if not days:
            return BacktestResult(dates=[], equity=[], trades=[], metrics=metrics.compute([]))

        cash = cfg.initial_cash
        qty: Dict[str, int] = {}
        last_close: Dict[str, float] = {}
        pending: Optional[Dict[str, float]] = None  # symbol -> target notional

        if hasattr(strategy, "initialize"):
            strategy.initialize(Context(days[0], bars, cash, qty, cash) if days else None)

        dates: List[dt.date] = []
        equity_curve: List[float] = []
        trades: List[Trade] = []

        for d in days:
            # 1. Fill yesterday's decision at today's open.
            if pending is not None:
                cash, day_trades = self._execute(pending, d, cash, qty)
                trades.extend(day_trades)

            # 2. Mark to market at today's close.
            for s in qty:
                b = bars.bar_on(s, d)
                if b is not None:
                    last_close[s] = b.close
            equity = cash + sum(q * last_close.get(s, 0.0) for s, q in qty.items())
            dates.append(d)
            equity_curve.append(round(equity, 4))

            # 3. Decide for tomorrow using only data <= d.
            ctx = Context(d, bars, cash, qty, equity)
            weights = strategy.on_bar(ctx) or {}
            pending = {s: w * equity for s, w in weights.items()}

        m = metrics.compute(equity_curve)
        bench_curve = self._benchmark_curve(days)
        bench_return = (bench_curve[-1] / bench_curve[0] - 1.0) if len(bench_curve) >= 2 and bench_curve[0] > 0 else 0.0
        m["benchmark_return"] = bench_return
        m["excess_return"] = m["total_return"] - bench_return
        m["information_ratio"] = metrics.information_ratio(equity_curve, bench_curve)
        m["turnover"] = self._turnover(trades, equity_curve)

        return BacktestResult(dates=dates, equity=equity_curve, trades=trades, metrics=m)

    def _benchmark_curve(self, days: List[dt.date]) -> List[float]:
        """Daily equal-weight index of the universe — the bar a strategy must
        clear to claim it added value. Each symbol is normalized to its own first
        traded close, so it's survivorship-correct (names enter at their IPO)."""
        first_close: Dict[str, float] = {}
        for s in self._bars.symbols():
            w = self._bars.window(s, days[-1], 10 ** 9) if days else []
            if w and w[0].close > 0:
                first_close[s] = w[0].close
        curve: List[float] = []
        for d in days:
            ratios = [self._bars.bar_on(s, d).close / fc
                      for s, fc in first_close.items() if self._bars.bar_on(s, d)]
            curve.append(sum(ratios) / len(ratios) if ratios else (curve[-1] if curve else 1.0))
        return curve

    def _turnover(self, trades: List[Trade], equity_curve: List[float]) -> float:
        """Total traded notional divided by mean equity — how hard the book churned
        (and a proxy for how sensitive the result is to trading-cost assumptions)."""
        traded = sum(t.price * t.qty for t in trades)
        mean_eq = sum(equity_curve) / len(equity_curve) if equity_curve else 0.0
        return round(traded / mean_eq, 6) if mean_eq > 0 else 0.0

    def _vol_cap(self, bar) -> Optional[int]:
        """Max shares fillable on this bar given the participation cap; None when
        uncapped (max_participation <= 0)."""
        p = self._cfg.max_participation
        if p <= 0:
            return None
        return int(p * bar.volume)

    def _execute(self, target: Dict[str, float], d: dt.date, cash: float,
                 qty: Dict[str, int]):
        """Reconcile holdings toward ``target`` notional at day ``d``'s open.

        Sells run before buys so freed cash can fund the buys. Returns the new
        cash balance and the list of fills.
        """
        cfg = self._cfg
        trades: List[Trade] = []

        # Desired share counts at this open (board-lot rounded). Symbols held but
        # absent from the target liquidate to zero.
        symbols = set(target) | set(qty)
        desired: Dict[str, int] = {}
        for s in symbols:
            b = self._bars.bar_on(s, d)
            if b is None:
                desired[s] = qty.get(s, 0)  # can't trade a non-trading symbol
                continue
            desired[s] = rules.round_lot(target.get(s, 0.0) / b.open) if b.open > 0 else 0

        # Sells first.
        for s in sorted(symbols):
            cur = qty.get(s, 0)
            delta = desired[s] - cur
            if delta >= 0:
                continue
            b = self._bars.bar_on(s, d)
            price = round(b.open * (1.0 - cfg.slippage), 4)
            if not rules.can_sell_at(b, price, rules.board_limit_pct(s, cfg.limit_pct)):
                continue
            sell_qty = min(-delta, cur)  # sellable == everything held (T+1)
            cap = self._vol_cap(b)
            if cap is not None:
                sell_qty = min(sell_qty, cap)  # can't dump more than the market traded
            if sell_qty <= 0:
                continue
            notional = price * sell_qty
            comm = rules.commission(notional, cfg.commission_rate, cfg.commission_min)
            duty = rules.stamp_duty(notional, cfg.stamp_duty_rate, "sell")
            cash += notional - comm - duty
            qty[s] = cur - sell_qty
            if qty[s] == 0:
                del qty[s]
            trades.append(Trade(d, s, "sell", sell_qty, price, comm, duty))

        # Buys.
        for s in sorted(symbols):
            cur = qty.get(s, 0)
            delta = desired[s] - cur
            if delta <= 0:
                continue
            b = self._bars.bar_on(s, d)
            price = round(b.open * (1.0 + cfg.slippage), 4)
            if not rules.can_buy_at(b, price, rules.board_limit_pct(s, cfg.limit_pct)):
                continue
            buy_qty = delta
            cap = self._vol_cap(b)
            if cap is not None:
                buy_qty = min(buy_qty, rules.round_lot(cap))  # can't absorb > the day's volume
            # Shrink to what cash (notional + commission) allows, in whole lots.
            while buy_qty > 0:
                notional = price * buy_qty
                comm = rules.commission(notional, cfg.commission_rate, cfg.commission_min)
                if notional + comm <= cash + 1e-9:
                    break
                buy_qty -= rules.LOT
            if buy_qty <= 0:
                continue
            notional = price * buy_qty
            comm = rules.commission(notional, cfg.commission_rate, cfg.commission_min)
            cash -= notional + comm
            qty[s] = cur + buy_qty
            trades.append(Trade(d, s, "buy", buy_qty, price, comm, 0.0))

        return cash, trades
