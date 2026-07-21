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
| Who may set `theodb.embedding_endpoint` | **any role** (unregistered placeholder GUC) | superuser, or a role explicitly granted SET — `ERROR: permission denied to set parameter` |
| `http://169.254.169.254/` (cloud metadata) | request issued by the DB host | `ERROR: 22023 … resolves to a blocked internal address` |
| `http://10.0.0.1/`, `127.0.0.1`, `192.168.x`, `172.16.x`, `::1`, `0.0.0.0` | request issued | refused |
| NAT64 `64:ff9b::a9fe:a9fe`, 6to4, CGNAT `100.64/10`, multicast | request issued | refused (added post-review, F2) |
| `http://169.254.169.254:x@api.openai.com/v1` (userinfo confusion) | request issued to the metadata service | refused (the BLOCKER the review caught, F1) |
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

All refusals below carry SQLSTATE **22023** (`invalid_parameter_value`). The transcript is from the FINAL binary
(post-review), so the message is the F5 wording — the caller is told which host it asked for, and the resolved
address goes to the server log only:

```
ERROR:  22023: theodb.embed: refusing to call 169.254.169.254 — it resolves to a blocked internal address
        (loopback/private/link-local/NAT64 targets are denied). The resolved address is in the server log; an
        operator may permit a specific host via theodb.egress_allowlist
        … refusing to call 127.0.0.1
        … refusing to call 10.0.0.1
        … refusing to call 172.16.0.1
        … refusing to call 192.168.0.1
        … refusing to call 0.0.0.0
        … refusing to call localhost      ← a NAME, resolved first, then judged
```

and the matching operator-side log lines (26 in the verification run):

```
LOG:  theodb egress guard: theodb.embed denied host localhost -> blocked address ::1
LOG:  theodb egress guard: theodb.embed denied host 10.0.0.1 -> blocked address 10.0.0.1
LOG:  theodb egress guard: theodb.embed denied host m134-internal.localhost -> blocked address ::1
```

`http://[::1]:9/v1` is refused one step earlier — `endpoint is not a usable http(s) URL` — because a bracketed
IPv6 literal is an authority shape `minreq` cannot dial either (it parses the host as `"["`). Refusing beats
pretending to check something the client will never reach.

The `localhost` line is the one that matters most: a check that only pattern-matched IP literals in the URL would
have let it through. The guard resolves first and judges the **resolved addresses**, so a DNS name pointing at an
internal host is refused exactly like the literal.

The chat path is covered by the same code, with the caller's own function name in the message:

```
ERROR:  ai._chat: refusing to call 169.254.169.254 — it resolves to a blocked internal address …
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

## 6. Post-review: one BLOCKER found and closed (the guard was bypassable)

The `council-security` review found a **full bypass** of the guard shipped in the first commit, plus four smaller
findings. All are fixed and re-verified on the rebuilt binary. Recording what was wrong, because the failure mode
is instructive:

### F1 (BLOCKER) — the guard and the HTTP client disagreed about which host would be dialed

`minreq` 2.14 does not implement userinfo. `HttpUrl::parse` (`http_url.rs:70-88`) takes every char up to the first
`:` / `/` / `?` as the host, and when the port fails to parse it **silently falls back to 80/443**
(`http_url.rs:159-165`). My first `endpoint_host` stripped userinfo the RFC-correct way. So:

```
http://169.254.169.254:x@api.openai.com/v1
   guard checked   → api.openai.com   (public → allowed)
   minreq connects → 169.254.169.254:80
```

The guard was checking a host the client never contacts. **Worse, my own test blessed it** — it asserted
`http://user:pass@10.0.0.1:8080/x` → `10.0.0.1`, the RFC reading, which is precisely the wrong model. A green test
gave false confidence exactly where the bug lived.

Fix: `endpoint_host` now **mirrors minreq's parser** rather than parsing "correctly", and fails closed on any
authority shape it cannot model. Verified against the reviewer's exact payloads on the shipped `.so`:

```
http://169.254.169.254:80@8.8.8.8/latest/meta-data/     → ERROR refusing to call 169.254.169.254
http://169.254.169.254:x@api.openai.com/v1/embeddings   → ERROR refusing to call 169.254.169.254
http://10.0.0.5:1@1.1.1.1/x                             → ERROR refusing to call 10.0.0.5
```

The lesson generalizes past this milestone: **a guard in front of a client must model the client, not the spec.**
Any divergence between the two parsers is the bypass.

### F2 (MEDIUM) — ranges the classifier intended to cover but missed

Added: NAT64 `64:ff9b::/96` (on an IPv6-only cloud host `64:ff9b::a9fe:a9fe` reaches 169.254.169.254 — this was
the sharpest gap), 6to4 `2002::/16`, Teredo, IPv4-compatible `::a.b.c.d`, site-local `fec0::/10`, multicast (v4+v6),
CGNAT `100.64/10` (k8s and ISP internal space), benchmarking `198.18/15`, `192.0.0/24`, reserved `240/4`.

