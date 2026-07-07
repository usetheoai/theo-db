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

-- Runtime metric (wiring triad pillar c): a single-row counter of jobs the worker processed / failed. The
-- worker bumps it via theodb_rs._vectorizer_bump_stats. Queryable (not just a LOG line) via theodb.vectorizer_stats().
CREATE TABLE IF NOT EXISTS theodb.vectorizer_worker_stats (
    only_row  boolean PRIMARY KEY DEFAULT true CHECK (only_row),
    processed bigint  NOT NULL DEFAULT 0,
    failed    bigint  NOT NULL DEFAULT 0,
    last_run  timestamptz
);
INSERT INTO theodb.vectorizer_worker_stats (only_row) VALUES (true) ON CONFLICT DO NOTHING;

-- The queryable metric: worker throughput (processed/failed cumulative) + live queue depth by state.
CREATE FUNCTION theodb.vectorizer_stats()
RETURNS TABLE(processed bigint, failed bigint, last_run timestamptz, pending bigint, processing bigint, failed_jobs bigint)
LANGUAGE sql STABLE AS $fn$
    SELECT s.processed, s.failed, s.last_run,
           (SELECT count(*) FROM theodb.vectorizer_queue WHERE state='pending'),
           (SELECT count(*) FROM theodb.vectorizer_queue WHERE state='processing'),
           (SELECT count(*) FROM theodb.vectorizer_queue WHERE state='failed')
    FROM theodb.vectorizer_worker_stats s
$fn$;
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

/// Look up a vectorizer's config. Diverges (typed) if the id is unknown. `model` may be NULL.
fn lookup_config(vectorizer_id: i32) -> (String, String, String, String, String, Option<String>) {
    Spi::connect(|c| {
        let t = c
            .select(
                "SELECT source_table, source_pk_col, content_col, target_table, target_col, model \
                 FROM theodb.vectorizer WHERE id=$1",
                None,
                &[vectorizer_id.into()],
            )
            .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer config lookup failed: {e:?}")));
        if t.is_empty() {
            crate::pg::err_input(&format!("vectorizer {vectorizer_id} not found"));
        }
        let r = t.first();
        (
            r.get::<String>(1).ok().flatten().unwrap_or_default(),
            r.get::<String>(2).ok().flatten().unwrap_or_default(),
            r.get::<String>(3).ok().flatten().unwrap_or_default(),
            r.get::<String>(4).ok().flatten().unwrap_or_default(),
            r.get::<String>(5).ok().flatten().unwrap_or_default(),
            r.get::<String>(6).ok().flatten(),
        )
    })
}

/// Build one dynamic SQL string with Postgres-native `format()` (injection-safe %I/%s over SPI). The
/// config identifiers (from `create_vectorizer`, owner-controlled) are still `%I`-quoted as defense.
fn build_sql(template: &str, a: &str, b: &str, c: &str) -> String {
    Spi::get_one_with_args::<String>(
        &format!("SELECT format($fmt${template}$fmt$, $1, $2, $3)"),
        &[a.into(), b.into(), c.into()],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| crate::pg::err_input("vectorizer: could not build dynamic query"))
}

/// Process one `upsert` job: fetch the source content, embed it (may longjmp on endpoint failure — the
/// worker traps that with PgTryBuilder and marks the job failed), and write the embedding into the target
/// column. v1 contract: the target row is keyed by the SAME PK value under `source_pk_col` (in-place: target
/// == source; or a sibling table carrying that PK column). Chunking is a follow-up (1 row → 1 embedding).
#[pg_extern]
fn _vectorizer_process_upsert(vectorizer_id: i32, source_pk: &str) {
    let (source_table, source_pk_col, content_col, target_table, target_col, model) =
        lookup_config(vectorizer_id);
    let fetch_q = build_sql(
        "SELECT %1$I::text FROM %2$s WHERE %3$I::text = $1",
        &content_col,
        &source_table,
        &source_pk_col,
    );
    let content = Spi::get_one_with_args::<String>(&fetch_q, &[source_pk.into()]).ok().flatten();
    // May diverge (ereport) on any embed failure — intentional: the worker's PgTryBuilder converts it to a
    // typed `failed` transition (Rule 8 — never swallow). embed reads GUCs via SPI, so this runs in a txn.
    let vec_text = crate::embed::run(content.as_deref(), model.as_deref());
    let upd_q = build_sql(
        "UPDATE %2$s SET %1$I = $1::vector WHERE %3$I::text = $2",
        &target_col,
        &target_table,
        &source_pk_col,
    );
    Spi::run_with_args(&upd_q, &[vec_text.into(), source_pk.into()])
        .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer upsert failed: {e:?}")));
}

