"""Deterministic performance metrics from a daily equity curve.

Pure stdlib (math only) with fixed conventions so two runs on the same equity
curve produce bit-identical numbers — the property the backtest certificate
attests to. Annualization assumes 252 trading days; risk-free rate is 0.
"""
from __future__ import annotations

import math
from typing import Dict, List

TRADING_DAYS = 252


def _daily_returns(equity: List[float]) -> List[float]:
    out = []
    for prev, cur in zip(equity, equity[1:]):
        out.append(cur / prev - 1.0 if prev != 0 else 0.0)
    return out


def _sample_std(xs: List[float]) -> float:
    n = len(xs)
    if n < 2:
        return 0.0
    mean = sum(xs) / n
    var = sum((x - mean) ** 2 for x in xs) / (n - 1)
    return math.sqrt(var)


def _max_drawdown(equity: List[float]) -> float:
    peak = equity[0]
    worst = 0.0
    for v in equity:
        if v > peak:
            peak = v
        if peak > 0:
            dd = v / peak - 1.0
            if dd < worst:
                worst = dd
    return worst


def information_ratio(equity: List[float], benchmark: List[float]) -> float:
    """Annualized mean active return over its tracking error — the risk-adjusted
    measure of skill *versus the benchmark*. Returns 0.0 on degenerate input
    (too short, mismatched lengths, or zero active-return variance)."""
    if len(equity) < 2 or len(equity) != len(benchmark):
        return 0.0
    se = _daily_returns(equity)
    be = _daily_returns(benchmark)
    excess = [s - b for s, b in zip(se, be)]
    std = _sample_std(excess)
    if std <= 0:
        return 0.0
    mean = sum(excess) / len(excess)
    return mean / std * math.sqrt(TRADING_DAYS)


def compute(equity: List[float]) -> Dict[str, float]:
    """Return total_return, cagr, sharpe, sortino, max_drawdown for the curve."""
    if len(equity) < 2:
        return {
            "total_return": 0.0, "cagr": 0.0, "sharpe": 0.0,
            "sortino": 0.0, "max_drawdown": 0.0,
        }

    total_return = equity[-1] / equity[0] - 1.0
    rets = _daily_returns(equity)
    mean = sum(rets) / len(rets)

    std = _sample_std(rets)
    sharpe = (mean / std * math.sqrt(TRADING_DAYS)) if std > 0 else 0.0

    downside = [r for r in rets if r < 0]
    dstd = _sample_std(downside) if len(downside) >= 2 else 0.0
    sortino = (mean / dstd * math.sqrt(TRADING_DAYS)) if dstd > 0 else 0.0

    n_days = len(equity) - 1
    growth = equity[-1] / equity[0]
    cagr = growth ** (TRADING_DAYS / n_days) - 1.0 if growth > 0 else -1.0

    return {
        "total_return": total_return,
        "cagr": cagr,
        "sharpe": sharpe,
        "sortino": sortino,
        "max_drawdown": _max_drawdown(equity),
    }
