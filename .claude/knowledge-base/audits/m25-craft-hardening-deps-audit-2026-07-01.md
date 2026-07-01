# Deps Audit: m25-craft-hardening

**Date:** 2026-07-01 · **Mode:** plan-bound:m25-craft-hardening · **Verdict:** PASS · **Hard caps:** []

## Summary
M25 is a **pure behavior-preserving refactor** (blueprint Corner 2, ADR-1). The plan's `## Dependencies § New`
is `(none)` — no crate added, no version bump, `Cargo.toml` untouched. Every fix is a code move / visibility
widening / function extraction / named const. There is **no new dependency surface to audit**.

- Ecosystem: Rust (pgrx 0.16.1 — existing, unchanged).
- New deps: 0. Removed: 0. CVE surface: unchanged from the released v0.24.0 baseline.

## Verdict
PASS — nothing to scan (no new/changed dependency). Proceed to `/plan-confidence` (already SHIPPABLE 95.6).
