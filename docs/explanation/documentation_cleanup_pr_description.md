# PR: Archive historical implementation summaries (move-only)

Suggested PR title:
docs(archive): move historical implementation summaries to docs/archive/implementation_summaries (move-only)

## Short summary

This is a move-only documentation change that archives developer-focused implementation logs,
phase reports, and deep implementation notes into `docs/archive/implementation_summaries/`.
No substantive content edits were made to the archived documents; only file moves and
small link updates required to preserve navigation and prevent broken references.

## Key changes (high level)

- Created archive directory:
  - `docs/archive/implementation_summaries/`
  - `docs/archive/README.md` (archive policy & guidance)
- Moved developer-oriented files from `docs/explanation/` → `docs/archive/implementation_summaries/`
  - Phase reports (`phase1_*` … `phase7_*`)
  - Provider internals and fixes (`copilot_*`, `ollama_*`)
  - Planning notes, checklists, and developer notes (`notes*.md`, `implementation_plan_refactoring_summary.md`, etc.)
- Added `docs/explanation/documentation_cleanup_summary.md` (audit of moved files and rationale)
- Updated internal links:
  - `docs/explanation/*` references to archived files now point to `../archive/implementation_summaries/<file>.md`
  - Files inside the archive reference other archived files using local filenames (`<file>.md`)
- Added a short archival note to `docs/explanation/implementations.md` pointing to the archive
- No functional code changes

## Rationale

- Improve developer UX for new users: keep `docs/` user-facing (tutorials/how-to/reference/explanation) and
  move low-value-for-most-users implementation noise into an explicit archive.
- Conservative move-only approach preserves full Git history and allows maintainers to review moves
  before any consolidation or deletion.

## What to review

- Confirm this is strictly a move-only change set (no content deletions or structural rewrites).
- Verify all links to moved files have been updated and resolve properly:
  - From `docs/explanation/` → `../archive/implementation_summaries/<file>.md`
  - From within `docs/archive/implementation_summaries/` → local `<file>.md` where appropriate
  - From docs that should remain in `docs/explanation/` (e.g., `overview.md`, `implementations.md`) ensure they still live at the top-level explanation area
- Validate the `docs/archive/README.md` content and the archival policy (maintainer decision: keep forever / move-and-delete-after-signoff / archival TTL)
- Confirm no user-facing docs were accidentally removed or truncated
- Confirm file names follow `lowercase_with_underscores.md`

## Local validation (recommended)

Run these locally before approving:

- Basic grep for lingering explanation references:

```bash
# from repo root
grep -R "docs/explanation/" docs/ || true
```

- Quick markdown link check (example script; adapt to your environment or use your favorite link-checker):

```bash
# simple python link-checker (example)
python3 - <<'PY'
import os,re,sys
bad=[]
for root,_,files in os.walk("docs"):
  for fn in files:
    if not fn.endswith(".md"): continue
    path=os.path.join(root,fn)
    with open(path,'r',encoding='utf-8') as fh:
      text=fh.read()
    for m in re.findall(r'\]\(([^)]+)\)', text):
      if m.startswith('http') or m.startswith('#') or m.startswith('mailto:'): continue
      tgt=os.path.normpath(os.path.join(root,m.split()[0]))
      if not os.path.exists(tgt):
        bad.append((path,m,tgt))
if bad:
  for p,m,t in bad: print("MISSING:",p,"->",m,"(resolved",t,")")
  sys.exit(1)
print("NO_MISSING_LINKS")
PY
```

- Filename policy:

```bash
# should return nothing if all filenames are lowercase/no capitals
find docs/ -type f -name '*[A-Z]*' -print || true
```

- Emoji scan (no emojis in docs):

```bash
# basic check for emoji-range characters (may need to adapt for your environment)
grep -R --color='auto' -P "[\x{1F600}-\x{1F64F}]" docs/ || true
```

- Spot-check archive index and summary:

```bash
# confirm summary exists and points to moved files
sed -n '1,240p' docs/explanation/documentation_cleanup_summary.md
ls -la docs/archive/implementation_summaries | wc -l
```

## Review checklist (to be completed by reviewer)

- [ ] Confirm moved files are present in `docs/archive/implementation_summaries/`
- [ ] Confirm `docs/explanation/implementations.md` contains an archival note
- [ ] Confirm `docs/explanation/documentation_cleanup_summary.md` lists and justifies the moves
- [ ] Check for broken links introduced by move (use the link-checker above)
- [ ] Verify filenames follow `lowercase_with_underscores.md` rules
- [ ] No emojis in documentation
- [ ] No substantive edits to archived documents (moves only; minor link updates permitted)
- [ ] Confirm maintainers agree on archive policy (retain / delete-with-signoff / TTL)
- [ ] Accept the PR if all checks pass and archive policy is clear

