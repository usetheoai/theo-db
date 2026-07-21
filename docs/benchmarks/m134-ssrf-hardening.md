# M134 — #117: closing the LLM-endpoint SSRF class (measured)

> Verified 2026-07-21 on the self-hosted droplet (165.227.121.20, PG17.10 pgrx-install, `theodb_rs` rebuilt with the
> M134 changes). This milestone measures **behaviour** (what the database refuses to call), not performance.
> Plan: `.claude/knowledge-base/plans/llm-endpoint-ssrf-hardening-plan.md`.
> Every restart below passed the anti-silent-restart gate — `pg_postmaster_start_time > .so mtime` asserted before
> any reading was trusted (`GATE ok — binário novo carregado (1784650097 > 1784650095)`).

## Headline

A database that makes outbound HTTP calls on behalf of SQL callers is an SSRF engine unless two things are true:
the **target is not caller-controlled**, and **internal addresses are refused**. Before M134, neither held.

| | Before M134 | After M134 (measured) |
|---|---|---|
| Who may set `theodb.embedding_endpoint` | **any role** (unregistered placeholder GUC) | superuser only — `ERROR: permission denied to set parameter` |
| `http://169.254.169.254/` (cloud metadata) | request issued by the DB host | `ERROR: 22023 … refusing to call blocked internal address` |
| `http://10.0.0.1/`, `127.0.0.1`, `192.168.x`, `172.16.x`, `::1`, `0.0.0.0` | request issued | refused, address named |
| `http://localhost:9/` (name, not literal) | request issued | refused — resolution happens **before** the check |
| Unresolvable host | connection attempted | refused (fail-closed) |
| Legitimate on-prem endpoint on 10/8 | worked | works via `theodb.egress_allowlist` (superuser-only) |
| Public endpoint (`api.openai.com`) | worked | **still works** — no regression |

## 1. The pre-existing check was not an SSRF control (honest framing)

`resolve_chat_cfg` / `resolve_embed_cfg` already rejected non-`http(s)` schemes. That blocks `file://` — it does
**nothing** about `http://169.254.169.254/`. The milestone's premise is that scheme validation was mistaken for
egress control. Both are now present and they are different checks.

## 2. T1.1 — the endpoint is no longer caller-controlled (measured)

Eleven GUCs registered with `GucContext::Suset`; the three key-bearing ones additionally carry
`GUC_SUPERUSER_ONLY` so Postgres hides the value from non-superusers in `pg_settings`.

```
           name            |  context
---------------------------+-----------
 theodb.egress_allowlist   | superuser
 theodb.embedding_api_key  | superuser
 theodb.embedding_endpoint | superuser
 theodb.embedding_model    | superuser
 theodb.llm_api_key        | superuser
 theodb.llm_endpoint       | superuser
 theodb.llm_model          | superuser
 theodb.llm_test_model     | superuser
 theodb.rerank_api_key     | superuser
 theodb.rerank_endpoint    | superuser
 theodb.rerank_model       | superuser
(11 rows)
```

As an unprivileged role:

```
SET ROLE m134_caller;                         -- current_user = m134_caller
SET theodb.embedding_endpoint = 'http://169.254.169.254/';
ERROR:  permission denied to set parameter "theodb.embedding_endpoint"
SET theodb.egress_allowlist = 'evil.example';
ERROR:  permission denied to set parameter "theodb.egress_allowlist"
```

The escape hatch cannot be widened by the party it constrains — that is the property that makes it an escape hatch
rather than a bypass.

## 3. T2.1/T2.2 — the denylist, at the single egress choke point (measured)

The guard lives in `http::post_json`, the **one** outbound call shared by embed, chat and rerank. A per-caller
check would have left whichever caller was added next unguarded; this cannot be bypassed by a future caller.

All eight refusals below carry SQLSTATE **22023** (`invalid_parameter_value`) and name the resolved address:

