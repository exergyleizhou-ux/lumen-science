"""In-sandbox backtest runner — the entrypoint the Docker container executes.

Loads the pinned dataset and the author's ``strategy.py``, runs the deterministic
engine, and emits a results bundle whose ``data_hash`` + ``equity_curve_hash`` +
``metrics`` are the reproducible payload pinned by the VQ certificate. Running
the same package twice yields a bit-identical bundle.

CLI (run inside the sandbox):
    python run.py --data data.csv --strategy strategy.py --config config.json \
        --out results.json
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
from typing import Any, Dict

import dataset
from engine import BacktestConfig, Engine

ENGINE_VERSION = "quant-engine/0.1.0"


def _load_strategy(strategy_path: str):
    spec = importlib.util.spec_from_file_location("user_strategy", strategy_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    # Prefer a Strategy class (can hold state); fall back to the module itself
    # if it exposes a top-level on_bar function.
    if hasattr(module, "Strategy"):
        return module.Strategy()
    if hasattr(module, "on_bar"):
        return module
    raise ValueError("strategy.py must define a `Strategy` class or an `on_bar` function")


def _equity_curve_hash(dates, equity) -> str:
    h = hashlib.sha256()
    for d, v in zip(dates, equity):
        h.update(f"{d.isoformat()}|{v:.4f}\n".encode("utf-8"))
    return h.hexdigest()


def run_backtest(data_path: str, strategy_path: str, config: Dict[str, Any]) -> Dict[str, Any]:
    bars = dataset.load_csv(data_path)
    strategy = _load_strategy(strategy_path)
    cfg = BacktestConfig(**config)
    result = Engine(bars, cfg).run(strategy)

    return {
        "engine_version": ENGINE_VERSION,
        "data_hash": dataset.dataset_hash(data_path),
        "config": dict(sorted(config.items())),
        "dates": [d.isoformat() for d in result.dates],
        "equity": result.equity,
        "equity_curve_hash": _equity_curve_hash(result.dates, result.equity),
        "metrics": result.metrics,
        "n_trades": len(result.trades),
    }


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Run a pinned A-shares backtest.")
    ap.add_argument("--data", required=True)
    ap.add_argument("--strategy", required=True)
    ap.add_argument("--config", required=True, help="path to a JSON config file")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    with open(args.config) as f:
        config = json.load(f)
    res = run_backtest(args.data, args.strategy, config)
    with open(args.out, "w") as f:
        json.dump(res, f, indent=2, sort_keys=True)
    print(f"backtest complete: {res['metrics']}  data_hash={res['data_hash'][:12]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
