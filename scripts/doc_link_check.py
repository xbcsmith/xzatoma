#!/usr/bin/env python3
"""
doc_link_check.py

Small, conservative internal Markdown link checker for the `docs/` directory.

- Walks all `.md` files under `docs/`.
- Finds Markdown links of the form: [text](target)
- Skips external links (http(s)://, mailto:, tel:) and pure anchors (#...)
- Resolves relative links relative to the source file and checks target existence
- Tolerant: if a link points to a directory, tries `README.md` inside it
- Exits with status code 1 if any broken links are found; prints a summary otherwise.

Limitations:
- Does not validate internal anchors (e.g. `file.md#some-heading`) vs actual headers.
- The link parsing is a heuristic using regex and will not cover every Markdown edge-case.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from typing import List, Tuple

LINK_RE = re.compile(r"\[([^\]]+)\]\(([^)]+)\)")

EXTERNAL_SCHEMES = ("http://", "https://", "mailto:", "tel:", "ftp://", "data:")


def is_external_link(target: str) -> bool:
    t = target.strip().lower()
    return t.startswith(EXTERNAL_SCHEMES)


def normalize_target_path(target: str) -> str:
    # Remove any query or fragment portion for file existence checks; keep only file part
    if "#" in target:
        target = target.split("#", 1)[0]
    if "?" in target:
        target = target.split("?", 1)[0]
    return target


def resolve_candidate(src_dir: str, target: str, repo_docs_root: str) -> List[str]:
    """
    Return a list of candidate filesystem paths to check for existence.

    We try a few heuristics:
    - relative path (from src_dir)
    - if target is directory, look for README.md in it
    - if given without extension, try adding .md
    - absolute path (starting with /) is treated as repo-root relative
    """
    candidates: List[str] = []
    t = target.strip()

    if not t:
        return candidates

    # Skip anchor-only links earlier, but guard here too
    if t.startswith("#"):
        return candidates

    t = normalize_target_path(t)

    # Absolute-style: treat leading slash as repo root (i.e., path relative to repo)
    if t.startswith("/"):
        candidate = os.path.normpath(os.path.join(os.path.dirname(repo_docs_root), t.lstrip("/")))
        candidates.append(candidate)
    else:
        # relative to source file
        candidate = os.path.normpath(os.path.join(src_dir, t))
        candidates.append(candidate)

    # If the target is a directory (or looks like one), try README.md inside it
    for c in list(candidates):
        if os.path.isdir(c):
            candidates.append(os.path.join(c, "README.md"))

    # If no extension provided, attempt to add .md (and .yaml, .json for some references)
    new_candidates = []
    for c in candidates:
        base, ext = os.path.splitext(c)
        if ext == "":
            new_candidates.extend([c + ".md", c + ".yaml", c + ".json"])
        new_candidates.append(c)
    candidates = list(dict.fromkeys(new_candidates))  # de-duplicate, preserving order

    return candidates


def find_broken_links(docs_root: str) -> List[Tuple[str, str, str, str]]:
    """
    Returns list of tuples: (source_file, link_text, link_target, resolved_candidate)
    for every link that could not be resolved to an existing file.
    """
    broken = []
    docs_root = os.path.normpath(docs_root)

    for dirpath, dirnames, filenames in os.walk(docs_root):
        for name in filenames:
            if not name.endswith(".md"):
                continue
            full_path = os.path.join(dirpath, name)
            rel_src = os.path.relpath(full_path, docs_root)
            with open(full_path, "r", encoding="utf-8") as fh:
                try:
                    text = fh.read()
                except Exception as e:
                    print(f"WARN: Could not read {full_path}: {e}", file=sys.stderr)
                    continue

            for m in LINK_RE.finditer(text):
                link_text = m.group(1)
                target = m.group(2).strip()
                # Skip external links and anchors
                if is_external_link(target) or target.startswith("#"):
                    continue

                # Resolve candidate paths
                candidates = resolve_candidate(dirpath, target, docs_root)
                exists = False
                resolved_existing = ""
                for cand in candidates:
                    if os.path.exists(cand):
                        exists = True
                        resolved_existing = cand
                        break

                if not exists:
                    # Use the first candidate as the resolved path for reporting
                    resolved_path = candidates[0] if candidates else target
                    broken.append((full_path, link_text, target, resolved_path))

    return broken


def main(argv: List[str]) -> int:
    p = argparse.ArgumentParser(description="Simple internal Markdown link checker for docs/")
    p.add_argument(
        "docs_root",
        nargs="?",
        default=os.path.join(os.path.dirname(__file__), "..", "docs"),
        help="Path to the docs/ directory (defaults to repository's docs/).",
    )
    p.add_argument("--verbose", "-v", action="store_true", help="Print more detail")
    args = p.parse_args(argv)

    docs_root = os.path.normpath(os.path.abspath(args.docs_root))
    if not os.path.isdir(docs_root):
        print(f"ERROR: docs root not found: {docs_root}", file=sys.stderr)
        return 2

    if args.verbose:
        print(f"Checking Markdown links under: {docs_root}")

    broken = find_broken_links(docs_root)
    if broken:
        print("Broken links found:")
        for src, text, target, resolved in broken:
            print(f"- {src}: [{text}]({target}) -> {resolved}")
        print(f"\nTotal broken links: {len(broken)}")
        return 1

    print("No broken internal links found (checked docs/).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
