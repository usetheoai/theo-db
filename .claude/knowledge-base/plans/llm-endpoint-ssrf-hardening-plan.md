---
slug: llm-endpoint-ssrf-hardening
milestone_id: M134
created_at: 2026-07-21
goal: Close the blind SSRF by making the outbound endpoint operator-only (Suset GUCs) and denying private/loopback/link-local targets at the single HTTP egress point, with negative tests asserting the typed error
---

# Plan — M134 (#117): blind SSRF via caller-settable `theodb.llm_endpoint`

## Goal

Make the outbound LLM/embedding endpoint **operator-controlled** (registered `GucContext::Suset` GUCs instead of
caller-settable placeholders) and **deny private/loopback/link-local targets at the single HTTP egress point**, so a
role holding EXECUTE on an LLM-touching function can no longer point the database host at internal addresses.

**Single metric:** a non-superuser session can neither `SET theodb.llm_endpoint` nor reach an internal address —
asserted by negative tests that check the **specific typed error** for each blocked range, recorded in
`docs/benchmarks/m134-ssrf-hardening.md`.

## Context

Issue #117: `theodb.llm_endpoint` is **not a registered GUC** — it is read via
`current_setting('theodb.llm_endpoint', true)` (`pg.rs:50-56`), i.e. a **placeholder**, and in PostgreSQL any role
may `SET` a dotted-name placeholder for its own session. The only guards are scheme (`chat.rs:266-267`) and
no-redirect (`http.rs:124-125`). An `http(s)`-only check is **not** an SSRF control: it blocks `file://`, not
`http://169.254.169.254/`. M110 widened the surface by adding `theodb.graph_upsert(..., use_llm := true)` as a
second reachable caller.

## Baseline Context

Repo state: git sha `4a3ae65`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/am/guc.rs` | 250+ | registers int/bool GUCs via `GucRegistry::define_int_guc(…, GucContext::Suset, …)`; **no** string GUCs, none for the `llm_*`/`embedding_*` triples | Register the endpoint/api-key/model GUCs as `Suset` string GUCs |
| `theodb_rs/src/http.rs` | 190+ | `post_json` builds the `minreq` request; scheme + no-redirect are the only guards | Add the egress guard: resolve the host and deny if ANY resolved address is private/loopback/link-local |
| `theodb_rs/src/chat.rs` | 280+ | `resolve_chat_cfg` validates the scheme only | unchanged logic; the guard moves to the shared egress |
| `docs/benchmarks/m134-ssrf-hardening.md` | — | (NEW) | Evidence: blocked ranges, the typed errors, and the honest residual-window note |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/chat.rs:258` — `resolve_chat_cfg(model)` reads `guc("theodb.llm_endpoint")`; scheme-only check at `:266-267`.
- `theodb_rs/src/pg.rs:50` — `guc(name)` = `SELECT current_setting('<name>', true)` — reads a placeholder today.
- `theodb_rs/src/http.rs:108` — `post_json(fn_name, endpoint, payload, api_key)` — **the single outbound HTTP call** for both chat and embed; `minreq::post(endpoint)` at `:119`, `Authorization` at `:127`, `send()` at `:130`.
- `theodb_rs/src/embed.rs:174` — `resolve_cfg` reads `theodb.embedding_endpoint` — the **same** placeholder-GUC class.
- `theodb_rs/src/am/guc.rs:238` — `init()`, called from `lib.rs:25` (`am::guc::init()`).

### Domain glossary

- **placeholder GUC** — a dotted name PostgreSQL accepts without registration; **any** role may `SET` it for its session. Registering it with `GucContext::Suset` restricts `SET` to superuser/operator.
- **blind SSRF** — the response is not returned to the attacker, but timing / circuit-breaker state still leaks whether an internal target exists.
- **DNS rebinding** — a hostname that resolves to a public address at check time and an internal one at connect time.

### Architecture boundaries affected

Per `rules/architecture.md`: the network guard belongs at the **single egress choke point** (`http::post_json`), not
duplicated per caller. GUC registration joins the existing `am/guc.rs` registry. No new module, no API change.

## Prior Art & Related Work

