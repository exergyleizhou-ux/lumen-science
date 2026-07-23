"""Tests for the akshare data fetcher's pure parts: column normalization,
exchange-suffix inference, and explicit-universe parsing. The network call
itself is integration-only (exercised by `lumen quant data`)."""
import pandas as pd

import fetch


def test_normalize_maps_chinese_columns_to_canonical_rows():
    # akshare stock_zh_a_hist schema (note: 收盘 comes before 最高/最低).
    df = pd.DataFrame({
        "日期": ["2024-01-03", "2024-01-02"],  # deliberately out of order
        "开盘": [10.0, 9.5],
        "收盘": [10.8, 10.0],
        "最高": [11.0, 10.2],
        "最低": [9.9, 9.4],
        "成交量": [1200, 1000],
    })
    rows = fetch.normalize_rows("600519.SH", df)
    assert [r["date"] for r in rows] == ["2024-01-02", "2024-01-03"]  # sorted
    assert rows[0] == {
        "date": "2024-01-02", "symbol": "600519.SH",
        # volume converted 手 -> shares (1000 手 * 100 = 100000 shares)
        "open": 9.5, "high": 10.2, "low": 9.4, "close": 10.0, "volume": 100000.0,
    }


def test_exchange_suffix_inference():
    assert fetch.with_suffix("600519") == "600519.SH"
    assert fetch.with_suffix("000858") == "000858.SZ"
    assert fetch.with_suffix("300750") == "300750.SZ"
    assert fetch.with_suffix("688981") == "688981.SH"
    assert fetch.with_suffix("430047") == "430047.BJ"
    # 5-digit codes are Hong Kong (port through 港股通)
    assert fetch.with_suffix("00700") == "00700.HK"
    assert fetch.with_suffix("00883") == "00883.HK"
    # already-suffixed passes through unchanged
    assert fetch.with_suffix("600519.SH") == "600519.SH"


def test_hk_volume_not_converted():
    # HK 成交量 is already in shares (not 手), so no x100.
    df = pd.DataFrame({"日期": ["2024-01-02"], "开盘": [280.0], "收盘": [283.0],
                       "最高": [291.0], "最低": [281.0], "成交量": [23354069]})
    rows = fetch.normalize_rows("00700.HK", df)
    assert rows[0]["volume"] == 23354069.0


def test_is_hk_detection():
    assert fetch.is_hk("00700.HK")
    assert fetch.is_hk("00700")
    assert not fetch.is_hk("600519.SH")
    assert not fetch.is_hk("600519")


def test_bare_code_for_akshare_strips_suffix():
    assert fetch.bare_code("600519.SH") == "600519"
    assert fetch.bare_code("000858.SZ") == "000858"
    assert fetch.bare_code("600519") == "600519"
    assert fetch.bare_code("00700.HK") == "00700"


def test_resolve_explicit_symbol_list():
    assert fetch.resolve_universe("600519.SH, 000858.SZ") == ["600519.SH", "000858.SZ"]
    # a bare code gets a suffix inferred
    assert fetch.resolve_universe("600519,300750") == ["600519.SH", "300750.SZ"]
