import pytest
from mergeutil.merge import merge_dicts

def test_merge_no_conflict():
    result = merge_dicts({"x": 1}, {"y": 2})
    assert result == {"x": 1, "y": 2}

def test_merge_override():
    result = merge_dicts({"x": 1, "y": 2}, {"y": 99})
    assert result == {"x": 1, "y": 99}

def test_empty():
    result = merge_dicts({}, {"a": 1})
    assert result == {"a": 1}