### F3 (MEDIUM) — a breakage this milestone introduced downstream

`theodb_ml.apply_model` does `set_config('theodb.llm_endpoint', …)` and is SECURITY INVOKER, so a non-superuser
caller now gets `permission denied to set parameter`. That is the intended posture — but the tempting "fix"
(SECURITY DEFINER) would hand endpoint control back to anyone with EXECUTE on `create_model` and **reopen #117 in
full**. Documented as a load-bearing warning in `sql/70-theodb-ml.sql`, in the function COMMENT, and in the
CHANGELOG as a second BREAKING entry.

### F5 (LOW) — the error was an internal name→IP oracle

`refusing to call inference.corp -> 10.1.2.3` handed any caller who could trigger it a working internal DNS map.
The resolved address now goes to the **server log** (`LOG: theodb egress guard: theodb.embed denied host
m134-internal.localhost -> blocked address ::1` — 26 such lines in the verification run); the caller sees only the
host it asked for.

### F4 / F6 — accepted, documented below rather than "fixed"

### What the review confirmed as genuinely closed

The reviewer independently verified — against the vendored `minreq` source, not by assumption — that obfuscated
IPv4 literals (`http://2130706433/`, `0177.0.0.1`, `127.1`) are normalized by `getaddrinfo` and blocked; that
`with_max_redirects(0)` errors *before* connecting (and `308` is never followed at all); that multi-A/dual-stack
has no "first is public, second is internal" window since both the guard and the client consider all resolved
addresses; that the API key never leaves the `Authorization` header; and that `post_json` really is the crate's
only outbound HTTP call.

### A testability change the review forced

The pure address policy now lives in `theodb_rs/src/egress.rs`, deliberately std-only and pgrx-free, so it
compiles and runs standalone:

```
$ rustc --test --edition 2021 src/egress.rs && ./egress_test
running 4 tests ... test result: ok. 4 passed; 0 failed
```

This matters because most F2 ranges (NAT64, 6to4, multicast) **cannot be expressed in a URL minreq will parse**, so
the in-PG SQL matrix physically cannot reach them — asserting them without an executable oracle would have been
faith. On its very first run this harness caught two *wrong test expectations* of mine (the `user:pass@` case
parses to host `user`, because `:` wins before `@`), which is exactly the class of error that produced F1.

## Honest limits (documented, not hidden)

1. **This is resolve-and-check, not resolve-then-connect.** `minreq` exposes no custom resolver or connector, so
   the address the guard checked cannot be pinned through to the socket without replacing the HTTP client. A
   DNS-rebinding attacker who flips the record between our resolution and `minreq`'s retains a narrow window. It is
   a real residual, and it is recorded here rather than papered over. Closing it means either an HTTP client with a
   connector hook or a pre-resolved connection — a dependency decision, deliberately out of this milestone's scope
   (ADR M134-2). Note the exploitability floor rose regardless: the attacker must now also be a **superuser** to
   choose the hostname at all.
2. **F4 — allowlisting a NAME delegates the decision to DNS.** An allowlisted host skips resolution entirely, so
   if that name's DNS is influenceable (internal takeover, wildcard), the operator has created a permanent
   unrestricted egress. Allowlist IP literals where possible. Accepted, not fixed: resolving allowlisted hosts and
   requiring an operator-declared CIDR is a larger design than this milestone.
3. **F6 — `Suset` is "superuser **or** a role granted SET on the parameter" (PG15+),** not "superuser only".
   `GRANT SET ON PARAMETER theodb.llm_endpoint TO app_role` re-opens the T1.1 half deliberately. `ALTER ROLE/
   DATABASE/FUNCTION … SET` require the same privilege, so those are closed.
4. **The denylist enumerates ranges.** Ranges outside RFC1918 / loopback / link-local / unique-local — a corporate
   network on public-registered space, for example — are not covered by classification. That is what the allowlist
   inverts for deployments that need it; a strict allow-only posture would be a different (stricter) design.
5. **`cargo pgrx test` remains inexecutable on this droplet** (the known symbol-linking limitation, same as M131 and
   M132). The `#[pg_test]` cases are committed and run under a working pg_test harness; the **executable proof for
   this milestone is the in-PG SQL matrix above**, run against the shipped `.so` behind the anti-silent-restart
   gate. That is a stronger oracle than the unit tests for exactly this behaviour — it exercises the real backend,
   the real GUC permission machinery, and the real resolver.

## Verdict

**#117 closed — after the review caught that the first attempt was bypassable.** The endpoint is operator-only, every internal-address class this codebase can reach is refused
with a typed error naming the target, the check sits at the one choke point every AI caller shares, unknown
resolution fails closed, the operator retains a scoped way to permit a real on-prem host, and the public path is
unchanged. The residual DNS-rebinding window is stated, not concealed.
