//! M102 — AI predicates as SET-oriented, planner-optimizable operators.
//!
//! Today `ai.if(prompt)` is a per-row scalar: one HTTP round-trip PER ROW, and the planner cannot batch it or
//! reason about its cost. M102 adds two planner-friendly surfaces over the SAME inference (`crate::chat`):
//!
//!   * `ai.if_batch(condition, vals[]) -> bool[]` — SET-oriented: over N values it builds N per-item prompts
//!     `"{condition}: {value}"` and answers them in ONE batched round-trip (`ai_generate_batch`), parsing each
//!     yes/no to a bool. Element-for-element EQUAL to the per-row `ai.if` on the same per-item prompt (proven
//!     hermetically with the deterministic `parity` test model — ADR D3), at 1 round-trip instead of N.
//!   * `ai.if_costly(condition, val) -> bool` — a per-row scalar declared with a HIGH `COST` so Postgres's own
//!     `order_qual_clauses` evaluates cheap relational quals FIRST; `WHERE cheap AND ai.if_costly(...)` then
//!     short-circuits the expensive AI on rows the cheap qual already dropped. That IS LOTUS's dependency-safe
//!     filter push-down, delegated to the planner (Rule 9 — do not reinvent qual ordering). The row-count
//!     reduction is observable via `ai.call_count()` (the wiring-triad runtime metric).
//!
//! Honest ceiling (ADR D4): a composability / round-trip win with STATISTICAL accuracy — orthogonal to vector
//! recall. Any latency/accuracy claim is measured in `docs/benchmarks/m102-ai-operators.*`, never asserted here.

use pgrx::prelude::*;

use crate::chat::{ai_if, ai_if_batch_answers};

/// The per-item prompt the batched operator and the per-row scalar BOTH build, so their results are equal
/// item-for-item under the same model. Keeping this the single source of the prompt shape is what makes the
/// result-equivalence contract (ADR D3) hold by construction rather than by coincidence. Newlines in the
/// untrusted `value` are collapsed to spaces so a value cannot forge a new numbered line in the batched prompt
/// (the batch protocol joins items with '\n' — council-security/ai-in-db hardening).
fn per_item_prompt(condition: &str, value: &str) -> String {
    format!("{condition}: {}", value.replace(['\n', '\r'], " "))
}

/// SET-oriented AI.IF: N values → N booleans in ONE inference round-trip. A NULL value yields a NULL bool and
/// is NOT sent to the model (preserving N-in/N-out alignment for the non-null items). An unparseable model
/// answer yields NULL for that item (SQL semantics — no panic across the C boundary; Rule 8).
pub(crate) fn ai_if_batch_impl(
    condition: &str,
    vals: &[Option<&str>],
    model: Option<&str>,
) -> Vec<Option<bool>> {
    // Own the per-item prompts for the non-null values, remembering each one's original position.
    let owned: Vec<(usize, String)> = vals
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|s| (i, per_item_prompt(condition, s))))
        .collect();

    let mut out: Vec<Option<bool>> = vec![None; vals.len()];
    if owned.is_empty() {
        return out; // all NULL → no round-trip
    }
    let refs: Vec<Option<&str>> = owned.iter().map(|(_, p)| Some(p.as_str())).collect();
    // ONE round-trip, boolean-shaped (yes/no system prompt) so the batched answers are directly comparable to
    // the per-row `ai.if` on a live model — not merely under the deterministic test model.
    let answers = ai_if_batch_answers(&refs, model);
    for ((i, _), ans) in owned.iter().zip(answers.iter()) {
        out[*i] = *ans;
    }
    out
}

/// The per-row scalar the high-COST `ai.if_costly` wraps — `ai.if` over the SAME per-item prompt the batched
/// operator builds, so the two surfaces agree element-for-element.
pub(crate) fn ai_if_one(condition: &str, value: &str, model: Option<&str>) -> bool {
    ai_if(Some(&per_item_prompt(condition, value)), model)
}

/// LOTUS dependency-safe push-down predicate (pure): a relational filter that reads only `depends_on` columns
/// may be pushed BELOW the AI op iff it does not read any column the AI op GENERATES — i.e. the two sets are
/// disjoint. Safe ⟺ `depends_on ∩ generated = ∅`. A filter that consumes an AI-generated field MUST stay above.
pub(crate) fn pushdown_safe(depends_on: &[&str], generated: &[&str]) -> bool {
    !depends_on.iter().any(|d| generated.contains(d))
}