/// Process one `delete` job: clear the target embedding for the removed source row. v1 is idempotent and
/// safe for the in-place case (target == source: the source row is already gone, so 0 rows) and the sibling
/// case (nulls the orphan embedding).
#[pg_extern]
fn _vectorizer_process_delete(vectorizer_id: i32, source_pk: &str) {
    let (_, source_pk_col, _, target_table, target_col, _) = lookup_config(vectorizer_id);
    let del_q = build_sql(
        "UPDATE %2$s SET %1$I = NULL WHERE %3$I::text = $1",
        &target_col,
        &target_table,
        &source_pk_col,
    );
    let _ = Spi::run_with_args(&del_q, &[source_pk.into()]);
}

/// Bump the worker throughput counter (wiring triad pillar c — queryable via theodb.vectorizer_stats()).
#[pg_extern]
fn _vectorizer_bump_stats(processed: i64, failed: i64) {
    Spi::run_with_args(
        "UPDATE theodb.vectorizer_worker_stats \
         SET processed = processed + $1, failed = failed + $2, last_run = now() WHERE only_row",
        &[processed.into(), failed.into()],
    )
    .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer stats bump failed: {e:?}")));
}
} // mod theodb_rs

// ── The background worker (ADR 0016) — the ONLY non-CI-testable piece (needs shared_preload_libraries). It
// orchestrates the 3-phase design per job: claim (txn, commits the lease) → process (own txn, embed wrapped
// in PgTryBuilder so an endpoint failure becomes a typed `failed`, never a worker crash — blueprint B1) →
// mark + stats (own txn). All work goes through SPI so the logic stays the tested #[pg_extern] functions. ──

const WORKER_BATCH: i32 = 10;
const WORKER_LEASE_SECS: i32 = 120; // ≥ (MAX_RETRIES+1)×HTTP_TIMEOUT so a live worker never loses its lease
const WORKER_MAX_ATTEMPTS: i32 = 5;
const WORKER_POLL_SECS: u64 = 1;
// v1: one worker per database; the DB is fixed here (a GUC-driven multi-DB launcher is a follow-up).
const WORKER_DBNAME: &str = "postgres";

/// Register the vectorizer worker — ONLY when loaded via `shared_preload_libraries` (postmaster). Calling
/// the static `.load()` from an ordinary backend (CREATE EXTENSION) is a no-op WARNING, so we guard on
/// `process_shared_preload_libraries_in_progress` to stay silent there (blueprint B2).
pub(crate) fn register_worker() {
    if unsafe { pgrx::pg_sys::process_shared_preload_libraries_in_progress } {
        use pgrx::bgworkers::*;
        BackgroundWorkerBuilder::new("theodb vectorizer worker")
            .set_function("theodb_embed_worker_main")
            .set_library("theodb_rs")
            .enable_spi_access()
            .set_restart_time(Some(std::time::Duration::from_secs(5)))
            .load();
    }
}