```
ERROR:  22023: theodb.embed: refusing to call blocked internal address 169.254.169.254 (host 169.254.169.254) — loopback/private/link-local targets are denied; an operator may permit a specific host via theodb.egress_allowlist
        … 127.0.0.1 (host 127.0.0.1)
        … 10.0.0.1 (host 10.0.0.1)
        … 172.16.0.1 (host 172.16.0.1)
        … 192.168.0.1 (host 192.168.0.1)
        … ::1 (host ::1)
        … ::1 (host localhost)          ← a NAME, resolved first, then judged
        … 0.0.0.0 (host 0.0.0.0)
```

The `localhost` line is the one that matters most: a check that only pattern-matched IP literals in the URL would
have let it through. The guard resolves first and judges the **resolved addresses**, so a DNS name pointing at an
internal host is refused exactly like the literal.

The chat path is covered by the same code, with the caller's own function name in the message:

```
ERROR:  ai._chat: refusing to call blocked internal address 169.254.169.254 (host 169.254.169.254) — …
```

**Fail-closed on unknown**, because "we could not check it" is not "it is safe":

```
ERROR:  theodb.embed: could not resolve endpoint host m134-no-such-host.invalid: failed to lookup address
        information: Name or service not known (blocked internal address check cannot run)
```

The guard also runs **before** the circuit breaker, so a blocked target never records breaker state. That ordering
is deliberate: breaker state is observable through timing, and writing it for an internal target would leak whether
that host is alive — the blind-SSRF signal the denylist exists to remove.

## 4. T2.3 — the operator escape hatch works (measured)

Hardening that breaks a legitimate on-prem deployment gets disabled wholesale, which is worse than the class it
closes. With the host named in the superuser-only allowlist, the call reaches the network layer — the failure is
now the connection (**38000**), not the guard (22023):

```
SET theodb.egress_allowlist = '127.0.0.1, other.example';
WARNING:  theodb.embed: endpoint connection error; retrying (1/2): Connection refused (os error 111)
WARNING:  theodb.embed: endpoint connection error; retrying (2/2): Connection refused (os error 111)
ERROR:  38000: theodb.embed: endpoint call failed: Connection refused (os error 111)
```

## 5. T3.1 — no regression against the real endpoint (measured)

Against `https://api.openai.com`, with the allowlist empty:

| Check | Result |
|---|---|
| `length(theodb.embed('hello world')::text) > 100` | `t` |
| second call (guard is stateless; breaker stays closed) | `t` |
| `length(ai._chat('hi')) > 0` | `t` |

## Honest limits (documented, not hidden)

1. **This is resolve-and-check, not resolve-then-connect.** `minreq` exposes no custom resolver or connector, so
   the address the guard checked cannot be pinned through to the socket without replacing the HTTP client. A
   DNS-rebinding attacker who flips the record between our resolution and `minreq`'s retains a narrow window. It is
   a real residual, and it is recorded here rather than papered over. Closing it means either an HTTP client with a
   connector hook or a pre-resolved connection — a dependency decision, deliberately out of this milestone's scope
   (ADR M134-2). Note the exploitability floor rose regardless: the attacker must now also be a **superuser** to
   choose the hostname at all.
2. **The denylist enumerates ranges.** Ranges outside RFC1918 / loopback / link-local / unique-local — a corporate
   network on public-registered space, for example — are not covered by classification. That is what the allowlist
   inverts for deployments that need it; a strict allow-only posture would be a different (stricter) design.
3. **`cargo pgrx test` remains inexecutable on this droplet** (the known symbol-linking limitation, same as M131 and
   M132). The `#[pg_test]` cases are committed and run under a working pg_test harness; the **executable proof for
   this milestone is the in-PG SQL matrix above**, run against the shipped `.so` behind the anti-silent-restart
   gate. That is a stronger oracle than the unit tests for exactly this behaviour — it exercises the real backend,
   the real GUC permission machinery, and the real resolver.

## Verdict

**#117 closed.** The endpoint is operator-only, every internal-address class this codebase can reach is refused
with a typed error naming the target, the check sits at the one choke point every AI caller shares, unknown
resolution fails closed, the operator retains a scoped way to permit a real on-prem host, and the public path is
unchanged. The residual DNS-rebinding window is stated, not concealed.
