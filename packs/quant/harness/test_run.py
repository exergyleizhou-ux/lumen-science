"""Tests for the in-sandbox runner: load strategy + data, run, emit results.

This is the layer the Docker sandbox invokes. Its output bundle (data_hash,
metrics, equity_curve_hash) is the reproducible payload the VQ certificate pins,
so the headline test is that two runs of the same package are bit-identical.
"""
import textwrap

import run


CSV = """date,symbol,open,high,low,close,volume
2024-01-02,A,10.0,10.0,10.0,10.0,1000000
2024-01-03,A,10.0,11.2,9.8,11.0,1000000
2024-01-04,A,11.0,12.2,10.8,12.0,1000000
2024-01-05,A,12.0,12.6,11.9,12.5,1000000
"""

STRATEGY = textwrap.dedent('''
    class Strategy:
        def on_bar(self, ctx):
            return {"A": 1.0}
''')

CONFIG = {"initial_cash": 100000.0, "commission_rate": 0.0, "commission_min": 0.0,
          "stamp_duty_rate": 0.0, "slippage": 0.0, "limit_pct": 0.10}


def _pkg(tmp_path):
    (tmp_path / "data.csv").write_text(CSV)
    (tmp_path / "strategy.py").write_text(STRATEGY)
    return str(tmp_path / "data.csv"), str(tmp_path / "strategy.py")


def test_run_backtest_returns_metrics_and_hashes(tmp_path):
    data_path, strat_path = _pkg(tmp_path)
    res = run.run_backtest(data_path, strat_path, CONFIG)
    assert res["data_hash"]
    assert res["equity_curve_hash"]
    assert res["metrics"]["total_return"] == 0.25  # 100k -> 125k buy-and-hold
    assert res["n_trades"] == 1
    assert "engine_version" in res


def test_run_is_reproducible(tmp_path):
    data_path, strat_path = _pkg(tmp_path)
    a = run.run_backtest(data_path, strat_path, CONFIG)
    b = run.run_backtest(data_path, strat_path, CONFIG)
    assert a["equity_curve_hash"] == b["equity_curve_hash"]
    assert a == b


def test_equity_curve_hash_changes_with_data(tmp_path):
    data_path, strat_path = _pkg(tmp_path)
    a = run.run_backtest(data_path, strat_path, CONFIG)
    (tmp_path / "data.csv").write_text(CSV.replace("12.5", "13.5"))
    b = run.run_backtest(data_path, strat_path, CONFIG)
    assert a["equity_curve_hash"] != b["equity_curve_hash"]