#[pg_guard]
#[no_mangle]
pub extern "C-unwind" fn theodb_embed_worker_main(_arg: pgrx::pg_sys::Datum) {
    use pgrx::bgworkers::*;
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);
    BackgroundWorker::connect_worker_to_spi(Some(WORKER_DBNAME), None);
    let owner = format!("bgw-{}", unsafe { pgrx::pg_sys::MyProcPid });

    while BackgroundWorker::wait_latch(Some(std::time::Duration::from_secs(WORKER_POLL_SECS))) {
        // Phase 1 — claim a batch (its own txn; the committed lease protects the jobs across phases).
        let jobs: Vec<(i64, i32, String, String)> = BackgroundWorker::transaction(|| {
            Spi::connect(|c| {
                let t = match c.select(
                    "SELECT job_id, vectorizer_id, source_pk, op \
                     FROM theodb_rs._vectorizer_claim_batch($1, $2, $3, $4)",
                    None,
                    &[owner.clone().into(), WORKER_BATCH.into(), WORKER_LEASE_SECS.into(), WORKER_MAX_ATTEMPTS.into()],
                ) {
                    Ok(t) => t,
                    Err(_) => return Vec::new(),
                };
                t.map(|r| {
                    (
                        r.get::<i64>(1).ok().flatten().unwrap_or_default(),
                        r.get::<i32>(2).ok().flatten().unwrap_or_default(),
                        r.get::<String>(3).ok().flatten().unwrap_or_default(),
                        r.get::<String>(4).ok().flatten().unwrap_or_default(),
                    )
                })
                .collect()
            })
        });
        if jobs.is_empty() {
            continue;
        }

        let (mut processed, mut failed) = (0i64, 0i64);
        for (job_id, vid, pk, op) in jobs {
            // Phase 2 — process in its own txn; PgTryBuilder converts an embed/write longjmp into `false`
            // (the txn commits nothing on failure) so the worker never dies on a poison endpoint (B1).
            let is_delete = op == "delete";
            let ok: bool = BackgroundWorker::transaction(|| {
                PgTryBuilder::new(|| {
                    let call = if is_delete {
                        "SELECT theodb_rs._vectorizer_process_delete($1, $2)"
                    } else {
                        "SELECT theodb_rs._vectorizer_process_upsert($1, $2)"
                    };
                    Spi::run_with_args(call, &[vid.into(), pk.clone().into()]).is_ok()
                })
                .catch_others(|_| false)
                .execute()
            });
            // Phase 3 — mark the outcome in a fresh txn (owner-guarded; a lost lease affects 0 rows).
            BackgroundWorker::transaction(|| {
                let sql = if ok {
                    format!("SELECT theodb_rs._vectorizer_mark_done({job_id}, '{owner}')")
                } else {
                    format!(
                        "SELECT theodb_rs._vectorizer_mark_failed({job_id}, '{owner}', 'embed/upsert failed', {WORKER_MAX_ATTEMPTS})"
                    )
                };
                let _ = Spi::run(&sql);
            });
            if ok {
                processed += 1;
            } else {
                failed += 1;
            }
        }
        BackgroundWorker::transaction(|| {
            let _ = Spi::run_with_args(
                "SELECT theodb_rs._vectorizer_bump_stats($1, $2)",
                &[processed.into(), failed.into()],
            );
        });
    }
}

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

    #[pg_test]
    fn process_delete_nulls_target_embedding() {
        Spi::run("CREATE TEMP TABLE docs (id int PRIMARY KEY, body text, emb vector(3))").unwrap();
        let vid: i32 = Spi::get_one(
            "SELECT theodb.create_vectorizer('docs'::regclass, 'id', 'body', 'docs', 'emb', 'm', 3)",
        )
        .unwrap()
        .unwrap();
        Spi::run("INSERT INTO docs VALUES (7, 'x', '[1,2,3]')").unwrap();
        Spi::run(&format!("SELECT theodb_rs._vectorizer_process_delete({vid}, '7')")).unwrap();
        let is_null: bool =
            Spi::get_one("SELECT emb IS NULL FROM docs WHERE id=7").unwrap().unwrap();
        assert!(is_null, "process_delete nulls the target embedding for the removed source row");
    }

    #[pg_test]
    fn stats_reflect_worker_bumps() {
        Spi::run("SELECT theodb_rs._vectorizer_bump_stats(3, 1)").unwrap();
        Spi::run("SELECT theodb_rs._vectorizer_bump_stats(2, 0)").unwrap();
        let (processed, failed): (i64, i64) = Spi::connect(|c| {
            let r = c.select("SELECT processed, failed FROM theodb.vectorizer_stats()", None, &[]).unwrap().first();
            (r.get::<i64>(1).unwrap().unwrap(), r.get::<i64>(2).unwrap().unwrap())
        });
        assert_eq!((processed, failed), (5, 1), "vectorizer_stats() sums the worker's processed/failed bumps");
    }
}