## Notes about remaining missing links

- The link checker will report a small set of missing files that are intentional placeholders for future docs:
  - `docs/tutorials/quickstart.md`
  - `docs/how-to/configure_providers.md`
  - `docs/reference/cli.md`
  - `docs/reference/configuration.md`
  - `docs/reference/workflow_format.md`
  - `docs/reference/api.md`
    These are planned for Phase 3 (quickstart, provider how-to, CLI references). The PR is safe to merge despite these planned missing pages; they will be created in a follow-up PR.

## Suggested merge strategy & commit message

- Keep moves as separate commits where possible to preserve history (recommend one commit per logical group if you want granularity).
- Do NOT squash-merge if you want to preserve per-file git history; if the project's policy prefers squashes, maintainers should be aware that rename history is harder to inspect after a squash.
- Suggested commit message (conventional):
  - `chore(docs): move historical implementation summaries to docs/archive/implementation_summaries (move-only)`
- Suggested PR labels: `docs`, `chore`, `documentation`

Post-merge recommendations (Phase 2+)

- Maintainers confirm archive policy (retain vs delete rule).
- Reclassify and reassign misfiled documents (how-to / reference) in follow-up PR(s).
- Add lightweight docs CI checks:
  - filename policy (lowercase + underscores)
  - markdown link checker on changed docs
  - emoji scan
  - code-fence language enforcement
- Create high-priority missing docs (quickstart, configure_providers, CLI reference) in Phase 3.

## PR: Reclassify & Consolidate (Phase 2) — Suggested PR description

Suggested PR title:
docs(reclassify): reclassify misfiled docs & consolidate provider docs (Phase 2)

Short summary:
This PR performs Phase 2 of the documentation cleanup: reclassifies short how-to snippets into `docs/how-to/`, promotes quick references into `docs/reference/`, canonicalizes the implementation plan filename to `docs/explanation/implementation_plan.md`, and consolidates user-facing provider configuration instructions into `docs/how-to/configure_providers.md`. Small content merges (for example, provider/model prompt display into `use_chat_modes.md`) are included to improve discoverability. Provider internals remain archived for history and developer reference.

Key changes (high level):

- Move: `docs/explanation/provider_abstraction_quick_reference.md` → `docs/reference/provider_abstraction.md`
- Merge: `docs/explanation/chat_mode_provider_display.md` → `docs/how-to/use_chat_modes.md` (archive original)
- Rename: `docs/explanation/implementation_plan_refactored.md` → `docs/explanation/implementation_plan.md` and include a short refactor summary (full audit retained in the archive)
- Create: `docs/how-to/configure_providers.md`
- Move: `docs/explanation/implementation_quickstart_checklist.md` → `docs/how-to/implementation_quickstart_checklist.md`
- Update index & links: `docs/explanation/implementations.md`, `docs/README.md`, and `docs/explanation/documentation_cleanup_summary.md` updated to reflect these changes

Testing & validation:

- Run the doc link-checker across `docs/` to ensure no regressions introduced by reclassification.
- Verify that all moved/renamed references were updated and resolve from their new locations.
- Ensure filenames and YAML extensions conform to AGENTS.md rules.
- Confirm archived internals were preserved (moved, not deleted).

What to review:

- Confirm the reclassifications align with maintainers' expectations for Diataxis placement.
- Check that links resolve and there are no accidental content deletions.
- Confirm the canonical `implementation_plan.md` contains a short refactor summary and links to the archived full plan.
- Prefer move-only commits where possible; small merge edits were made only to combine short how-to fragments into canonical pages.

Suggested commit message:

- `chore(docs): reclassify & consolidate docs (Phase 2)`

Suggested reviewers & labels:

- `docs`, `chore`, `documentation`, `review-needed`

## References

- Audit & moved-file list: `docs/explanation/documentation_cleanup_summary.md`
- Archive README & policy: `docs/archive/README.md`
- Implementation index note: `docs/explanation/implementations.md`

If you want, I can:

- Prepare a small, copy-ready PR description and checklist (this file can be used as the PR body),
- Split the move set into smaller per-area commits if you prefer per-feature review,
- Draft the Phase 2 change list (reclassifications & renames) and a suggested CI workflow for docs checks.

Thank you — this change preserves historical implementation detail, makes the top-level docs more approachable, and leaves a clear audit trail for maintainers to review before any further consolidation.
