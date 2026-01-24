#!/usr/bin/env python3
"""
code_fence_check.py

Scan Markdown files under `docs/` to ensure fenced code blocks include an explicit
language (or path) tag.

Rules enforced:
- Opening fenced code blocks using backticks (```) or tildes (~~~) must include
  a non-empty language/path token immediately after the fence.
  Examples:
    Good: ```rust
    Good: ```bash
    Good: ```/dev/null/example.rs#L1-10
    Bad:  ```
    Bad:  ```   (no token)
- Closing fences are matched and ignored for the 'missing language' rule.
- Unclosed fences are reported as an error.

Exit codes:
- 0  - No violations found
- 1  - Violations detected
- 2  - Fatal error (e.g., docs root not found)

This script is intentionally conservative and designed to be used as a lightweight
pre-merge check for documentation PRs. It complements other checks such as the
internal link checker and emoji scan.

Usage:
    python3 scripts/code_fence_check.py [--docs-root PATH] [--extensions .md,.markdown] [--verbose]

By default, it scans "../docs" relative to this script.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from typing import Dict, List, Tuple

# Regex to detect a fence line and capture
#  - group 1: fence characters (e.g., ``` or ~~~)
#  - group 2: the rest of the line after the fence
_FENCE_RE = re.compile(r"^\s*([`~]{3,})(.*)$")


def find_missing_language_tags(
    docs_root: str, extensions: Tuple[str, ...] = (".md",)
) -> Dict[str, List[Tuple[int, str]]]:
    """
    Scan files under `docs_root` for fenced code block openings that do not
    include a language/path tag.

    Returns a mapping:
        { filepath: [ (line_number, line_text), ... ] }
    """
    results: Dict[str, List[Tuple[int, str]]] = {}
    docs_root = os.path.abspath(docs_root)
    if not os.path.isdir(docs_root):
        raise FileNotFoundError(f"docs root not found: {docs_root}")

    for dirpath, _, filenames in os.walk(docs_root):
        for fname in filenames:
            lower = fname.lower()
            if not any(lower.endswith(ext) for ext in extensions):
                continue

            path = os.path.join(dirpath, fname)
            try:
                with open(path, "r", encoding="utf-8") as fh:
                    in_block = False
                    block_fence_char = ""
                    block_fence_seq = ""
                    block_open_line = 0

                    for lineno, line in enumerate(fh, start=1):
                        m = _FENCE_RE.match(line)
                        if m:
                            fence_seq = m.group(1)
                            fence_char = fence_seq[0]  # '`' or '~'
                            rest = m.group(2).strip()

                            if not in_block:
                                # Opening fence
                                # If the rest is empty, it's a violation (missing language)
                                if rest == "":
                                    results.setdefault(path, []).append(
                                        (lineno, line.rstrip("\n"))
                                    )
                                # Enter code block (even if language missing) to maintain state
                                in_block = True
                                block_fence_char = fence_char
                                block_fence_seq = fence_seq
                                block_open_line = lineno
                            else:
                                # Potential closing fence: verify it's the same fence character
                                # (we don't require the same length of the fence sequence)
                                if fence_char == block_fence_char:
                                    in_block = False
                                    block_fence_char = ""
                                    block_fence_seq = ""
                                    block_open_line = 0
                                # else: another fence-like line inside a block that uses a different char;
                                # treat it as literal content (no state change)
                        # plain line -> continue scanning
                    # After file ends, check for unclosed block
                    if in_block:
                        # Report as a distinct issue (use line number of the opening fence)
                        msg = f"<unclosed code block opened at line {block_open_line}>"
                        results.setdefault(path, []).append((block_open_line, msg))
            except UnicodeDecodeError:
                results.setdefault(path, []).append((0, "<binary-or-non-utf8-file>"))
            except Exception as e:
                results.setdefault(path, []).append((0, f"<error reading file: {e}>"))
    return results


def print_report(found: Dict[str, List[Tuple[int, str]]], verbose: bool = False) -> None:
    if not found:
        print("Code-fence language check: OK — all fenced code blocks include a language/path tag.")
        return

    total_files = len(found)
    total_issues = sum(len(v) for v in found.values())
    print(f"Code-fence language check: {total_issues} issue(s) across {total_files} file(s).")
    print()
    for path, occurrences in sorted(found.items()):
        print(f"{path}:")
        for lineno, snippet in occurrences:
            if lineno == 0:
                print(f"  [warning] {snippet}")
            else:
                print(f"  Line {lineno}: {snippet!r}")
                if verbose and lineno > 0:
                    # Optionally show a hint
                    print(f"    Hint: add a language or path after the opening fence, e.g., ``````rust`````` or ``````/dev/null/example.rs#L1-3``````")
        print()


def parse_args(argv: List[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Check that fenced code blocks include a language/path tag.")
    default_docs = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "docs"))
    p.add_argument("--docs-root", default=default_docs, help=f"Path to the docs/ directory (default: {default_docs})")
    p.add_argument(
        "--extensions",
        default=".md",
        help="Comma-separated file extensions to scan (default: .md). Example: .md,.markdown",
    )
    p.add_argument("--verbose", "-v", action="store_true", help="Show helpful hints and context for violations")
    return p.parse_args(argv)


def main(argv: List[str]) -> int:
    args = parse_args(argv)
    exts = tuple(e.strip().lower() for e in args.extensions.split(",") if e.strip())

    try:
        found = find_missing_language_tags(args.docs_root, exts)
    except FileNotFoundError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 2
    except Exception as e:
        print(f"ERROR: unexpected failure while scanning docs/: {e}", file=sys.stderr)
        return 2

    if found:
        print_report(found, verbose=args.verbose)
        return 1

    print_report(found, verbose=args.verbose)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
