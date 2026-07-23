"""Out-of-sample (walk-forward) evaluation — the overfit gate.

A strategy that only shines in-sample is overfit; the honest read is whether its
edge survives on data it didn't get to "see" when it was chosen. split_evaluate
runs the same strategy on an in-sample window [first, split] and a held-out
out-of-sample window (split, last], and reports both plus whether the excess
return over the benchmark persisted into the OOS window.
"""
from __future__ import annotations

import datetime as dt
from typing import Any, Dict

from engine import BacktestConfig, Engine
from data import Bars


def split_evaluate(bars: Bars, strategy, split_date: dt.date, cfg: BacktestConfig) -> Dict[str, Any]:
    days = bars.trading_days()
    is_res = Engine(bars, cfg).run(strategy, end=split_date)
    after = [d for d in days if d > split_date]
    oos_res = Engine(bars, cfg).run(strategy, start=after[0]) if after else None

    is_m = is_res.metrics
    oos_m = oos_res.metrics if oos_res else {}
    is_excess = is_m.get("excess_return", 0.0)
    oos_excess = oos_m.get("excess_return", 0.0)
    return {
        "split_date": split_date.isoformat(),
        "is": is_m,
        "oos": oos_m,
        "is_excess": is_excess,
        "oos_excess": oos_excess,
        # Skill should persist: a real edge beats the benchmark in BOTH windows.
        "persisted": is_excess > 0 and oos_excess > 0,
        "excess_decay": is_excess - oos_excess,
    }
