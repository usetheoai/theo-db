---
slug: llm-endpoint-ssrf-hardening
milestone_id: M134
date: 2026-07-21
reviewers: council-security (×2, independent passes), council-rust-pgrx, cross-validation
---

# Review — M134 / #117 LLM-endpoint SSRF hardening

**Verdict:** READY_TO_MERGE

Commits: `f34ec2a` (feature) → `2d66e3d` (security fixes) → `5e7f69c` (evidence refresh) → `775a9d3` (pgrx review
items) → `bb17167` (proof gaps).
Evidence: `docs/benchmarks/m134-ssrf-hardening.md`. Plan: `.claude/knowledge-base/plans/llm-endpoint-ssrf-hardening-plan.md`.

## Severity matrix

| Sev | Finding | Source | State |
|---|---|---|---|
| BLOCKER | F1 — `endpoint_host` parsed the URL per RFC while `minreq` does not implement userinfo and silently falls back to port 80, so `http://169.254.169.254:x@api.openai.com/v1` dialed the metadata service. The guard was checking a host the client never contacts — and my own test had blessed the wrong model. | council-security #1 | **FIXED** (`egress.rs` now mirrors minreq's parser; exploit payloads are regression tests) — re-verified by an independent council-security pass over 47 adversarial URL shapes: **zero mismatches** |
| MEDIUM | F2 — classifier missed NAT64 `64:ff9b::/96` (reaches 169.254.169.254 on IPv6-only cloud hosts), 6to4, Teredo, `::a.b.c.d`, site-local, multicast, CGNAT `100.64/10`, `198.18/15`, `192.0.0/24`, `240/4` | council-security #1 | **FIXED**; arithmetic re-verified by execution, not inspection |
| MEDIUM | F3 — `theodb_ml.apply_model` now needs superuser; the tempting SECURITY DEFINER "fix" would reopen #117 in full | council-security #1 | **DOCUMENTED** as load-bearing in the SQL, the COMMENT and the CHANGELOG |
| LOW | F5 — the error was an internal name→IP oracle | council-security #1 | **FIXED** — resolved address moved to the server log |
| LOW/INFO | F4 (allowlisting a name delegates to DNS), F6 (`Suset` ≠ superuser-only in PG15+) | council-security #1 | **ACCEPTED**, stated in the evidence's honest limits |
| — | pgrx safety: unwind-vs-longjmp, blocking DNS, `log!`, 11 static GucSettings, `to_string_lossy` | council-rust-pgrx | **SOUND** — 0 BLOCKER/0 HIGH; corrected my model (pgrx 0.19 converts ERROR to `panic_any`, so frames unwind and destructors run). `panic = "unwind"` documented as a correctness dependency |
| MEDIUM | breaker fail-fast weakened to "no TCP" (an open breaker pays one DNS lookup) | council-rust-pgrx | **ACCEPTED + measured** (~0.26 ms literal, ~0.6 % of a real call); a TTL negative-cache is state nobody asked for |
| HIGH ×2 + MEDIUM ×4 | Proof drift: five acceptance criteria weakened or dropped between plan and evidence (42501 SQLSTATE, allowlist-scoping half, multi-A demonstration, latency "measured", REVOKE grep), plus a silent reversal of the plan's `*_model` decision | cross-validation | **ALL CLOSED** in `bb17167` with measurements; `*_model` reverted to caller-settable |

## What was measured on the shipped `.so`

Every restart passed the anti-silent-restart gate (`pg_postmaster_start_time > .so mtime`).

- 11 denial cases across loopback/RFC1918/link-local/CGNAT/multicast/userinfo-confusion — all SQLSTATE 22023.
- `42501` on non-superuser SET of `theodb.llm_endpoint` / `llm_api_key` / `egress_allowlist`.
- GUC contexts: **7 superuser** (3 endpoints, 3 api keys, `llm_test_model`) / **3 user** (the model names).
- Allowlist is scoped: `127.0.0.1` → 38000 (reaches network), `127.0.0.2` → 22023 (still refused).
- Multi-A name resolving to `{10.0.0.7, 8.8.8.8}` → refused.
- Real endpoint: embed ×2 and chat all `t` — no regression.
- 5/5 pure policy tests pass standalone (`rustc --test src/egress.rs`).
- clippy clean on the three touched files.

## Honest gaps carried into the release

1. **DNS-rebinding TOCTOU** — resolve-and-check, not resolve-then-connect. `minreq` exposes no connector hook;
   pinning it out is a dependency decision, not a bug fix. Disclosed at the call site and in the evidence.
2. **The three `#[pg_test]` cases have never executed** — `cargo pgrx test` does not link on this droplet. The DoD
   line "the new tests pass on the droplet build" is therefore **unmet as written**, and is recorded as unmet
   rather than reworded. The pure policy suite plus the in-PG matrix carry the proof.
3. **F4/F6** as above.

## Hard gates

| Gate | State |
|---|---|
| Tests green on the branch | yes (5/5 pure; in-PG matrices green) |
| No secrets committed | verified — no `sk-`/`Bearer`/key material in any file or message |
| No commit on `main` | all work on `develop` |
| No `Co-Authored-By` trailer | verified across all commits |
| CHANGELOG updated | yes — 2 Added, 2 Changed (BREAKING scope corrected) |