#[pgrx::pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;

    /// `theodb_rs._ai_if_batch` — SET-oriented AI.IF (the SQL `ai.if_batch`). NULL array → 22023; NULL element
    /// → NULL bool; empty → empty (no round-trip).
    #[pg_extern]
    fn _ai_if_batch(
        condition: &str,
        vals: Option<Vec<Option<String>>>,
        model: Option<&str>,
    ) -> Vec<Option<bool>> {
        let vals = match vals {
            Some(v) => v,
            None => crate::pg::err_input("ai.if_batch: vals must not be NULL"),
        };
        let refs: Vec<Option<&str>> = vals.iter().map(|o| o.as_deref()).collect();
        crate::ai_op::ai_if_batch_impl(condition, &refs, model)
    }

    /// `theodb_rs._ai_if_costly` — per-row AI.IF over `"{condition}: {val}"` (the SQL `ai.if_costly`, declared
    /// with a high COST so the planner runs cheap quals first).
    #[pg_extern]
    fn _ai_if_costly(condition: &str, val: &str, model: Option<&str>) -> bool {
        crate::ai_op::ai_if_one(condition, val, model)
    }

    /// `theodb_rs._ai_call_count` — inference round-trips this backend issued since the last reset (the SQL
    /// `ai.call_count()` — the M102 wiring-triad runtime metric; proves "1 round-trip for N rows" + push-down).
    #[pg_extern]
    fn _ai_call_count() -> i64 {
        crate::chat::ai_call_count() as i64
    }

    /// `theodb_rs._ai_call_reset` — reset the round-trip counter (the SQL `ai.call_reset()`).
    #[pg_extern]
    fn _ai_call_reset() {
        crate::chat::ai_call_reset();
    }
}