- Issue #117 (repro against `169.254.169.254`, root-cause pointers).
- `theodb_rs/src/am/guc.rs:239-258` — the established `GucRegistry::define_*_guc(…, GucContext::Suset, GucFlags::default())` pattern (adopted from pgvectorscale, Rule 9).
- `theodb_rs/src/nl.rs` — the NL→SQL posture (denylist + fail-closed + REVOKE) this mirrors.
- `rules/testing.md § 4.1` — a negative-case test asserts the **specific typed error and message**, not merely "it throws".

## ADRs

### ADR M134-1 — the network guard lives at the shared egress, and covers the embedding endpoint too

**Decision:** implement the private-address denylist inside `http::post_json` (the single outbound call), not in
`resolve_chat_cfg`. This automatically covers the **embedding** endpoint, which has the identical placeholder-GUC
exposure, and registers **both** triples (`llm_*` and `embedding_*`) as `Suset`.

**Rationale (cites `rules/architecture.md` + the parsimony ladder):** one choke point cannot be bypassed by a future
caller; a per-caller check would have to be duplicated and would leave `embed` open — a known-identical hole it
would be dishonest to leave standing while claiming the SSRF class is closed.

**Alternatives rejected:**
- **Guard only `resolve_chat_cfg`** (the issue's literal scope) — REJECTED: leaves the embed path exposed to the same attack and duplicates the check for the next caller.
- **Rely on `REVOKE` alone** — REJECTED: REVOKE limits *who* can call, not *where* the database connects; defence in depth is the point.

### ADR M134-2 — resolve-and-check, with the residual rebinding window documented rather than hidden

**Decision:** resolve the endpoint host and reject if **any** resolved address is private/loopback/link-local, then
let `minreq` connect by hostname. Do **not** claim resolve-then-connect IP pinning.

**Rationale (Rule 3):** `minreq` (`Cargo.toml:25`) exposes no custom resolver or connector, so pinning the checked
IP through to the socket is not possible without replacing the HTTP client — a far larger change than this security
fix warrants. Checking *all* resolved addresses closes the static case; a DNS-rebinding attacker who flips the record
between our resolution and `minreq`'s retains a narrow window. That window is **stated in the evidence**, not papered
over.

**Alternatives rejected:** replace `minreq` with a client supporting custom connectors — REJECTED for this
milestone: a dependency swap on the hot embed path deserves its own measured milestone, and the denylist delivers
most of the protection now. Recorded as a follow-up.

## Dependencies

`## Dependencies`: **none new**. Uses `std::net::ToSocketAddrs` (stdlib — parsimony rung 2) and the existing pgrx
`GucRegistry`/`GucSetting`. `minreq` unchanged. No crate added.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Endpoint/api-key are operator-only (`Suset`), not caller-settable | T1.1 |
| Private/loopback/link-local targets denied at the egress | T2.1 |
| Negative tests assert the specific typed error per range | T2.2 |
| Operator-only allowlist so a legitimate internal endpoint can be re-permitted | T2.3 |
| Existing posture (REVOKE, no-redirect, fail-fast) does not regress | T3.1 |

## Phase 1 — make the endpoint operator-only

### T1.1 — register the `llm_*` and `embedding_*` GUCs as `Suset` string GUCs

#### Why this step
A placeholder GUC is settable by any role, which is the root of #117. Reasoning: register endpoint/api-key/model for
both triples via `GucRegistry::define_string_guc(…, GucContext::Suset, GucFlags::default())` in the existing
`am::guc::init()`, so `SET` requires superuser while `current_setting` reads keep working unchanged.

#### Files to edit
- `theodb_rs/src/am/guc.rs`.

#### TDD
- RED: `test_m134_llm_endpoint_is_not_caller_settable` — as a non-superuser role, `SET theodb.llm_endpoint = …` must raise (assert the error), while the same `SET` as superuser succeeds.
- GREEN: register the GUCs.
- REFACTOR: one helper for the six registrations.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `SELECT count(*) FROM pg_settings WHERE name IN ('theodb.llm_endpoint','theodb.llm_api_key','theodb.embedding_endpoint','theodb.embedding_api_key')` returns **4** (registered GUCs appear in `pg_settings`; placeholders do not).
- A non-superuser `SET theodb.llm_endpoint` raises `42501` (insufficient privilege), asserted by the test.

#### DoD
- `cargo build` exits 0; the test passes.

## Phase 2 — deny internal targets at the egress

### T2.1 — private/loopback/link-local denylist in `http::post_json`

#### Why this step
ADR M134-1: the scheme check is not an SSRF control. Reasoning: before issuing the request, parse the host from the
endpoint, resolve it with `ToSocketAddrs`, and reject with a typed input error if **any** resolved address is
loopback, private, link-local, or unique-local — covering `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`,
`192.168.0.0/16`, `169.254.0.0/16`, `::1`, `fc00::/7`.

#### Files to edit
- `theodb_rs/src/http.rs`.

#### TDD
- RED: `test_m134_denies_link_local_metadata` — an endpoint of `http://169.254.169.254/latest/meta-data/` raises the typed error and **no request is issued**.
- GREEN: implement the guard.
- REFACTOR: the address classifier is a pure function so every range is unit-testable without network.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- **DNS resolution fails** (unknown host): reject with a typed error — fail-closed, never "resolve failed so allow".
- **A hostname resolves to several addresses, one internal**: rejected (the check is over ALL resolved addresses, not the first).
- **A legitimate on-prem endpoint is internal**: denied by default; the operator re-permits it via the allowlist GUC (T2.3 rationale below) — the escape hatch is operator-only, never caller-settable.

#### Acceptance criteria
- `test_m134_classifier_blocks_all_private_ranges` asserts `is_blocked_addr` returns true for `127.0.0.1`, `10.0.0.1`, `172.16.0.1`, `192.168.0.1`, `169.254.169.254`, `::1`, `fd00::1` and false for `8.8.8.8` — 7 blocked, 1 allowed, `cargo test` exits 0.
- `post_json` with endpoint `http://169.254.169.254/latest/meta-data/` raises SQLSTATE `22023` whose message contains `blocked internal address`, asserted by `test_m134_denies_link_local_metadata`.

#### DoD
- Tests green; `cargo build` exits 0.

### T2.2 — negative tests assert the specific typed error

#### Why this step
`rules/testing.md § 4.1`: a negative-case test asserts the error **and message**, not merely that it throws.
Reasoning: assert SQLSTATE `22023` and that the message names the blocked address class, so a future refactor that
turns the hard failure into a silent skip fails the suite.

#### Files to edit
- `theodb_rs/src/http.rs` (test module).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- Each negative test asserts `ERRCODE_INVALID_PARAMETER_VALUE` (22023) AND a message substring naming the blocked class — `assert!(msg.contains("blocked internal address"))` — so a refactor that downgrades the hard failure to a silent skip fails the suite.

#### DoD
- Tests green.

### T2.3 — operator-only allowlist for legitimate internal endpoints

#### Why this step
Mitigates the HIGH risk in the table: many self-hosted deployments run the model server on `10.x`/`192.168.x` by
design, so a blanket denylist would break exactly the operator this product targets. Reasoning: a `Suset` string GUC
holding a comma-separated host list; the egress guard consults it **after** classifying an address as internal, so
re-permitting is a deliberate operator act and never a caller's.

#### Files to edit
- `theodb_rs/src/am/guc.rs`; `theodb_rs/src/http.rs`.

#### TDD
- RED: `test_m134_allowlist_permits_only_listed_host` — with the allowlist containing `10.1.2.3`, that host passes while `10.1.2.4` is still blocked.
- GREEN: implement the allowlist consult.
- REFACTOR: keep the allowlist parse pure (comma-split + trim) so it is testable without a GUC.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `test_m134_allowlist_permits_only_listed_host` asserts an allowlisted internal host is permitted AND a non-listed internal host in the same range is still blocked; `cargo test` exits 0.
- The allowlist GUC is registered `GucContext::Suset` (appears in `pg_settings`, non-superuser `SET` raises insufficient privilege).

#### DoD
- Test green; the GUC is operator-only.

## Phase 3 — prove no regression

### T3.1 — existing posture re-proven

#### Why this step
The roadmap's security DoD line requires `REVOKE … FROM PUBLIC` + SSRF/no-redirect/fail-fast to keep holding.
Reasoning: re-run the existing REVOKE assertions and confirm a normal public endpoint still works end-to-end (the
embed path must not be broken by the new guard).

#### Files to edit
- `docs/benchmarks/m134-ssrf-hardening.md`; `CHANGELOG.md`.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `SELECT length(theodb.embed('hello world')::text) > 100` returns true against the real public endpoint after the guard is active (the denylist does not block legitimate egress).
- `cargo test` exits 0 with the pre-existing REVOKE assertions unchanged (`grep -c "FROM PUBLIC" theodb_rs/src/vectorizer.rs` unchanged from before the change).

#### DoD
- Evidence written; CHANGELOG updated; #117 closed with the evidence.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| An over-broad denylist breaks legitimate on-prem internal LLM endpoints (many self-hosts run the model server on `10.x`/`192.168.x`) | HIGH | Ship an operator-only (`Suset`) allowlist GUC so an internal host can be re-permitted deliberately; document it. The escape hatch must never be caller-settable | engine |
| DNS rebinding between our resolution and `minreq`'s | MEDIUM | Check ALL resolved addresses (closes the static case); the residual window is **documented**, not hidden (ADR M134-2). Replacing the HTTP client for true pinning is a recorded follow-up | engine |
| Registering previously-placeholder GUCs breaks a deployment that `SET` them per session as a non-superuser | MEDIUM | That is exactly the vulnerability being closed; call it out in the CHANGELOG as an intentional behaviour change, with `ALTER SYSTEM` as the supported path | engine |
| Resolution adds latency to every outbound call | LOW | One `ToSocketAddrs` per request against an already ~100 ms+ HTTP round-trip; measured in the evidence rather than assumed | benchmarks |

## Unresolved Questions

- Should the allowlist be per-host or per-CIDR? Starting per-host (exact match) — narrower, and CIDR can follow if an operator needs a range.
- Should `theodb.llm_model` also be `Suset`? It is not a network vector (it only flows into the JSON body), so it stays caller-settable unless evidence says otherwise.

## Failure scenarios

- **The guard blocks a legitimate public endpoint** (misclassification): the live embed check in T3.1 fails, and the milestone does not ship until the classifier is corrected — a false positive here breaks the product's core path.
- **`ToSocketAddrs` hangs on a hostile DNS server**: bounded by the existing per-request timeout budget; recorded honestly if resolution latency proves material.
- **A caller sets the endpoint before the GUCs are registered** (extension not loaded): `current_setting` returns NULL and the existing "endpoint is not set" typed error fires — fail-closed.

## Global Definition of Done

- [ ] `SELECT count(*) FROM pg_settings WHERE name IN ('theodb.llm_endpoint','theodb.llm_api_key','theodb.embedding_endpoint','theodb.embedding_api_key')` returns **4**, and a non-superuser `SET theodb.llm_endpoint` raises insufficient-privilege (asserted).
- [ ] The address classifier blocks a representative address of all seven ranges (`127/8`, `10/8`, `172.16/12`, `192.168/16`, `169.254/16`, `::1`, `fc00::/7`) — pure unit test, no network.
- [ ] An endpoint of `http://169.254.169.254/latest/meta-data/` raises the typed error (SQLSTATE + message asserted) **before any socket is opened**.
- [ ] A live embed against the real public endpoint still succeeds after the guard (the denylist does not break legitimate egress).
- [ ] `cargo build` exits 0 and the new tests pass on the droplet build.
- [ ] `docs/benchmarks/m134-ssrf-hardening.md` records the blocked ranges, the typed errors, and the **residual DNS-rebinding window**; CHANGELOG updated; #117 closed with the evidence.

## Final Phase — Integration Validation

- `cargo build` + tests green on the droplet; restart with the postmaster-start-time gate asserted.
- Live check: metadata address blocked, real endpoint still works.
- council-security review (is the class actually closed, or only the named instance?).
