# docs/archive — Implementation summaries archive

## Overview

This directory contains archived developer-focused implementation summaries, phase reports, and internal notes that were moved out of `docs/explanation/` as part of a documentation cleanup (Phase 1). The goal of the archive is to preserve historical implementation detail while keeping the main `docs/` tree user-facing and discoverable.

Files in this directory are preserved and traceable through Git history. Moves were performed as move-only changes (no content deletions or large rewrites were performed as part of Phase 1).

## Archive policy (recommended)

- Move-only by default: Files are relocated to `docs/archive/implementation_summaries/` using move-only PRs (i.e., `git mv`) and small link updates. Avoid changing substantive content during the move.
- Retention: Archive items are retained indefinitely until maintainers approve removal or consolidation. Deletion or consolidation requires a separate PR with explicit justification and maintainer sign-off.
- Restoration: Restoring a file to `docs/explanation/` (or another canonical location) must be done via a PR that moves the file back and updates links, with a clear rationale for why it should be promoted.
- Governance: Policy decisions (retain vs. delete vs. consolidate) are a maintainer-level decision and should be documented in an issue or PR comment.

## How to reference archived files

- From files under `docs/explanation/` use a relative link to the archive:

 - Example:
  ```text
  Link text: Phase 3 security implementation
  Path: ../archive/implementation_summaries/phase3_security_validation_implementation.md
  ```

- From an archived file to an explanation doc use:

 - Example:
  ```text
  Link text: Chat modes architecture
  Path: ../../explanation/chat_modes_architecture.md
  ```

- When linking across archived files inside the archive folder prefer local filenames (no directories). Note: from this README (located at `docs/archive/`) prefix with `implementation_summaries/`:
 - Example:
  ```text
  Link text: Copilot model fetching
  Path: implementation_summaries/copilot_dynamic_model_fetching.md
  ```
 - From a file inside `docs/archive/implementation_summaries/` you can use:
  ```text
  Link text: Copilot model fetching
  Path: copilot_dynamic_model_fetching.md
  ```

## Move-only PR guidance (recommended checklist)

1. Use `git mv` to move files (preserves history).
2. Update referencing files to point to the new locations:
  - `docs/explanation/*` → `../archive/implementation_summaries/<file>.md`
  - `docs/archive/implementation_summaries/*` linking to each other → local filenames (`<file>.md`)
3. Do not delete or substantially edit the moved files in the same PR.
4. Include a short audit in the PR description that lists moved files and rationale (for example: `docs/explanation/documentation_cleanup_summary.md`).
5. Run the validation steps below (link checks, filename checks, emoji scan).
6. Request maintainer review and add the `documentation` or `docs` label as appropriate.

## Validation (suggested checks to run locally or in CI)

- Search for remaining references to the old locations:

 ```bash
 # find any lingering references to docs/explanation/
 grep -R "docs/explanation/" docs/ || true
 ```

- Confirm new archive links exist:

 ```bash
 grep -R "\.\./archive/implementation_summaries/" docs/ || true
 ```

- Ensure filenames follow lowercase_with_underscores:

 ```bash
 # should return nothing if names are conformant
 find docs/ -type f -name '*[A-Z]*' -print || true
 ```

- Recommended CI checks (if available):
 - Markdown link check (ensure internal links resolve)
 - Filename policy (lowercase + underscores)
 - Emoji scan (no emojis in docs)
 - Code-fence language enforcement (all fenced code blocks include a language)

## Restoring an archived file

If a file must be restored from the archive to a user-facing location:

1. Create a branch and use `git mv` to move the file back:
  ```bash
  git mv docs/archive/implementation_summaries/<file>.md docs/explanation/<file>.md
  ```
2. Update all references that pointed to the archived location so they now reference the explanation location.
3. Run validation checks (link-checker, filename policy, emoji scan).
4. Open a PR that explains why the document should be promoted and request maintainer review.

## Finding archived content

- List all archived implementation summaries:

 ```bash
 ls docs/archive/implementation_summaries
 ```

- Search across archived content:
 ```bash
 grep -R "security" docs/archive/implementation_summaries || true
 ```

## CI / long-term recommendations

- Add a lightweight docs CI workflow that runs the suggested validation checks on docs-only PRs (link-check, filename pattern, emoji scan, and code-fence language checks).
- Consider a small maintainer-owned process for periodic review of archived items (e.g., quarterly), especially when planning major documentation rewrites.

## Contact / escalation

If you believe a file should be restored, deleted, or consolidated, open an issue describing:

- The file(s) in question
- Why the change is needed
- Suggested new location and impact on readers

Maintainers will review and provide next steps. For Phase 1 moves, consult `docs/explanation/documentation_cleanup_summary.md` for the full audit and rationale of the moved files.
