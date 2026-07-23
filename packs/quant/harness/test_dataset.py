"""Tests for the CSV dataset loader and its content hash.

The loader turns a pinned daily-bar CSV into a Bars store, deriving each
session's prev_close from the prior session (the reference for price limits).
``dataset_hash`` is a canonical content hash — the cert's data_hash — and must
be identical for the same data regardless of row order or cosmetic whitespace.
"""
import datetime as dt

import dataset


CSV = """date,symbol,open,high,low,close,volume
2024-01-02,A,10.0,10.5,9.9,10.2,1000
2024-01-03,A,10.2,11.0,10.1,10.8,1200
2024-01-02,B,20.0,20.4,19.8,20.1,800
2024-01-03,B,20.1,20.9,20.0,20.7,900
"""


def test_load_csv_builds_bars_with_derived_prev_close(tmp_path):
    p = tmp_path / "data.csv"
    p.write_text(CSV)
    bars = dataset.load_csv(str(p))

    a = bars.window("A", dt.date(2024, 1, 3), 10)
    assert [b.close for b in a] == [10.2, 10.8]
    # first session's prev_close falls back to its own open; second uses prior close
    assert a[0].prev_close == 10.0
    assert a[1].prev_close == 10.2


def test_dataset_hash_is_stable_across_loads(tmp_path):
    p = tmp_path / "data.csv"
    p.write_text(CSV)
    assert dataset.dataset_hash(str(p)) == dataset.dataset_hash(str(p))


def test_dataset_hash_ignores_row_order_and_whitespace(tmp_path):
    p1 = tmp_path / "a.csv"
    p1.write_text(CSV)
    # same data, rows shuffled and extra blank line + trailing spaces
    shuffled = (
        "date,symbol,open,high,low,close,volume\n"
        "2024-01-03,B,20.1,20.9,20.0,20.7,900\n"
        "2024-01-02,A,10.0,10.5,9.9,10.2,1000  \n"
        "\n"
        "2024-01-03,A,10.2,11.0,10.1,10.8,1200\n"
        "2024-01-02,B,20.0,20.4,19.8,20.1,800\n"
    )
    p2 = tmp_path / "b.csv"
    p2.write_text(shuffled)
    assert dataset.dataset_hash(str(p1)) == dataset.dataset_hash(str(p2))


def test_dataset_hash_changes_when_a_value_changes(tmp_path):
    p1 = tmp_path / "a.csv"
    p1.write_text(CSV)
    p2 = tmp_path / "b.csv"
    p2.write_text(CSV.replace("10.8", "10.9"))
    assert dataset.dataset_hash(str(p1)) != dataset.dataset_hash(str(p2))
