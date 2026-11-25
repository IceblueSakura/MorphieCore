# Documentation Guide (`docs/`, `README.md`)

Applies to `docs/` and `README.md` (inherits [AGENTS.md](../AGENTS.md)).

## Single authoritative home

- Follow [README](../README.md#documentation-map) and keep one authoritative owner per topic; link instead of duplicating specs or maintaining parallel plans.
- Separate proposed design, accepted decisions, implementation status, and acceptance evidence (a structural edit grants no approval).

## No ephemeral archives

- Keep [Development](development.md) as a concise overall acceptance summary without run archives, historical run classifications, execution dates, tested-version inventories, acceptance commands, or per-run evidence catalogs.
- Do not retain phase-specific review pages or add run reports/history pages.

## Lifecycle and domain organization

- Organize each document by domain with shared rules above specialized branches.
- Keep unresolved issues in [Unscheduled issues](roadmap/unscheduled-issues.md#unscheduled-issues), accepted adaptations in [Architecture](architecture/index.md), and dated research notes indexed in [References](references.md#research-notes).
- Remove finished slices from [Roadmap](roadmap/index.md); record consequential trade-offs in [Settled decisions](architecture/decisions.md), not routine model or dialect changes.

## Verification

- Doc-only edits need [documentation checks](development.md#documentation-checks) only:
  ```sh
  nix develop .#docs --offline --command python3 tests/check_docs.py
  git diff --check
  ```
- Note: `tests/check_docs.py` scans git-tracked Markdown files (`git ls-files`), so stage new Markdown files (`git add <paths>`) before running it.
