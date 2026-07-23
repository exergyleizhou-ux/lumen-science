import pytest, tempfile, os
from fileutil.reader import safe_read

def test_normal_read():
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "hello.txt")
        with open(p, "w") as f:
            f.write("world")
        assert safe_read(d, "hello.txt") == "world"

def test_traversal_rejected():
    with tempfile.TemporaryDirectory() as d:
        result = safe_read(d, "../etc/passwd")
        assert result is None