// SQL wrappers: the public `ai.if_batch` / `ai.if_costly` / `ai.call_count` / `ai.call_reset`. `ai.if_costly`
// carries `COST 100000` so `order_qual_clauses` places cheaper relational quals ahead of it (the dependency-safe
// push-down, delegated to the planner). All REVOKEd from PUBLIC (least-privilege parity with the other ai.*).
extension_sql!(
    r#"
CREATE FUNCTION ai.if_batch(condition text, vals text[], model text DEFAULT NULL)
RETURNS boolean[] LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_if_batch(condition, vals, model) $$;

CREATE FUNCTION ai.if_costly(condition text, val text, model text DEFAULT NULL)
RETURNS boolean LANGUAGE sql VOLATILE COST 100000
AS $$ SELECT theodb_rs._ai_if_costly(condition, val, model) $$;

CREATE FUNCTION ai.call_count() RETURNS bigint LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_call_count() $$;

CREATE FUNCTION ai.call_reset() RETURNS void LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_call_reset() $$;

COMMENT ON FUNCTION ai.if_batch(text, text[], text) IS
  'SET-oriented AI.IF: N values -> N booleans in ONE inference round-trip (batched, yes/no-shaped). '
  'Comparable to the per-row ai.if on "{condition}: {value}". NULL value -> NULL bool. SECURITY: values become '
  'LLM prompt input (prompt-injection surface, blast radius = the row own boolean); NEVER GRANT to an isolated '
  'role. Implemented in Rust (theodb_rs, M102).';
COMMENT ON FUNCTION ai.if_costly(text, text, text) IS
  'Per-row AI.IF declared with a high COST so the planner evaluates cheaper relational quals first '
  '(dependency-safe filter push-down). Observe the row reduction via ai.call_count(). SECURITY: values become '
  'LLM prompt input; NEVER GRANT to an isolated role. theodb_rs, M102.';

REVOKE ALL ON FUNCTION ai.if_batch(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.if_costly(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_if_batch(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_if_costly(text, text, text) FROM PUBLIC;
"#,
    name = "theodb_ai_op",
    requires = [_ai_if_batch, _ai_if_costly, _ai_call_count, _ai_call_reset, "theodb_ai_wrappers"],
);

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    // ---- pure-logic unit tests (no PG needed for the dependency-safety algebra) ----

    #[pg_test]
    fn pushdown_safe_iff_disjoint() {
        // a cheap filter on `id` is safe below an AI op that generates `is_spam`
        assert!(crate::ai_op::pushdown_safe(&["id"], &["is_spam"]));
        // a filter that reads the AI-generated field must NOT be pushed below the AI op
        assert!(!crate::ai_op::pushdown_safe(&["is_spam"], &["is_spam"]));
        // partial overlap is unsafe
        assert!(!crate::ai_op::pushdown_safe(&["id", "is_spam"], &["is_spam"]));
        // empty depends_on is trivially safe
        assert!(crate::ai_op::pushdown_safe(&[], &["is_spam"]));
    }

    // ---- result-equivalence: batched operator == per-row path, with the deterministic 'parity' model ----

    #[pg_test]
    fn if_batch_equals_per_row_and_uses_one_round_trip() {
        Spi::run("SET theodb.llm_test_model = 'parity'").unwrap();
        // reset the round-trip counter, then run the batched operator over 4 values
        Spi::run("SELECT ai.call_reset()").unwrap();
        let batched = Spi::get_one::<Vec<Option<bool>>>(
            "SELECT ai.if_batch('is even', ARRAY['1','2','3','4'])",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            batched,
            vec![Some(false), Some(true), Some(false), Some(true)],
            "parity model: even -> yes/true, odd -> no/false"
        );
        // exactly ONE round-trip for the 4 values (the batching win)
        let calls = Spi::get_one::<i64>("SELECT ai.call_count()").unwrap().unwrap();
        assert_eq!(calls, 1, "batched operator issues ONE round-trip for N rows");

        // element-for-element equal to the per-row ai.if on the same "{condition}: {value}" prompt
        for (i, v) in ["1", "2", "3", "4"].iter().enumerate() {
            let per_row = Spi::get_one::<bool>(&format!("SELECT ai.\"if\"('is even: {v}')"))
                .unwrap()
                .unwrap();
            assert_eq!(Some(per_row), batched[i], "batched[{i}] == per-row ai.if('is even: {v}')");
        }
    }

    #[pg_test]
    fn if_batch_null_value_yields_null_bool_no_round_trip_for_all_null() {
        Spi::run("SET theodb.llm_test_model = 'parity'").unwrap();
        // a NULL element -> NULL bool, non-null still evaluated
        let mixed = Spi::get_one::<Vec<Option<bool>>>(
            "SELECT ai.if_batch('is even', ARRAY['2', NULL, '3'])",
        )
        .unwrap()
        .unwrap();
        assert_eq!(mixed, vec![Some(true), None, Some(false)]);

        // all-NULL -> no round-trip at all
        Spi::run("SELECT ai.call_reset()").unwrap();
        let _ = Spi::get_one::<Vec<Option<bool>>>(
            "SELECT ai.if_batch('is even', ARRAY[NULL, NULL]::text[])",
        )
        .unwrap();
        let calls = Spi::get_one::<i64>("SELECT ai.call_count()").unwrap().unwrap();
        assert_eq!(calls, 0, "all-NULL input issues zero round-trips");
    }

    // ---- push-down: the cheap qual runs first, so the AI op sees only the survivors ----

    #[pg_test]
    fn cheap_qual_pushes_below_costly_ai_predicate() {
        Spi::run("SET theodb.llm_test_model = 'parity'").unwrap();
        Spi::run("CREATE TEMP TABLE t_pd(id int, txt text)").unwrap();
        Spi::run("INSERT INTO t_pd SELECT g, g::text FROM generate_series(1,100) g").unwrap();

        // WHERE id <= 10 AND ai.if_costly(...) — the high COST makes the planner evaluate `id <= 10` first,
        // so ai.if_costly runs on at most 10 rows (AND short-circuit), not all 100.
        Spi::run("SELECT ai.call_reset()").unwrap();
        let matched = Spi::get_one::<i64>(
            "SELECT count(*) FROM t_pd WHERE id <= 10 AND ai.if_costly('is even', txt)",
        )
        .unwrap()
        .unwrap();
        assert_eq!(matched, 5, "ids 2,4,6,8,10 are even among id<=10");

        let calls = Spi::get_one::<i64>("SELECT ai.call_count()").unwrap().unwrap();
        assert!(
            calls <= 10,
            "cheap qual pushed first: AI predicate evaluated on <=10 survivors, not all 100 (got {calls})"
        );
    }
}
