"""Replay a disclosed holdings timeline through the honest backtest engine.

A ``ReplayStrategy`` turns a public track record — a sequence of (date, target
weights) disclosures — into a strategy the engine can run: on each day it returns
the most recently disclosed weights as of that day (point-in-time; a future
rebalance never leaks in). The engine then prices it honestly (real bars, T+1,
price limits, costs, volume cap), so our independently-computed return can be
compared against whatever return was *claimed*.

Use it to recompute a fund's disclosed quarterly holdings, a 雪球 portfolio's
rebalance history, or positions someone posted — and expose the gap.
"""
from __future__ import annotations

import datetime as dt
from typing import Dict, List, Tuple

Timeline = List[Tuple[dt.date, Dict[str, float]]]


class ReplayStrategy:
    def __init__(self, timeline: Timeline):
        # Sort by date once so 'most recent on/before today' is a simple scan.
        self._timeline: Timeline = sorted(timeline, key=lambda e: e[0])

    def on_bar(self, ctx) -> Dict[str, float]:
        today = ctx.today
        current: Dict[str, float] = {}
        for day, weights in self._timeline:
            if day <= today:
                current = weights
            else:
                break
        return dict(current)
