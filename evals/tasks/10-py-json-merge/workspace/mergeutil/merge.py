def merge_dicts(a, b):
    """Merge b into a. b's values win on conflict."""
    result = dict(a)
    # BUG: this does nothing — b is not merged
    return result
