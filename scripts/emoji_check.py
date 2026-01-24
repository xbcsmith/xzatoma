#!/usr/bin/env python3
"""
emoji_check.py

Small helper script to scan Markdown documentation for emoji characters.

Usage:
    python3 emoji_check.py [--docs-root PATH] [--include-archive] [--verbose] [--strict]

By default the scanner uses a relaxed emoji range (avoids flagging box-drawing and check marks).
Set `--strict` to use the legacy, broader ranges.

By default, the script scans the `docs/` directory that is a sibling of this
script's parent directory (i.e., ../docs relative to this file). It searches
all `.md` files recursively but skips files under `docs/archive/` by default.
Pass `--include-archive` to include archived documentation (developer notes)
in the scan.

Exit codes:
    0 - no emoji characters found
    1 - emoji characters were detected (and printed)
    2 - error (invalid docs root or other fatal issue)

Notes:
- This is intentionally conservative and does not attempt a perfect unicode
  emoji definition. It looks for characters in a broad set of emoji-related
  Unicode ranges (emoticons, symbols, pictographs, dingbats, flags, etc.).
- It does not scan files outside the `docs/` tree by default. That allows
  AGENTS.md to retain its visual markers if necessary while keeping the
  rest of the documentation emoji-free.
- By default the script also skips `docs/archive/` (historical implementation notes and logs). Use `--include-archive` to include archived files in the scan.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from typing import Dict, List, Tuple

# Two patterns are provided:
#  - RELAXED (default): captures common emoji/pictograph ranges but avoids
#    dingbats/technical ranges that commonly include box-drawing characters,
#    check marks, and similar glyphs used for formatting.
#  - STRICT (legacy): the original, broader set of ranges (use --strict to opt in).
_EMOJI_RE_RELAXED = re.compile(
    "["  # relaxed: common emoji/pictograph ranges only
    "\U0001F300-\U0001F5FF"  # symbols & pictographs
    "\U0001F600-\U0001F64F"  # emoticons
    "\U0001F680-\U0001F6FF"  # transport & map
    "\U0001F1E0-\U0001F1FF"  # flags
    "\U0001F900-\U0001F9FF"  # supplemental symbols & pictographs
    "\U0001FA70-\U0001FAFF"  # Symbols & Pictographs Extended-A
    "]+",
    flags=re.UNICODE,
)

# Legacy, broader pattern (matches the previous behavior)
_EMOJI_RE_STRICT = re.compile(
    "["  # begin char class
    "\U0001F300-\U0001F5FF"  # symbols & pictographs
    "\U0001F600-\U0001F64F"  # emoticons
    "\U0001F680-\U0001F6FF"  # transport & map
    "\U0001F1E0-\U0001F1FF"  # flags
    "\U00002702-\U000027B0"  # dingbats
    "\U000024C2-\U0001F251"
    "\U0001F900-\U0001F9FF"  # supplemental symbols & pictographs
    "\U0001FA70-\U0001FAFF"  # Symbols & Pictographs Extended-A
    "\U00002600-\U000026FF"  # Misc symbols
    "\U00002300-\U000023FF"  # Misc technical
    "]+",
    flags=re.UNICODE,
)


def find_emoji_in_text(text: str, strict: bool = False) -> List[Tuple[int, str]]:
    """
    Return a list of (index, match_text) for emoji-like matches in `text`.
    Index is the Python string index (character offset).

    If `strict` is True, the legacy broader pattern is used (includes dingbats
    and technical symbol ranges). By default (strict=False) a relaxed pattern
    is used to avoid flagging box-drawing and check marks.
    """
    pattern = _EMOJI_RE_STRICT if strict else _EMOJI_RE_RELAXED
    return [(m.start(), m.group(0)) for m in pattern.finditer(text)]


def scan_docs_for_emoji(docs_root: str, extensions: Tuple[str, ...] = (".md",), include_archive: bool = False, strict: bool = False) -> Dict[str, List[Tuple[int, str, str]]]:
    """
    Scan files under `docs_root` for emoji characters.

    Returns a mapping:
        { filepath: [ (line_number, matched_characters, line_text), ... ] }

    Args:
        strict: If True, use the legacy, broader emoji ranges (same as --strict).
    """
    results: Dict[str, List[Tuple[int, str, str]]] = {}

    docs_root = os.path.abspath(docs_root)
    if not os.path.isdir(docs_root):
        raise FileNotFoundError(f"docs root not found: {docs_root}")

    for dirpath, _, filenames in os.walk(docs_root):
        # Optionally skip archived documentation to avoid noisy historical artifacts
        rel_dir = os.path.relpath(dirpath, docs_root)
        if not include_archive and (rel_dir == "archive" or rel_dir.startswith("archive" + os.sep)):
            continue

        for fname in filenames:
            if not fname.lower().endswith(extensions):
                continue
            path = os.path.join(dirpath, fname)
            try:
                with open(path, "r", encoding="utf-8") as fh:
                    for i, line in enumerate(fh, start=1):
                        matches = find_emoji_in_text(line, strict=strict)
                        if matches:
                            # Concatenate all matched emoji sequences on the line for reporting
                            matched_texts = [m[1] for m in matches]
                            results.setdefault(path, []).append((i, "".join(matched_texts), line.rstrip("\n")))
            except UnicodeDecodeError:
                # Could not decode the file as UTF-8; warn and skip
                results.setdefault(path, []).append((0, "", "<binary-or-non-utf8-file>"))

    return results


def print_report(found: Dict[str, List[Tuple[int, str, str]]], verbose: bool = False) -> None:
    if not found:
        print("No emoji characters found in docs/ (checked .md files).")
        return

    total_files = len(found)
    total_occurrences = sum(len(v) for v in found.values())
    print(f"Emoji characters detected: {total_occurrences} occurrence(s) across {total_files} file(s).")
    print()
    for path, occurrences in sorted(found.items()):
        print(f"{path}:")
        for lineno, match, context in occurrences:
            if lineno == 0 and context == "<binary-or-non-utf8-file>":
                print(f"  [warning] Could not read file as UTF-8 which prevents emoji scanning.")
            else:
                # Show the matched characters in a readable form (unicode codepoints)
                codepoints = " ".join(f"U+{ord(ch):04X}" for ch in match)
                print(f"  Line {lineno}: {match!r} ({codepoints})")
                if verbose:
                    print(f"    {context}")
        print()


def parse_args(argv: List[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Scan docs/ for emoji characters.")
    default_docs = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", "docs"))
    p.add_argument("--docs-root", default=default_docs, help=f"Path to docs/ directory (default: {default_docs})")
    p.add_argument("--extensions", default=".md", help="Comma-separated file extensions to scan (default: .md)")
    p.add_argument("--include-archive", action="store_true", help="Include files under docs/archive/ in the scan (defaults to False)")
    p.add_argument("--strict", action="store_true", help="Use legacy strict emoji detection (includes dingbats and technical ranges). Default: relaxed detection to avoid flagging box-drawing and check marks.")
    p.add_argument("--verbose", "-v", action="store_true", help="Show matching line context for each occurrence")
    return p.parse_args(argv)


def main(argv: List[str]) -> int:
    args = parse_args(argv)
    docs_root = args.docs_root
    exts = tuple(e.strip().lower() for e in args.extensions.split(",") if e.strip())

    if args.verbose:
        if args.strict:
            print("Using strict emoji detection (--strict): legacy broad ranges enabled.")
        else:
            print("Using relaxed emoji detection (default): avoids flagging box-drawing and check marks.")

    try:
        found = scan_docs_for_emoji(docs_root, exts, include_archive=args.include_archive, strict=args.strict)
    except FileNotFoundError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 2

    if found:
        print_report(found, verbose=args.verbose)
        return 1
    else:
        print("No emoji characters found in docs/ (checked files).")
        return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
