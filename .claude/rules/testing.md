# Testing

Source of Truth for test discipline. Stack-agnostic.

## § 1 — Philosophy

- Tests protect **behavior**, not lines. 100% coverage with empty assertions is worse than 60% coverage with meaningful tests.
- Tests are **executable documentation**. A good test describes what the system does without reading production code.
- A broken test is the **highest-priority bug**. Once red tests are ignored, all tests lose value.

## § 2 — Pyramid

```
        /  E2E  \        Few — critical end-to-end flows only
       /----------\
      / Integration\     Moderate — system boundaries (DB, APIs, queues)
     /--------------\
    /   Unit         \   Many — pure business logic, fast, deterministic
   /------------------\
```

- **Unit** — pure business logic, no I/O. Run in milliseconds. The foundation.
- **Integration** — boundaries: repositories against a real DB, clients against real APIs, consumers against real queues. DIP pays off here: unit tests mock, integration tests use real implementations.
- **E2E** — critical user-visible flows. Few, stable, representative. Don't chase edge cases here.

## § 3 — Rules

- Every business rule MUST have a unit test. No exceptions.
- Every bug fix starts with a **failing regression test**, then the fix.
- Tests MUST be deterministic. Flaky tests are bugs — fix or delete.
- Each test exercises ONE behavior. "and" in the test name is a smell.
- Tests are independent. No shared mutable state, no order dependency.
- Use Arrange-Act-Assert (AAA) or Given-When-Then. Pick one per repo.
- Test names describe behavior, not method: `transfer_fails_when_balance_insufficient`, not `test_transfer_1`.

## § 4 — What to test vs. what NOT to test

| Test | Don't test |
|---|---|
| Business rules, calculations | Trivial getters/setters |
| Validation, edge cases | Framework-generated code |
| Integration with external systems | Internal structure (test behavior, not implementation) |
| Error / fallback scenarios | Third-party libraries (they have their own tests) |
| API contracts (request/response) | Layout/CSS unless it's a product requirement |

## § 4.1 — Edge cases vs negative cases

Two distinct lenses. Cover **both** — not just whichever is easier to imagine. A suite with only edge cases is half done.

| | **Edge case** | **Negative case** |
|---|---|---|
| What it is | An extreme of a **valid** scenario | An **invalid / wrong / unexpected** input |
| Why it happens | Caller pushes a limit; a rare-but-real event occurs | Caller makes a mistake; a system fails |
| Question it answers | "Does it hold **at the boundary**?" | "Does it **fail-fast and recover gracefully**?" |
| Passing behavior | Correct result at the extreme | Typed error + clear message, no corruption |
| Examples | password of exactly 8 or 16 chars; empty-but-valid list; leap day (Feb 29); max int | letters in a phone field; missing required email; network down on submit; `null` where a value is required |

- **Edge cases test boundaries; negative cases test error handling.** They fail differently: an unhandled edge produces a *wrong answer*; an unhandled negative produces a *crash or a silent swallow*.
- Negative cases are where **Error Handling** is proven (fail-fast, fail-clear, **typed errors**, validate at the boundary). A negative-case test asserts the *specific typed error and message* — not merely "it throws".
- For every input boundary, ask both questions: "what is the largest/smallest **valid** value?" (edge) **and** "what is the first **invalid** value past it?" (negative).

## § 5 — Test pairing convention

The default convention assumed by stop-validation.sh:

- `<name>_test.<ext>` (same directory) — Go, Python (pytest), most languages
- `<name>.test.<ext>` — JS/TS (Jest)
- `<name>.spec.<ext>` — JS/TS (Jasmine), Ruby
- `test_<name>.<ext>` — Python (pytest alternative)

If your project uses a different convention (e.g., separate `tests/` mirror tree), document it here so the hook knows where to look.

## § 5.1 — Columnar routing changes: the type-coverage A/B gate (M163)

Any change to the columnar routing admit-paths (`theodb_rs/src/am/columnar_agg.rs` `classify_target_node` /
`extract_*_predicate` / group-expr) MUST pass **`benchmarks/columnar_type_ab.py`** BEFORE `/review`. The ClickBench A/B
oracle runs over benchmark data, which does NOT exercise the type space — so type-class bugs (integer widening, temporal,
float IEEE, collation) survive it and are only caught by council review after a 14-min rebuild (the recurring
M151/M154/M157/M161 defect; M161 alone: 1 BLOCKER + 1 HIGH the `int4-int4` A/B never triggered).

- **What it does:** for each routed class × each per-type edge value (int2 `32767`, int4/int8 max, float `-0.0`/`NaN`,
  timestamp/date/timestamptz, non-C text, NULL — the `EDGE_CATALOG`), it asserts the fail-closed contract:
  **byte-identical (symmetric-EXCEPT diverged=0) if it routes, OR correct-decline (EXPLAIN has no `Custom Scan`)**.
- **Self-test:** a **positive control** (a deliberately-divergent pair) MUST report `diverged>0`; if it ever reports 0
  the oracle is broken and the run aborts. This is the proof it would catch a wrong `out_typoid` (the M161 BLOCKER).
- **Run:** `PGDATABASE=<db-with-theodb_rs> python3 benchmarks/columnar_type_ab.py --out docs/benchmarks/<slug>-type-coverage.md`
  (exit 0 = all cases as-expected). Pure-logic tiers run without a DB via `pytest benchmarks/test_columnar_type_ab.py`.
- **Extend it:** a new routed type or class MUST add its edges to `EDGE_CATALOG` + a case to `build_cases()`;
  `test_edge_catalog_has_all_routed_types` fails when a routed type has no edges (coverage-rot guard).
- It is **complementary** to the ClickBench A/B, never a replacement (the ClickBench data proves production shapes; this
  proves the type space).

## § 6 — Anti-patterns

- Tests depending on execution order or shared state.
- Tests asserting on internal structure (break on every refactor).
- Excessive mocking: if you need 10 mocks to test a function, the design is wrong (revisit SRP).
- Commented-out or permanently `@skip`'d tests — invisible technical debt.
- Testing only the happy path. Bugs live in edge cases **and** negative cases (see § 4.1) — covering one lens while ignoring the other is half a suite.
- Time/randomness in unit tests — inject a clock/RNG so the test is deterministic.
