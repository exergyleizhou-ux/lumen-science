#!/usr/bin/env python3
"""Validate SPDX 2.3 documents with the official spdx-tools library."""

from __future__ import annotations

import sys
from pathlib import Path

from spdx_tools.spdx.parser.parse_anything import parse_file
from spdx_tools.spdx.validation.document_validator import validate_full_spdx_document


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: validate-spdx.py DOCUMENT...", file=sys.stderr)
        return 2
    failed = False
    for raw_path in sys.argv[1:]:
        path = Path(raw_path)
        try:
            document = parse_file(str(path))
            messages = validate_full_spdx_document(document, "SPDX-2.3")
        except Exception as exc:  # Parser failures are contract failures.
            print(f"FAIL: official SPDX parser rejected {path}: {exc}", file=sys.stderr)
            failed = True
            continue
        if messages:
            failed = True
            print(f"FAIL: official SPDX validator rejected {path}:", file=sys.stderr)
            for message in messages:
                print(f"  {message}", file=sys.stderr)
        else:
            print(f"OK: official SPDX 2.3 validator accepted {path}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
