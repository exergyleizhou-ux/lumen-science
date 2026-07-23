import pytest
from mathutil.divide import safe_divide

def test_normal_division():
    assert safe_divide(10, 2) == 5

def test_divide_by_zero():
    assert safe_divide(10, 0) == 0
