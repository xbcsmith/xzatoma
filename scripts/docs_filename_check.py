#!/usr/bin/env python3
"""
docs_filename_check.py

Validate documentation filenames under the repository `docs/` directory.

Checks performed:
- Markdown filenames (".md") must be lowercase with underscores and match: `^[a-z0-9_]+\.md$`
  - Exception: `README.md` is allowed (exact match).
  - Disallowed: hyphens (kebab-case), CamelCase, spaces, and other non-alphanumeric/underscore characters.
- Markdown extension must be lowercase `.md` (reject `.MD`, `.Md`, etc.).
- YAML files must use `.yaml` extension (reject `.yml`).
- Directory path components under `docs/` should not contain uppercase characters.
- Prints a concise, actionable report and exits with:
    0 - all checks passed
    1 - one or more violations found
    2 - fatal error (invalid docs root)

This is intentionally conservative and meant to be used as a lightweight, pre-merge validation
tool for documentation changes. It complements other scripts such as `doc_link_check.py`
and `emoji_check.py`.
"""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple

# Pattern to validate markdown basenames (except README.md)
MD_BASENAME_RE = re.compile(r"^[a-z0-9_]+\.md$")

README_FILENAME = "README.md"
YML_EXT = ".yml"
YAML_EXT = ".yaml"
MD_EXT = ".md"


def find_docs_root(provided: str | None = None) -> Path:
    """
    Determine the docs root path. By default, this is ../docs relative to this script.
    """
    if provided:
        docs = Path(provided)
    else:
        # Default: repository's docs/ sibling to this script's parent directory
        docs = Path(__file__).resolve().parent.parent / "docs"
    return docs


def check_path_component_case(component: str) -> bool:
    """
    Returns True if the path component contains uppercase letters.
    """
    return any(ch.isupper() for ch in component)


def analyze_file(path: Path, docs_root: Path) -> List[str]:
    """
    Analyze a single file and return a list of issues detected for this file.
    """
    issues: List[str] = []
    rel = path.relative_to(docs_root)
    rel_str = str(rel)

    # Check directory components for uppercase characters (not allowed)
    for comp in rel.parts[:-1]:  # directories only
        if check_path_component_case(comp):
            issues.append(f"directory contains uppercase characters: '{comp}'")

    name = path.name
    suffix = path.suffix  # includes leading dot, e.g., '.md'

    # Markdown checks
    if suffix.lower() == MD_EXT:
        # Enforce lowercase extension
        if suffix != MD_EXT:
            issues.append(f"extension must be lowercase '{MD_EXT}' (found '{suffix}')")

        if name == README_FILENAME:
            # README.md is allowed to be uppercase in the basename (exact match)
            return issues

        # Basename must match the allowed pattern (lowercase, digits, underscores)
        if not MD_BASENAME_RE.match(name):
            # Provide helpful diagnostics
            if " " in name:
                issues.append("filename contains spaces; use underscores instead")
            if "-" in name:
                issues.append("filename contains hyphen(s); use underscores instead")
            if any(ch.isupper() for ch in name):
                issues.append("filename contains uppercase letters; use lowercase only")
            # If none of the above matched, give a generic message
            if not any(
                [
                    "spaces" in msg or "hyphen" in msg or "uppercase" in msg
                    for msg in issues
                ]
            ):
                issues.append(
                    "filename must be lowercase with underscores and contain only "
                    "letters, digits, and underscores (e.g., 'my_doc_page.md')"
                )

    # YAML checks: reject `.yml` in favor of `.yaml`
    elif suffix.lower() == YML_EXT:
        issues.append(
            "YAML files must use the '.yaml' extension (rename from .yml → .yaml)"
        )

    elif suffix.lower() == YAML_EXT:
        # Enforce lowercase `.yaml` (rare edge-case where extension case mismatched)
        if suffix != YAML_EXT:
            issues.append(f"extension must be lowercase '{YAML_EXT}' (found '{suffix}')")

    # Other files are ignored (images, attachments, etc.)

    return issues


def find_violations(docs_root: Path, verbose: bool = False) -> Dict[Path, List[str]]:
    """
    Walk docs_root and collect filename violations.
    """
    if not docs_root.exists() or not docs_root.is_dir():
        raise FileNotFoundError(f"docs root not found: {docs_root}")

    violations: Dict[Path, List[str]] = {}

    for p in docs_root.rglob("*"):
        if not p.is_file():
            continue
        # We only validate markdown/yaml conventions; ignore other file types except to
        # check path casings (directories will be checked per-file as we iterate files).
        suffix = p.suffix.lower()
        if suffix not in (MD_EXT, YML_EXT, YAML_EXT):
            # Still detect uppercase directories via analyze_file (it will check dir parts)
            issues = analyze_file(p, docs_root)
            if issues and verbose:
                violations[p] = issues
            continue

        issues = analyze_file(p, docs_root)
        if issues:
            violations[p] = issues

    return violations


def print_report(violations: Dict[Path, List[str]]) -> None:
    if not violations:
        print("Docs filename check: OK — no filename or extension issues found.")
        return

    total = len(violations)
    print(f"Docs filename check: {total} file(s) with issues:\n")
    for path, issues in sorted(violations.items(), key=lambda kv: str(kv[0])):
        print(f"- {path}:")
        for msg in issues:
            print(f"    - {msg}")
    print()
    print(
        "Summary: Please rename files to follow 'lowercase_with_underscores.md' "
        "and use '.yaml' for YAML files. Exception: 'README.md' is allowed."
    )


def parse_args(argv: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Validate documentation filenames and extensions under docs/."
    )
    default_docs = os.path.normpath(
        os.path.join(os.path.dirname(__file__), "..", "docs")
    )
    parser.add_argument(
        "docs_root",
        nargs="?",
        default=default_docs,
        help=f"Path to the docs/ directory (default: {default_docs})",
    )
    parser.add_argument(
        "--verbose", "-v", action="store_true", help="Show additional diagnostics"
    )
    return parser.parse_args(argv)


def main(argv: List[str]) -> int:
    args = parse_args(argv)
    try:
        docs_root = Path(find_docs_root(args.docs_root)).resolve()
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 2

    try:
        violations = find_violations(docs_root, verbose=args.verbose)
    except FileNotFoundError as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 2
    except Exception as e:
        print(f"Unexpected error while scanning docs/: {e}", file=sys.stderr)
        return 2

    if violations:
        print_report(violations)
        return 1

    print_report(violations)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
