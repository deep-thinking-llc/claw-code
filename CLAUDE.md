# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Detected stack
- Languages: Rust.
- Frameworks: none detected from the supported starter markers.

## Verification
- Run Rust verification from `src/`: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`

## Repository shape
- `src/` contains the Rust workspace and active CLI/runtime implementation.
- `.github/scripts/check_doc_source_of_truth.py` is the remaining Python-based maintenance check in the active tree.

## Working agreement
- Prefer small, reviewable changes and keep generated bootstrap files aligned with actual repo workflows.
- Keep shared defaults in `.claude.json`; reserve `.claude/settings.local.json` for machine-local overrides.
- Do not overwrite existing `CLAUDE.md` content automatically; update it intentionally when repo workflows change.
- **Design/implementation docs**: WIP docs use `_` prefix and are gitignored (`_*.md`, `_docs/`). Drop the `_` to commit. See `AGENTS.md` for the full naming convention.
