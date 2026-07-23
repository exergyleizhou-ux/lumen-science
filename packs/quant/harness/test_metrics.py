"""Tests for deterministic performance metrics computed from an equity curve.

Expected values are hand-computed so the test pins the exact conventions:
  * daily simple returns
  * Sharpe = mean(r) / sample_std(r, ddof=1) * sqrt(252), risk-free = 0
  * max drawdown = min over t of equity[t]/running_peak - 1
"""
import pytest

import metrics


def test_flat_curve_is_all_zeros():
    m = metrics.compute([100.0, 100.0, 100.0])
    assert m["total_return"] == 0.0
    assert m["sharpe"] == 0.0
    assert m["max_drawdown"] == 0.0


def test_total_return():
    m = metrics.compute([100.0, 110.0, 121.0])
    assert m["total_return"] == pytest.approx(0.21)


def test_max_drawdown_exact():
    # peak 120 then trough 90 -> 90/120 - 1 = -0.25
    m = metrics.compute([100.0, 120.0, 90.0, 110.0])
    assert m["max_drawdown"] == pytest.approx(-0.25)


def test_sharpe_matches_hand_computation():
    # returns: +0.10, -0.05, +0.10
    # mean=0.05, sample_std=sqrt(0.0075)=0.0866025, daily sharpe=0.5773503
    # annualized = *sqrt(252)=9.16515
    m = metrics.compute([100.0, 110.0, 104.5, 114.95])
    assert m["sharpe"] == pytest.approx(9.16515, abs=1e-4)


def test_single_point_curve_is_safe():
    m = metrics.compute([100.0])
    assert m["total_return"] == 0.0
    assert m["sharpe"] == 0.0
    assert m["max_drawdown"] == 0.0


def test_information_ratio_matches_hand_computation():
    # strategy daily returns: +0.10, 0.00 ; benchmark: +0.02, -0.02
    # excess: +0.08, +0.02 -> mean 0.05, sample_std sqrt(0.0018)=0.0424264
    # IR = 0.05/0.0424264 * sqrt(252) = 18.7088
    equity = [100.0, 110.0, 110.0]
    benchmark = [100.0, 102.0, 99.96]
    assert metrics.information_ratio(equity, benchmark) == pytest.approx(18.7088, abs=1e-3)


def test_information_ratio_safe_on_degenerate_input():
    assert metrics.information_ratio([100.0], [100.0]) == 0.0
    assert metrics.information_ratio([100.0, 110.0], [100.0]) == 0.0  # length mismatch
    # identical curves -> zero excess variance -> 0, not a div-by-zero
    assert metrics.information_ratio([100.0, 110.0, 121.0], [100.0, 110.0, 121.0]) == 0.0
