import os

def safe_read(base_dir, rel_path):
    """Read file relative to base_dir. Return None if path traversal detected."""
    full = os.path.join(base_dir, rel_path)
    # BUG: no traversal check
    with open(full) as f:
        return f.read()
