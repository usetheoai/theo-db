//! Vectorizer job-queue logic (M54, ADR 0016). The crash-safe HEART of the declarative auto-embedding
//! pipeline, sliced OUT of the background worker so 100% of the queue state machine is testable via
//! `#[pg_test]` WITHOUT a running worker, `shared_preload_libraries`, or an OpenAI endpoint (blueprint § Fatia
//! de testabilidade; MEMORY m46: CI does not run `cargo pgrx test`, pgrx test sets no preload).
//!
//! The worker main (a ~20-line loop, elsewhere) composes: `claim_batch` (txn1, commits) → `embed_batch` (no
//! txn, over HTTP) → `mark_done`/`mark_failed` (txn2, owner-guarded). The COMMITTED lease — not a held lock —
//! protects a job between phases (H2). Every transition is guarded by a fencing `owner` token so a slow-but-
//! alive worker whose lease expired and was reclaimed cannot clobber the new owner (H1 — the single most
//! important crash-safety correction from discovery). Attempts are burned ON CLAIM (not on failure) so a job
//! that kills the worker before reporting still counts down to the `failed` dead-letter (H3, poison-pill).
use pgrx::prelude::*;

// The declarative config + the crash-safe job queue (ADR 0016). `owner` is an opaque text fencing token
// (a worker-generated uuid rendered as text — text keeps the marshalling trivial; the fencing works
// identically). `state` is a typed CHECK enum (pending/processing/failed — 'done' jobs are DELETEd, à la
// pgmq.archive). The partial-ish claim index covers the `SKIP LOCKED` scan.
extension_sql!(
    r#"
CREATE TABLE IF NOT EXISTS theodb.vectorizer (
    id            serial PRIMARY KEY,
    source_table  text NOT NULL,
    source_pk_col text NOT NULL,
    content_col   text NOT NULL,
    target_table  text NOT NULL,
    target_col    text NOT NULL,
    model         text,
    dims          int,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS theodb.vectorizer_queue (
    job_id         bigserial PRIMARY KEY,
    vectorizer_id  int  NOT NULL REFERENCES theodb.vectorizer(id) ON DELETE CASCADE,
    source_pk      text NOT NULL,
    op             text NOT NULL DEFAULT 'upsert' CHECK (op IN ('upsert','delete')),
    state          text NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','processing','failed')),
    attempts       int  NOT NULL DEFAULT 0,
    owner          text,
    lease_deadline timestamptz,
    last_error     text,
    enqueued_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS vectorizer_queue_claim_idx
    ON theodb.vectorizer_queue (state, enqueued_at);
"#,
    name = "theodb_vectorizer_schema",
);

// The declarative surface (plpgsql — dynamic DDL is natural here, KISS): a generic AFTER-row trigger that
// enqueues on INSERT/UPDATE/DELETE, `theodb.create_vectorizer` that wires it to a source table, and a v1
// chunking helper. The trigger does ONLY a cheap INSERT into the queue (no HTTP) — all model latency stays
// in the worker, off the writer's transaction (ADR 0016 / blueprint R5).
extension_sql!(
    r#"
-- Generic enqueue trigger: TG_ARGV[0]=vectorizer_id, TG_ARGV[1]=source_pk_col. The PK is extracted as text
-- via to_jsonb(row)->>col (no dynamic EXECUTE needed). Enqueues 'delete' on DELETE, else 'upsert'.
CREATE FUNCTION theodb._vectorizer_enqueue() RETURNS trigger LANGUAGE plpgsql AS $fn$
DECLARE
    vid   int  := TG_ARGV[0]::int;
    pkcol text := TG_ARGV[1];
BEGIN
    IF TG_OP = 'DELETE' THEN
        INSERT INTO theodb.vectorizer_queue (vectorizer_id, source_pk, op)
        VALUES (vid, to_jsonb(OLD) ->> pkcol, 'delete');
        RETURN OLD;
    END IF;
    INSERT INTO theodb.vectorizer_queue (vectorizer_id, source_pk, op)
    VALUES (vid, to_jsonb(NEW) ->> pkcol, 'upsert');
    RETURN NEW;
END;
$fn$;

-- Declarative registration: record the config + attach the enqueue trigger to the source table. Returns the
-- vectorizer id. Idempotent-ish: a second call on the same table creates a second vectorizer/trigger (by id)
-- — callers dedupe by not re-registering. The AFTER trigger name is namespaced by id so several can coexist.
CREATE FUNCTION theodb.create_vectorizer(
    source_table  regclass,
    source_pk_col text,
    content_col   text,
    target_table  text,
    target_col    text,
    model         text DEFAULT NULL,
    dims          int  DEFAULT NULL
) RETURNS int LANGUAGE plpgsql AS $fn$
DECLARE
    vid    int;
    tgname text;
BEGIN
    INSERT INTO theodb.vectorizer (source_table, source_pk_col, content_col, target_table, target_col, model, dims)
    VALUES (source_table::text, source_pk_col, content_col, target_table, target_col, model, dims)
    RETURNING id INTO vid;

    tgname := format('theodb_vectorizer_%s', vid);
    EXECUTE format(
        'CREATE TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %s '
        'FOR EACH ROW EXECUTE FUNCTION theodb._vectorizer_enqueue(%L, %L)',
        tgname, source_table::text, vid::text, source_pk_col);
    RETURN vid;
END;
$fn$;

COMMENT ON FUNCTION theodb.create_vectorizer(regclass, text, text, text, text, text, int) IS
  'Declaratively maintain an embedding column: attach an AFTER INSERT/UPDATE/DELETE trigger to source_table '
  'that enqueues jobs into theodb.vectorizer_queue; the background worker drains them (M54, ADR 0016). The '
  'trigger only enqueues (cheap, no HTTP) — model latency stays off the writer transaction.';

-- v1 chunking helper: a fixed-size CHARACTER window with overlap (the simplest correct chunker). HONEST v1
-- scope (YAGNI): this is NOT a separator-aware recursive splitter (paragraph/sentence/word hierarchy) — that
-- is a tracked follow-up. Fail-fast typed on bad params (Rule 8). Empty/NULL input → empty array.
CREATE FUNCTION theodb.chunk_text(content text, chunk_size int DEFAULT 512, overlap int DEFAULT 64)
RETURNS text[] LANGUAGE plpgsql IMMUTABLE AS $fn$
DECLARE
    result text[] := '{}';
    pos    int := 1;
    len    int := length(coalesce(content, ''));
    step   int;
BEGIN
    IF chunk_size <= 0 THEN
        RAISE EXCEPTION 'theodb.chunk_text: chunk_size must be > 0 (got %)', chunk_size;
    END IF;
    IF overlap < 0 OR overlap >= chunk_size THEN
        RAISE EXCEPTION 'theodb.chunk_text: overlap must be in [0, chunk_size) (got %, chunk_size %)', overlap, chunk_size;
    END IF;
    IF len = 0 THEN
        RETURN '{}';
    END IF;
    step := chunk_size - overlap;
    WHILE pos <= len LOOP
        result := array_append(result, substr(content, pos, chunk_size));
        pos := pos + step;
    END LOOP;
    RETURN result;
END;
$fn$;

REVOKE ALL ON FUNCTION theodb.create_vectorizer(regclass, text, text, text, text, text, int) FROM PUBLIC;
"#,
    name = "theodb_vectorizer_surface",
    requires = ["theodb_vectorizer_schema"],
);

// All `#[pg_extern]` entrypoints live in the `theodb_rs` schema (project convention — mirrors
// `api.rs`'s `#[pg_schema] mod theodb_rs`). pgrx merges same-named `#[pg_schema]` modules across files.
#[pgrx::pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;

/// Atomically claim up to `batch` jobs for `owner`. A job is claimable when `pending` OR when `processing`
/// with an expired lease (its previous owner is presumed dead — visibility timeout). `FOR UPDATE SKIP LOCKED`
/// makes concurrent workers claim disjoint rows without blocking. `attempts` is incremented here (on claim);
/// jobs already at `attempts >= max_attempts` are NOT reclaimed (they are dead-lettered by `mark_failed`).
/// Returns the claimed jobs; the worker embeds them, then calls `mark_done`/`mark_failed` with the same
/// `owner`. `lease_secs` MUST exceed the worst-case embed time ((MAX_RETRIES+1)×HTTP_TIMEOUT ≈ 90s) so a
/// live-but-slow worker's lease does not expire mid-embed.
#[pg_extern]
fn _vectorizer_claim_batch(
    owner: &str,
    batch: i32,
    lease_secs: i32,
    max_attempts: i32,
) -> TableIterator<
    'static,
    (
        name!(job_id, i64),
        name!(vectorizer_id, i32),
        name!(source_pk, String),
        name!(op, String),
        name!(attempts, i32),
    ),
> {
    let rows = Spi::connect(|client| {
        let tbl = client
            .select(
                "UPDATE theodb.vectorizer_queue \
                 SET state='processing', owner=$1, lease_deadline=now() + make_interval(secs => $3), \
                     attempts = attempts + 1 \
                 WHERE job_id IN ( \
                   SELECT job_id FROM theodb.vectorizer_queue \
                   WHERE (state='pending' OR (state='processing' AND lease_deadline < now())) \
                     AND attempts < $4 \
                   ORDER BY enqueued_at \
                   FOR UPDATE SKIP LOCKED \
                   LIMIT $2) \
                 RETURNING job_id, vectorizer_id, source_pk, op, attempts",
                None,
                &[owner.into(), batch.into(), lease_secs.into(), max_attempts.into()],
            )
            .unwrap_or_else(|e| {
                crate::pg::err_input(&format!("vectorizer claim failed: {e:?}"))
            });
        let mut out = Vec::with_capacity(tbl.len());
        for row in tbl {
            let job_id = row.get::<i64>(1).ok().flatten().unwrap_or_default();
            let vid = row.get::<i32>(2).ok().flatten().unwrap_or_default();
            let pk = row.get::<String>(3).ok().flatten().unwrap_or_default();
            let op = row.get::<String>(4).ok().flatten().unwrap_or_default();
            let att = row.get::<i32>(5).ok().flatten().unwrap_or_default();
            out.push((job_id, vid, pk, op, att));
        }
        out
    });
    TableIterator::new(rows)
}

/// Owner-guarded completion: delete the job iff it is still `processing` AND still owned by `owner`. Returns
/// true iff the row was removed. A false return means the lease was lost (reclaimed by another worker) — the
/// caller MUST discard its result and NOT write, else it would clobber the new owner (H1 fencing).
#[pg_extern]
fn _vectorizer_mark_done(job_id: i64, owner: &str) -> bool {
    Spi::get_one_with_args::<i64>(
        "WITH d AS (DELETE FROM theodb.vectorizer_queue \
         WHERE job_id=$1 AND owner=$2 AND state='processing' RETURNING 1) \
         SELECT count(*) FROM d",
        &[job_id.into(), owner.into()],
    )
    .ok()
    .flatten()
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Owner-guarded failure. Recoverable vs terminal is decided IN SQL by `attempts` (burned on claim): below
/// `max_attempts` the job returns to `pending` (owner/lease cleared → immediately re-claimable, a bounded
/// retry); at/over the cap it becomes `failed` (dead-letter — never an infinite loop, H3). `last_error` is
/// recorded (Rule 8 — never swallow). Returns true iff the guarded row matched (false = lease lost, discard).
#[pg_extern]
fn _vectorizer_mark_failed(job_id: i64, owner: &str, err: &str, max_attempts: i32) -> bool {
    Spi::get_one_with_args::<i64>(
        "WITH u AS (UPDATE theodb.vectorizer_queue \
         SET state = CASE WHEN attempts >= $4 THEN 'failed' ELSE 'pending' END, \
             owner = NULL, lease_deadline = NULL, last_error = $3 \
         WHERE job_id=$1 AND owner=$2 AND state='processing' RETURNING 1) \
         SELECT count(*) FROM u",
        &[job_id.into(), owner.into(), err.into(), max_attempts.into()],
    )
    .ok()
    .flatten()
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Owner-guarded lease renewal for long batches: extend `lease_deadline` for jobs still owned by `owner`.
/// The worker calls this at ~⅓ of the lease interval while `embed_batch` runs, so a live worker never loses
/// jobs it is actively processing. Returns the count of jobs renewed (jobs whose lease was already lost are
/// silently skipped — they belong to another owner now). `job_ids` is a bigint[].
#[pg_extern]
fn _vectorizer_renew_lease(job_ids: Vec<i64>, owner: &str, lease_secs: i32) -> i64 {
    Spi::get_one_with_args::<i64>(
        "WITH u AS (UPDATE theodb.vectorizer_queue \
         SET lease_deadline = now() + make_interval(secs => $3) \
         WHERE job_id = ANY($1) AND owner=$2 AND state='processing' RETURNING 1) \
         SELECT count(*) FROM u",
        &[job_ids.into(), owner.into(), lease_secs.into()],
    )
    .ok()
    .flatten()
    .unwrap_or(0)
}
} // mod theodb_rs

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// Insert a minimal vectorizer + N pending jobs into a fresh queue; return the vectorizer id.
    fn seed(n: i32) -> i32 {
        Spi::run(
            "INSERT INTO theodb.vectorizer (source_table, source_pk_col, content_col, target_table, target_col, model, dims) \
             VALUES ('src','id','body','dst','emb','m',3)",
        )
        .unwrap();
        let vid = Spi::get_one::<i32>("SELECT max(id) FROM theodb.vectorizer").unwrap().unwrap();
        for i in 0..n {
            Spi::run(&format!(
                "INSERT INTO theodb.vectorizer_queue (vectorizer_id, source_pk, op) VALUES ({vid}, 'pk{i}', 'upsert')"
            ))
            .unwrap();
        }
        vid
    }

    #[pg_test]
    fn claim_moves_pending_to_processing_with_lease() {
        seed(3);
        let claimed: i64 = Spi::get_one(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)",
        )
        .unwrap()
        .unwrap();
        assert_eq!(claimed, 3, "all 3 pending jobs claimed");
        let processing: i64 = Spi::get_one(
            "SELECT count(*) FROM theodb.vectorizer_queue WHERE state='processing' AND owner='w1' AND lease_deadline > now()",
        )
        .unwrap()
        .unwrap();
        assert_eq!(processing, 3, "claimed jobs are processing, owned by w1, lease in the future");
    }

    #[pg_test]
    fn dead_owner_job_is_reclaimed_after_lease_expiry() {
        seed(1);
        // w1 claims, then we simulate w1 dying by back-dating its lease into the past.
        Spi::get_one::<i64>("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)").unwrap();
        Spi::run("UPDATE theodb.vectorizer_queue SET lease_deadline = now() - interval '1 second'").unwrap();
        // w2 must be able to reclaim the expired job (visibility timeout).
        let reclaimed: i64 =
            Spi::get_one("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w2', 10, 60, 5)").unwrap().unwrap();
        assert_eq!(reclaimed, 1, "expired-lease job is reclaimable by a new owner");
        let owner: String =
            Spi::get_one("SELECT owner FROM theodb.vectorizer_queue").unwrap().unwrap();
        assert_eq!(owner, "w2", "reclaimed job is now owned by w2");
    }

    #[pg_test]
    fn stale_owner_mark_done_affects_zero_rows() {
        seed(1);
        // w1 claims; lease expires; w2 reclaims. w1's late mark_done MUST NOT delete w2's job (H1 fencing).
        Spi::get_one::<i64>("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)").unwrap();
        Spi::run("UPDATE theodb.vectorizer_queue SET lease_deadline = now() - interval '1 second'").unwrap();
        Spi::get_one::<i64>("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w2', 10, 60, 5)").unwrap();
        let job_id: i64 = Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue").unwrap().unwrap();
        let w1_done: bool =
            Spi::get_one(&format!("SELECT theodb_rs._vectorizer_mark_done({job_id}, 'w1')")).unwrap().unwrap();
        assert!(!w1_done, "stale owner w1 must NOT complete the reclaimed job");
        let still_there: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.vectorizer_queue WHERE owner='w2'").unwrap().unwrap();
        assert_eq!(still_there, 1, "w2's job survives w1's stale mark_done");
        // The real owner w2 completes it → row removed.
        let w2_done: bool =
            Spi::get_one(&format!("SELECT theodb_rs._vectorizer_mark_done({job_id}, 'w2')")).unwrap().unwrap();
        assert!(w2_done, "true owner w2 completes the job");
        let remaining: i64 = Spi::get_one("SELECT count(*) FROM theodb.vectorizer_queue").unwrap().unwrap();
        assert_eq!(remaining, 0, "job removed after w2 completes");
    }

    #[pg_test]
    fn mark_failed_dead_letters_at_attempt_cap() {
        seed(1);
        // max_attempts=1: the first claim burns attempts→1; a recoverable failure at the cap dead-letters.
        Spi::get_one::<i64>("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 1)").unwrap();
        let job_id: i64 = Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue").unwrap().unwrap();
        let ok: bool = Spi::get_one(&format!(
            "SELECT theodb_rs._vectorizer_mark_failed({job_id}, 'w1', 'endpoint 500', 1)"
        ))
        .unwrap()
        .unwrap();
        assert!(ok, "guarded mark_failed matched the owner's job");
        let (state, err): (String, String) = Spi::connect(|c| {
            let r = c
                .select("SELECT state, last_error FROM theodb.vectorizer_queue", None, &[])
                .unwrap()
                .first();
            (r.get::<String>(1).unwrap().unwrap(), r.get::<String>(2).unwrap().unwrap())
        });
        assert_eq!(state, "failed", "at the attempt cap the job is dead-lettered, not retried forever");
        assert_eq!(err, "endpoint 500", "the typed error is recorded (Rule 8 — never swallow)");
    }

    #[pg_test]
    fn mark_failed_below_cap_returns_to_pending() {
        seed(1);
        // max_attempts=3: first claim burns attempts→1; a recoverable failure returns the job to pending.
        Spi::get_one::<i64>("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 3)").unwrap();
        let job_id: i64 = Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue").unwrap().unwrap();
        Spi::get_one::<bool>(&format!(
            "SELECT theodb_rs._vectorizer_mark_failed({job_id}, 'w1', 'transient', 3)"
        ))
        .unwrap();
        let (state, owner_null): (String, bool) = Spi::connect(|c| {
            let r = c
                .select("SELECT state, owner IS NULL FROM theodb.vectorizer_queue", None, &[])
                .unwrap()
                .first();
            (r.get::<String>(1).unwrap().unwrap(), r.get::<bool>(2).unwrap().unwrap())
        });
        assert_eq!(state, "pending", "below the cap the job returns to pending (bounded retry)");
        assert!(owner_null, "the failed job releases its owner so it is immediately re-claimable");
    }

    #[pg_test]
    fn renew_lease_extends_only_owned_jobs() {
        seed(2);
        Spi::get_one::<i64>("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 30, 5)").unwrap();
        let ids: Vec<i64> = Spi::connect(|c| {
            c.select("SELECT job_id FROM theodb.vectorizer_queue ORDER BY job_id", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i64>(1).unwrap())
                .collect()
        });
        let arr = format!("ARRAY[{}]::bigint[]", ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        // w2 (not the owner) renews nothing; w1 renews both.
        let w2n: i64 = Spi::get_one(&format!("SELECT theodb_rs._vectorizer_renew_lease({arr}, 'w2', 120)")).unwrap().unwrap();
        assert_eq!(w2n, 0, "a non-owner renews nothing (fencing)");
        let w1n: i64 = Spi::get_one(&format!("SELECT theodb_rs._vectorizer_renew_lease({arr}, 'w1', 120)")).unwrap().unwrap();
        assert_eq!(w1n, 2, "the owner renews all its jobs");
    }

    #[pg_test]
    fn create_vectorizer_enqueues_on_dml() {
        Spi::run("CREATE TEMP TABLE docs (id int PRIMARY KEY, body text)").unwrap();
        let vid: i32 = Spi::get_one(
            "SELECT theodb.create_vectorizer('docs'::regclass, 'id', 'body', 'docs', 'emb', 'm', 3)",
        )
        .unwrap()
        .unwrap();
        assert!(vid > 0, "create_vectorizer returns the new vectorizer id");
        // INSERT → one pending 'upsert' job keyed by the row PK.
        Spi::run("INSERT INTO docs VALUES (42, 'hello world')").unwrap();
        let (pk, op, state): (String, String, String) = Spi::connect(|c| {
            let r = c
                .select("SELECT source_pk, op, state FROM theodb.vectorizer_queue ORDER BY job_id DESC LIMIT 1", None, &[])
                .unwrap()
                .first();
            (
                r.get::<String>(1).unwrap().unwrap(),
                r.get::<String>(2).unwrap().unwrap(),
                r.get::<String>(3).unwrap().unwrap(),
            )
        });
        assert_eq!((pk.as_str(), op.as_str(), state.as_str()), ("42", "upsert", "pending"),
            "INSERT enqueues a pending upsert for the row PK");
        // UPDATE → another upsert; DELETE → a delete job.
        Spi::run("UPDATE docs SET body='changed' WHERE id=42").unwrap();
        Spi::run("DELETE FROM docs WHERE id=42").unwrap();
        let ops: Vec<String> = Spi::connect(|c| {
            c.select("SELECT op FROM theodb.vectorizer_queue ORDER BY job_id", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert_eq!(ops, vec!["upsert", "upsert", "delete"], "INSERT/UPDATE enqueue upsert, DELETE enqueues delete");
    }

    #[pg_test]
    fn chunk_text_windows_with_overlap() {
        // 'abcdefghij' (len 10), size 4, overlap 1 → step 3 → positions 1,4,7,10 → abcd,defg,ghij,j.
        let chunks: Vec<String> = Spi::connect(|c| {
            c.select("SELECT unnest(theodb.chunk_text('abcdefghij', 4, 1))", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert_eq!(chunks, vec!["abcd", "defg", "ghij", "j"], "fixed-size character window with overlap");
    }

    #[pg_test]
    fn chunk_text_empty_returns_empty() {
        let n: i32 = Spi::get_one("SELECT array_length(theodb.chunk_text(''), 1)").unwrap().unwrap_or(0);
        assert_eq!(n, 0, "empty content yields an empty chunk array (NULL length → 0)");
    }

    #[pg_test(error = "theodb.chunk_text: overlap must be in [0, chunk_size) (got 5, chunk_size 4)")]
    fn chunk_text_rejects_overlap_ge_size() {
        Spi::run("SELECT theodb.chunk_text('abc', 4, 5)").unwrap();
    }
}
