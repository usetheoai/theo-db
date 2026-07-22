---
slug: llm-endpoint-ssrf-hardening
milestone_id: M134
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Feature grill — M134 fix #117 (blind SSRF via caller-settable `theodb.llm_endpoint`)

Answers synthesized from issue #117 (filed during M110) + the cited source lines, per the grill protocol's
"explore first" rule. User intent was explicit: create milestones for #132, #140, #117.

## Q1 — What is this feature and why NOW?

Fix **#117**: `theodb.llm_endpoint` is **not a registered GUC** — it is read via
`current_setting('theodb.llm_endpoint', true)` (`pg.rs:50-56`), i.e. a **placeholder GUC**, and in PostgreSQL *any*
role may `SET` a dotted-name placeholder for its own session. The only guards are scheme (`chat.rs:266-267`, must be
`http(s)://`) and no-redirect (`http.rs:124-125`). There is **no private-IP / loopback / link-local block**, and an
`http(s)`-only check is not an SSRF control.

Impact: a role holding EXECUTE on any LLM-touching function (`ai._chat`, `ai.extract_entities`, or the newer
`theodb.graph_upsert(..., use_llm := true)`) can point the **database host** at an arbitrary internal target —
cloud metadata `169.254.169.254`, `127.0.0.1`, internal services — and trigger a server-side request. Blind SSRF:
internal port scan via timing / circuit-breaker state, plus hits on unauthenticated internal endpoints.

**Why now:** it is a barrier to *any* multi-tenant or untrusted-role deployment, and M110 **widened** it by adding
`graph_upsert` as a second callable path. It is one of the three items on the shortest path from "strong engineering
artifact" to "operable by someone else".

## Q2 — Dependencies (which milestones must be [x])

- **M131** `[x]` — most recent completed milestone.
- **M110** `[x]` — the milestone that added the second reachable caller (`graph_upsert`) this must also cover.

All satisfied.

## Q3 — Definition of Done (verifiable)

1. `theodb.llm_endpoint` and `theodb.llm_api_key` are **registered custom GUCs with `GucContext::Suset`**
   (operator/superuser-settable only) — a non-superuser session can no longer `SET` them (asserted by a negative
   test that expects the error).
2. A **private/loopback/link-local denylist** is enforced before the POST for at least: `169.254.0.0/16`,
   `127.0.0.0/8`, `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `::1`, `fc00::/7`.
3. **Resolve-then-connect to the same IP** so a DNS-rebinding host (public A record that re-resolves to an internal
   address) cannot bypass the denylist.
4. Negative tests asserting the **specific typed error** for each blocked range **and** for a rebinding attempt —
   per `rules/testing.md § 4.1`, a negative-case test asserts the error and message, not merely "it throws".
5. The existing posture does **not** regress: `REVOKE … FROM PUBLIC`, no-redirect, and fail-fast all still hold
   (re-proven, matching the DoD line already in the roadmap's security section).

## Q4 — Top 2 NEW risks

1. **Resolve-then-connect requires pinning the resolved IP through the HTTP client.** Our client
   (`http.rs`) may not expose connection-level IP pinning, so the fix could need a client-level change (custom
   resolver / connector) rather than a check in `resolve_chat_cfg`. Mitigation: spike the client capability BEFORE
   committing to the approach; if pinning is infeasible, document the residual rebinding window honestly instead of
   claiming full coverage.
2. **An over-broad denylist breaks legitimate on-prem internal LLM endpoints.** Many self-hosted deployments run the
   model server on `10.x`/`192.168.x` by design — a blanket block would break exactly the operator we target.
   Mitigation: the denylist is the default, with an explicit **operator-only allowlist GUC** (also `Suset`) to
   re-permit specific internal hosts; the escape hatch must be operator-settable, never caller-settable.

## Prior art

- Issue #117 (repro with `169.254.169.254`, root-cause pointers `chat.rs:258-268`, `pg.rs:50`, `http.rs:124`).
- `theodb_rs/src/am/guc.rs` — the existing `define_custom_*_guc` registrations (only unrelated int/bool GUCs today).
- `rules/testing.md § 4.1` (negative cases assert the typed error), `rules/error-handling.md` (fail-fast, typed).
- The NL→SQL security posture (denylist + fail-closed + REVOKE) already applied in `nl.rs` — the model to follow.

## SOTA delta

None required — SSRF defense is well-established (deny private ranges + resolve-then-connect); no new reference
peers needed beyond the existing `knowledge-base/references/`.
