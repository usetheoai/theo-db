//! Vectorizer job-queue logic (M54, ADR 0016). The crash-safe HEART of the declarative auto-embedding
//! pipeline, sliced OUT of the background worker so 100% of the queue state machine is testable via
//! `#[pg_test]` WITHOUT a running worker, `shared_preload_libraries`, or an OpenAI endpoint (blueprint § Fatia
//! de testabilidade; MEMORY m46: CI does not run `cargo pgrx test`, pgrx test sets no preload).
//!
//! The worker main (elsewhere) composes: `claim_batch` (txn, commits the lease) → process incl. `embed_batch`
//! (its own txn, subtransaction-isolated so a caught endpoint ERROR rolls back clean — council H-1) →
//! `mark_done`/`mark_failed` (txn, owner-guarded). The COMMITTED lease — not a held lock — protects a job
//! between phases; the worker renews the lease before each fallback job so a live worker never loses one it is
//! processing. Every transition is fenced by a globally-unique `owner` uuid so a slow-but-alive worker whose
//! lease expired and was reclaimed cannot clobber the new owner (H1). Attempts are burned ON CLAIM so a job
//! that kills the worker before reporting counts down to the `failed` dead-letter (H3); a reaper dead-letters
//! orphans stuck at the cap. HONEST tradeoff (council HIGH-1): the embed HTTP runs INSIDE a transaction
//! (synchronous, like dblink / pgsql-http — `embed` reads GUCs via SPI, which needs a txn); the per-request
//! timeout bounds the snapshot hold. A fully async embed (read cfg → commit → HTTP → write) is a tracked
//! follow-up so the xmin horizon is never pinned by a hung endpoint.
use pgrx::prelude::*;

// The declarative config + the crash-safe job queue (ADR 0016). `owner` is an opaque text fencing token
// (a worker-generated uuid rendered as text — text keeps the marshalling trivial; the fencing works
// identically). `state` is a typed CHECK enum (pending/processing/failed — 'done' jobs are DELETEd, à la
// pgmq.archive). The partial-ish claim index covers the `SKIP LOCKED` scan.
extension_sql!(
    r#"
CREATE TABLE IF NOT EXISTS theodb.vectorizer (
    id             serial PRIMARY KEY,
    source_table   text NOT NULL,
    source_pk_col  text NOT NULL,
    content_col    text NOT NULL,
    target_table   text NOT NULL,
    target_col     text NOT NULL,
    model          text,
    dims           int,
    -- M66: opt-in declarative chunking. NULL chunk_strategy → the v1 in-place mode (1 doc → 1 vector,
    -- non-breaking). Non-NULL → 1 doc → N chunks → N rows in the `{target_table}_chunks` table.
    chunk_strategy text,
    chunk_size     int NOT NULL DEFAULT 512,
    chunk_overlap  int NOT NULL DEFAULT 64,
    created_at     timestamptz NOT NULL DEFAULT now()
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

-- Partial: excludes `failed` dead-letter tombstones so the claim scan stays enxuto as they accumulate
-- (council-index-storage M-3). Covers both the `pending` and the `processing`-reclaim branches.
CREATE INDEX IF NOT EXISTS vectorizer_queue_claim_idx
    ON theodb.vectorizer_queue (state, enqueued_at)
    WHERE state IN ('pending', 'processing');

-- M104 producer backpressure via COALESCING: at most ONE pending job per (vectorizer_id, source_pk). A bulk
-- backfill / repeated UPDATE that re-enqueues the same row is de-duplicated (ON CONFLICT DO NOTHING on the
-- enqueue), so the pending queue depth is bounded by the DISTINCT pending work set — the single worker cannot be
-- flooded past the number of distinct rows that actually changed (the audit's producer-faster-than-consumer gap).
CREATE UNIQUE INDEX IF NOT EXISTS vectorizer_queue_pending_uniq
    ON theodb.vectorizer_queue (vectorizer_id, source_pk)
    WHERE state = 'pending';
"#,
    name = "theodb_vectorizer_schema",
    requires = ["theodb_schema_bootstrap"], // M70: schema theodb criado pelo theodb_rs (flip ADR-D1)
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
        VALUES (vid, to_jsonb(OLD) ->> pkcol, 'delete')
        -- coalesce: one pending job per (vectorizer,PK); last op wins (a delete after a pending upsert must
        -- supersede it — the net state is "deleted"). enqueued_at is preserved to keep FIFO order (a hot row
        -- re-touched forever cannot starve older work by resetting its position).
        ON CONFLICT (vectorizer_id, source_pk) WHERE state = 'pending' DO UPDATE SET op = EXCLUDED.op;
        RETURN OLD;
    END IF;
    INSERT INTO theodb.vectorizer_queue (vectorizer_id, source_pk, op)
    VALUES (vid, to_jsonb(NEW) ->> pkcol, 'upsert')
    ON CONFLICT (vectorizer_id, source_pk) WHERE state = 'pending' DO UPDATE SET op = EXCLUDED.op;
    RETURN NEW;
END;
$fn$;

-- Declarative registration: record the config + attach the enqueue trigger to the source table. Returns the
-- vectorizer id. Idempotent-ish: a second call on the same table creates a second vectorizer/trigger (by id)
-- — callers dedupe by not re-registering. The AFTER trigger name is namespaced by id so several can coexist.
CREATE FUNCTION theodb.create_vectorizer(
    source_table   regclass,
    source_pk_col  text,
    content_col    text,
    target_table   regclass,
    target_col     text,
    model          text DEFAULT NULL,
    dims           int  DEFAULT NULL,
    chunk_strategy text DEFAULT NULL,
    chunk_size     int  DEFAULT 512,
    chunk_overlap  int  DEFAULT 64
) RETURNS int LANGUAGE plpgsql AS $fn$
DECLARE
    vid    int;
    tgname text;
BEGIN
    -- Validate chunking config at the boundary (fail-fast, Rule 8) — the chunker also validates, but the
    -- DDL should reject a bad config before any row is processed.
    IF chunk_strategy IS NOT NULL THEN
        IF chunk_strategy NOT IN ('fixed','sentence','recursive') THEN
            RAISE EXCEPTION 'theodb.create_vectorizer: unknown chunk_strategy % (valid: fixed, sentence, recursive)', chunk_strategy;
        END IF;
        IF chunk_size <= 0 OR chunk_overlap < 0 OR chunk_overlap >= chunk_size THEN
            RAISE EXCEPTION 'theodb.create_vectorizer: require chunk_size > 0 and 0 <= overlap < chunk_size (got size %, overlap %)', chunk_size, chunk_overlap;
        END IF;
    END IF;
    INSERT INTO theodb.vectorizer (source_table, source_pk_col, content_col, target_table, target_col, model, dims, chunk_strategy, chunk_size, chunk_overlap)
    VALUES (source_table::text, source_pk_col, content_col, target_table, target_col, model, dims, chunk_strategy, chunk_size, chunk_overlap)
    RETURNING id INTO vid;

    -- M66: when chunking is enabled, provision the sibling chunk table (1 doc → N chunks). Idempotent.
    IF chunk_strategy IS NOT NULL THEN
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %s_chunks (source_pk text NOT NULL, chunk_index int NOT NULL, '
            'chunk_text text NOT NULL, embedding vector%s, PRIMARY KEY (source_pk, chunk_index))',
            target_table::text, CASE WHEN dims IS NULL THEN '' ELSE '('||dims||')' END);
    END IF;

    tgname := format('theodb_vectorizer_%s', vid);
    EXECUTE format(
        'CREATE TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %s '
        'FOR EACH ROW EXECUTE FUNCTION theodb._vectorizer_enqueue(%L, %L)',
        tgname, source_table::text, vid::text, source_pk_col);
    RETURN vid;
END;
$fn$;

COMMENT ON FUNCTION theodb.create_vectorizer(regclass, text, text, regclass, text, text, int, text, int, int) IS
  'Declaratively maintain an embedding column: attach an AFTER INSERT/UPDATE/DELETE trigger to source_table '
  'that enqueues jobs into theodb.vectorizer_queue; the background worker drains them (M54, ADR 0016). The '
  'trigger only enqueues (cheap, no HTTP) — model latency stays off the writer transaction.';

-- (M66) the v1 plpgsql `theodb.chunk_text` was removed here — superseded by the Rust `theodb.chunk`
-- (fixed/sentence/recursive + overlap, Unicode-safe; `theodb_rs::chunk`, wired in api.rs). One chunker (KISS).

REVOKE ALL ON FUNCTION theodb.create_vectorizer(regclass, text, text, regclass, text, text, int, text, int, int) FROM PUBLIC;

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
        let rows = Spi::connect_mut(|client| {
            let tbl = client
            .update(
                "UPDATE theodb.vectorizer_queue \
                 SET state='processing', owner=$1, lease_deadline=now() + make_interval(secs => $3), \
                     attempts = attempts + 1 \
                 WHERE job_id IN ( \
                   SELECT job_id FROM theodb.vectorizer_queue \
                   WHERE ((state='pending' AND (lease_deadline IS NULL OR lease_deadline < now())) \
                          OR (state='processing' AND lease_deadline < now())) \
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
        Spi::connect_mut(|client| {
            client
                .update(
                    "WITH d AS (DELETE FROM theodb.vectorizer_queue \
                 WHERE job_id=$1 AND owner=$2 AND state='processing' RETURNING 1) \
                 SELECT count(*) FROM d",
                    None,
                    &[job_id.into(), owner.into()],
                )
                .ok()
                .and_then(|t| t.first().get::<i64>(1).ok().flatten())
                .map(|n| n > 0)
                .unwrap_or(false)
        })
    }

    /// Owner-guarded failure. Recoverable vs terminal is decided IN SQL by `attempts` (burned on claim): below
    /// `max_attempts` the job returns to `pending` (owner/lease cleared → immediately re-claimable, a bounded
    /// retry); at/over the cap it becomes `failed` (dead-letter — never an infinite loop, H3). `last_error` is
    /// recorded (Rule 8 — never swallow). Returns true iff the guarded row matched (false = lease lost, discard).
    #[pg_extern]
    fn _vectorizer_mark_failed(job_id: i64, owner: &str, err: &str, max_attempts: i32) -> bool {
        // Sanitize AT THE SINK (council-security MEDIUM): redact credential-shaped runs and bound the length here, so
        // no present or future caller can persist a secret (or an unbounded blob) into `last_error`.
        // `super::` — this fn lives inside the `#[pg_schema] mod theodb_rs`; the helper is at file scope.
        let err = &super::sanitize_error_text(err);
        // M144 T2.3: exponential backoff. A failed-but-recoverable job returns to `pending` with a FUTURE
        // `lease_deadline` (not NULL) so `_vectorizer_claim_batch` skips it until the backoff elapses — a
        // transient endpoint outage no longer re-fires the whole backlog in a tight loop. Backoff =
        // 2^attempts seconds capped at 300; the exponent is capped at 12 BEFORE `power()` so a large
        // `attempts` (e.g. 60) saturates the cap instead of overflowing (EC-3). Dead-lettered jobs
        // (attempts >= max) keep `lease_deadline = NULL` — they are terminal, never reclaimed.
        Spi::connect_mut(|client| {
            client
                .update(
                    "WITH u AS (UPDATE theodb.vectorizer_queue \
                 SET state = CASE WHEN attempts >= $4 THEN 'failed' ELSE 'pending' END, \
                     owner = NULL, last_error = $3, \
                     lease_deadline = CASE WHEN attempts >= $4 THEN NULL \
                         ELSE now() + make_interval(secs => least(power(2, least(attempts, 12))::int, 300)) END \
                 WHERE job_id=$1 AND owner=$2 AND state='processing' RETURNING 1) \
                 SELECT count(*) FROM u",
                    None,
                    &[job_id.into(), owner.into(), err.into(), max_attempts.into()],
                )
                .ok()
                .and_then(|t| t.first().get::<i64>(1).ok().flatten())
                .map(|n| n > 0)
                .unwrap_or(false)
        })
    }

    /// Owner-guarded lease renewal for long batches: extend `lease_deadline` for jobs still owned by `owner`.
    /// The worker calls this at ~⅓ of the lease interval while `embed_batch` runs, so a live worker never loses
    /// jobs it is actively processing. Returns the count of jobs renewed (jobs whose lease was already lost are
    /// silently skipped — they belong to another owner now). `job_ids` is a bigint[].
    #[pg_extern]
    fn _vectorizer_renew_lease(job_ids: Vec<i64>, owner: &str, lease_secs: i32) -> i64 {
        Spi::connect_mut(|client| {
            client
                .update(
                    "WITH u AS (UPDATE theodb.vectorizer_queue \
                 SET lease_deadline = now() + make_interval(secs => $3) \
                 WHERE job_id = ANY($1) AND owner=$2 AND state='processing' RETURNING 1) \
                 SELECT count(*) FROM u",
                    None,
                    &[job_ids.into(), owner.into(), lease_secs.into()],
                )
                .ok()
                .and_then(|t| t.first().get::<i64>(1).ok().flatten())
                .unwrap_or(0)
        })
    }

    /// Look up a vectorizer's config. Diverges (typed) if the id is unknown. `model` may be NULL.
    /// Resolved vectorizer config. `chunk_strategy == None` → the v1 in-place mode (1 doc → 1 vector);
    /// `Some(_)` → the M66 chunk-table mode (1 doc → N chunks → N rows in `{target_table}_chunks`).
    pub(crate) struct VecCfg {
        source_table: String,
        source_pk_col: String,
        content_col: String,
        target_table: String,
        target_col: String,
        model: Option<String>,
        pub(crate) chunk_strategy: Option<String>,
        chunk_size: i32,
        chunk_overlap: i32,
    }

    fn lookup_config(vectorizer_id: i32) -> VecCfg {
        Spi::connect(|c| {
            let t = c
            .select(
                "SELECT source_table, source_pk_col, content_col, target_table, target_col, model, \
                 chunk_strategy, chunk_size, chunk_overlap FROM theodb.vectorizer WHERE id=$1",
                None,
                &[vectorizer_id.into()],
            )
            .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer config lookup failed: {e:?}")));
            if t.is_empty() {
                crate::pg::err_input(&format!("vectorizer {vectorizer_id} not found"));
            }
            let r = t.first();
            VecCfg {
                source_table: r.get::<String>(1).ok().flatten().unwrap_or_default(),
                source_pk_col: r.get::<String>(2).ok().flatten().unwrap_or_default(),
                content_col: r.get::<String>(3).ok().flatten().unwrap_or_default(),
                target_table: r.get::<String>(4).ok().flatten().unwrap_or_default(),
                target_col: r.get::<String>(5).ok().flatten().unwrap_or_default(),
                model: r.get::<String>(6).ok().flatten(),
                chunk_strategy: r.get::<String>(7).ok().flatten(),
                chunk_size: r.get::<i32>(8).ok().flatten().unwrap_or(512),
                chunk_overlap: r.get::<i32>(9).ok().flatten().unwrap_or(64),
            }
        })
    }

    /// M66 chunk-table upsert: chunk `content` → embed each chunk in ONE round-trip → replace the doc's chunk
    /// rows atomically (DELETE the PK's old chunks, then INSERT the new ones — no orphans on re-embed).
    fn upsert_chunks(cfg: &VecCfg, source_pk: &str, content: &str) {
        let strategy = cfg.chunk_strategy.as_deref().unwrap_or("recursive");
        let chunks = crate::chunk::chunk(
            content,
            strategy,
            cfg.chunk_size as usize,
            cfg.chunk_overlap as usize,
        );
        let chunk_tbl = format!("{}_chunks", cfg.target_table);
        // Always clear the doc's existing chunks first (re-embed must not leave orphans).
        let del = format!("DELETE FROM {chunk_tbl} WHERE source_pk = $1");
        Spi::run_with_args(&del, &[source_pk.into()]).unwrap_or_else(|e| {
            crate::pg::err_input(&format!("vectorizer chunk delete failed: {e:?}"))
        });
        if chunks.is_empty() {
            return; // empty/whitespace doc → no chunks, no embed call
        }
        let items: Vec<Option<&str>> = chunks.iter().map(|s| Some(s.as_str())).collect();
        let vecs = crate::embed::run_batch(&items, cfg.model.as_deref()); // ONE HTTP round-trip for N chunks
        let ins = format!(
            "INSERT INTO {chunk_tbl} (source_pk, chunk_index, chunk_text, embedding) VALUES ($1, $2, $3, $4::vector)"
        );
        for (i, (chunk_text, vec_text)) in chunks.iter().zip(vecs.iter()).enumerate() {
            Spi::run_with_args(
                &ins,
                &[
                    source_pk.into(),
                    (i as i32).into(),
                    chunk_text.as_str().into(),
                    vec_text.as_str().into(),
                ],
            )
            .unwrap_or_else(|e| {
                crate::pg::err_input(&format!("vectorizer chunk insert failed: {e:?}"))
            });
        }
    }

    /// Build one dynamic SQL string with Postgres-native `format()` over SPI. COLUMN identifiers use `%I`
    /// (quoted). TABLE names use `%s` — but they come from `create_vectorizer`'s `regclass` params stored as
    /// `regclass::text`, which Postgres already renders as a safely-quoted, schema-qualified identifier (not raw
    /// user text), so `%s` on them is injection-safe. Config is owner-controlled server-side, not request input.
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
        let cfg = lookup_config(vectorizer_id);
        let fetch_q = build_sql(
            "SELECT %1$I::text FROM %2$s WHERE %3$I::text = $1",
            &cfg.content_col,
            &cfg.source_table,
            &cfg.source_pk_col,
        );
        let content =
            Spi::get_one_with_args::<String>(&fetch_q, &[source_pk.into()]).ok().flatten();
        // M66: chunk-table mode (opt-in) writes N chunk rows; the default in-place mode writes 1 vector.
        if cfg.chunk_strategy.is_some() {
            upsert_chunks(&cfg, source_pk, content.as_deref().unwrap_or(""));
            return;
        }
        // May diverge (ereport) on any embed failure — intentional: the worker's PgTryBuilder converts it to a
        // typed `failed` transition (Rule 8 — never swallow). embed reads GUCs via SPI, so this runs in a txn.
        let vec_text = crate::embed::run(content.as_deref(), cfg.model.as_deref());
        let upd_q = build_sql(
            "UPDATE %2$s SET %1$I = $1::vector WHERE %3$I::text = $2",
            &cfg.target_col,
            &cfg.target_table,
            &cfg.source_pk_col,
        );
        Spi::run_with_args(&upd_q, &[vec_text.into(), source_pk.into()])
            .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer upsert failed: {e:?}")));
    }

    /// Process one `delete` job: clear the target embedding for the removed source row. v1 is idempotent and
    /// safe for the in-place case (target == source: the source row is already gone, so 0 rows) and the sibling
    /// case (nulls the orphan embedding).
    #[pg_extern]
    fn _vectorizer_process_delete(vectorizer_id: i32, source_pk: &str) {
        let cfg = lookup_config(vectorizer_id);
        // M66 chunk-table mode: delete all N chunk rows of the removed doc.
        if cfg.chunk_strategy.is_some() {
            let del = format!("DELETE FROM {}_chunks WHERE source_pk = $1", cfg.target_table);
            // M144 T1.3: propagate the SPI error instead of `let _ =`-swallowing it. A failed delete
            // must NOT be marked `done` by the worker (:918) — an ereport here is trapped by the worker
            // subtxn (:903-913) and routed to `_vectorizer_mark_failed` → M122 dead-letter. Defense-in-depth
            // (audit #76 was marked heuristic): in pgrx 0.19 a DML error already longjmps past `let _ =`, so
            // this closes the rare `Err(SpiError(code))` path and drops the Result-discarding smell. Had a
            // failed delete been swallowed AND returned Ok, the removed doc's embedding would stay searchable
            // (PII). 0 rows affected is `Ok` and still marks done — only a real SPI error propagates.
            Spi::run_with_args(&del, &[source_pk.into()]).unwrap_or_else(|e| {
                crate::pg::err_input(&format!("vectorizer chunk delete failed: {e:?}"))
            });
            return;
        }
        let del_q = build_sql(
            "UPDATE %2$s SET %1$I = NULL WHERE %3$I::text = $1",
            &cfg.target_col,
            &cfg.target_table,
            &cfg.source_pk_col,
        );
        // M144 T1.3: same propagation for the null-out arm (see chunk-arm comment above).
        Spi::run_with_args(&del_q, &[source_pk.into()])
            .unwrap_or_else(|e| crate::pg::err_input(&format!("vectorizer delete failed: {e:?}")));
    }

    /// Process a BATCH of `upsert` jobs from the SAME vectorizer in ONE `embed_batch` HTTP round-trip (DoD:
    /// "worker consome em batch via embed_batch" — the N→1 fix reused from `embed.rs:55`). Fetches all contents,
    /// embeds them in a single call, and upserts + marks each done. On ANY endpoint failure the whole call
    /// diverges (embed_batch longjmps) — the worker traps it and falls back to per-job `_vectorizer_process_upsert`
    /// so a single poison row cannot fail the batch. `job_ids[i]` aligns with `source_pks[i]`. Returns the count
    /// marked done (fencing-guarded: a lost lease contributes 0).
    #[pg_extern]
    fn _vectorizer_process_upsert_batch(
        vectorizer_id: i32,
        job_ids: Vec<i64>,
        source_pks: Vec<String>,
        owner: &str,
    ) -> i64 {
        let cfg = lookup_config(vectorizer_id);
        let fetch_q = build_sql(
            "SELECT %1$I::text FROM %2$s WHERE %3$I::text = $1",
            &cfg.content_col,
            &cfg.source_table,
            &cfg.source_pk_col,
        );
        let contents: Vec<Option<String>> = source_pks
            .iter()
            .map(|pk| {
                Spi::get_one_with_args::<String>(&fetch_q, &[pk.as_str().into()]).ok().flatten()
            })
            .collect();
        // M66 chunk-table mode: each doc fans out to N chunks (already 1 embed_batch round-trip per doc via
        // upsert_chunks). The cross-doc batch optimization is for the 1→1 in-place mode.
        if cfg.chunk_strategy.is_some() {
            let mut done = 0i64;
            for (i, pk) in source_pks.iter().enumerate() {
                upsert_chunks(&cfg, pk, contents[i].as_deref().unwrap_or(""));
                if _vectorizer_mark_done(job_ids[i], owner) {
                    done += 1;
                }
            }
            return done;
        }
        // ONE HTTP round-trip for the whole batch (may diverge on failure — caught by the worker → per-job fallback).
        let items: Vec<Option<&str>> = contents.iter().map(|c| c.as_deref()).collect();
        let vecs = crate::embed::run_batch(&items, cfg.model.as_deref());
        let upd_q = build_sql(
            "UPDATE %2$s SET %1$I = $1::vector WHERE %3$I::text = $2",
            &cfg.target_col,
            &cfg.target_table,
            &cfg.source_pk_col,
        );
        let mut done = 0i64;
        for (i, pk) in source_pks.iter().enumerate() {
            Spi::run_with_args(&upd_q, &[vecs[i].as_str().into(), pk.as_str().into()])
                .unwrap_or_else(|e| {
                    crate::pg::err_input(&format!("vectorizer batch upsert failed: {e:?}"))
                });
            if _vectorizer_mark_done(job_ids[i], owner) {
                done += 1;
            }
        }
        done
    }

    // ── M122 — 3-phase split of the in-place (non-chunk) batch: phase A (read+resolve cfg, this txn) → phase B
    // (embed, NO txn — the worker calls `embed::run_batch_resolved` between the two transactions so `backend_xmin`
    // is released for the HTTP) → phase C (write+mark, a fresh txn). Chunk-mode (M66) keeps the single-txn
    // `_vectorizer_process_upsert_batch` above (documented drawback — it still pins xmin for chunk vectorizers). ──

    /// Phase-A result: everything phase B (the off-txn embed) and phase C (the write) need, as OWNED values so no
    /// PG pointer/Datum crosses the commit boundary (the embed then holds no snapshot).
    pub(crate) struct BatchRead {
        pub(crate) endpoint: String,
        pub(crate) model: String,
        pub(crate) api_key: Option<String>,
        pub(crate) contents: Vec<Option<String>>,
        pub(crate) cfg: VecCfg,
    }

    /// M122 phase A — read the batch's content + resolve the network cfg, inside the caller's txn. Performs NO
    /// write to the target and NO embed. Returns owned values; the worker commits this txn before the embed.
    pub(crate) fn _vectorizer_read_batch(vectorizer_id: i32, source_pks: &[String]) -> BatchRead {
        let cfg = lookup_config(vectorizer_id);
        let (endpoint, model, api_key) = crate::embed::resolve_batch_cfg(cfg.model.as_deref());
        let fetch_q = build_sql(
            "SELECT %1$I::text FROM %2$s WHERE %3$I::text = $1",
            &cfg.content_col,
            &cfg.source_table,
            &cfg.source_pk_col,
        );
        let contents: Vec<Option<String>> = source_pks
            .iter()
            .map(|pk| {
                Spi::get_one_with_args::<String>(&fetch_q, &[pk.as_str().into()]).ok().flatten()
            })
            .collect();
        BatchRead { endpoint, model, api_key, contents, cfg }
    }

    /// M122 phase C — write the already-embedded vectors + mark each job done, inside a fresh txn. Performs NO
    /// embed/HTTP. Idempotent (overwrite-by-pk); `mark_done` is owner-guarded so a stale worker whose lease
    /// expired cannot mark a re-claimed job. Returns the number of jobs marked done.
    pub(crate) fn _vectorizer_write_batch(
        cfg: &VecCfg,
        job_ids: &[i64],
        source_pks: &[String],
        vecs: &[String],
        owner: &str,
    ) -> i64 {
        let upd_q = build_sql(
            "UPDATE %2$s SET %1$I = $1::vector WHERE %3$I::text = $2",
            &cfg.target_col,
            &cfg.target_table,
            &cfg.source_pk_col,
        );
        let mut done = 0i64;
        for (i, pk) in source_pks.iter().enumerate() {
            Spi::run_with_args(&upd_q, &[vecs[i].as_str().into(), pk.as_str().into()])
                .unwrap_or_else(|e| {
                    crate::pg::err_input(&format!("vectorizer batch upsert failed: {e:?}"))
                });
            if _vectorizer_mark_done(job_ids[i], owner) {
                done += 1;
            }
        }
        done
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

    /// Dead-letter orphans stuck in `processing` at the attempt cap. A worker that crashed AFTER burning the
    /// last attempt (on claim) but BEFORE reporting leaves a job `processing` that the claim can never reclaim
    /// (`attempts < max` is false) — without this reaper it would leak in `processing` forever, inflating the
    /// metric and never reaching the `failed` dead-letter (council-index-storage HIGH-2). Returns the count reaped.
    #[pg_extern]
    fn _vectorizer_reap_orphans(max_attempts: i32) -> i64 {
        Spi::connect_mut(|client| {
            client
            .update(
                "WITH r AS (UPDATE theodb.vectorizer_queue \
                 SET state='failed', last_error='lease expired at attempt cap (worker crashed before reporting)' \
                 WHERE state='processing' AND attempts >= $1 AND lease_deadline < now() RETURNING 1) \
                 SELECT count(*) FROM r",
                None,
                &[max_attempts.into()],
            )
            .ok()
            .and_then(|t| t.first().get::<i64>(1).ok().flatten())
            .unwrap_or(0)
        })
    }

    /// M104 — bound the dead-letter: keep the most recent `keep` `failed` rows (highest job_id), delete older ones.
    /// Without this, a persistent poison row or a mis-set endpoint accumulates `failed` tombstones on-disk forever
    /// (the audit's unbounded on-disk dead-letter finding) — the partial claim index hides the growth from the hot
    /// path, so it is silent. `done` jobs are already DELETEd; this bounds the retained `failed` history. Returns the
    /// count purged. Called by the worker's periodic maintenance (`theodb.vectorizer_dead_letter_max`, default 1000).
    #[pg_extern]
    fn _vectorizer_purge_dead_letters(keep: i32) -> i64 {
        Spi::connect_mut(|client| {
            client
            .update(
                "WITH d AS (DELETE FROM theodb.vectorizer_queue WHERE state='failed' AND job_id NOT IN \
                 (SELECT job_id FROM theodb.vectorizer_queue WHERE state='failed' ORDER BY job_id DESC LIMIT $1) \
                 RETURNING 1) SELECT count(*) FROM d",
                None,
                &[keep.max(0).into()],
            )
            .ok()
            .and_then(|t| t.first().get::<i64>(1).ok().flatten())
            .unwrap_or(0)
        })
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

/// Run `f` inside an internal subtransaction so a caught Postgres ERROR leaves clean SPI/snapshot state
/// before the outer `BackgroundWorker::transaction` commits (council-rust-pgrx H-1: `PgTryBuilder::catch`
/// only `FlushErrorState`s — it does NOT abort the (sub)transaction; committing a dirty one warns/PANICs
/// under `--enable-cassert`). Returns `Some(f())` when `f` succeeded (subtxn released into the parent),
/// `None` when `f` raised (subtxn rolled back).
/// M132 (#132): the caught error's SQLSTATE + message are RETURNED, not discarded. Before, `catch_others(|_| None)`
/// threw the cause away and every failure was marked with the same eight-word literal, so a 401, a missing
/// embedding GUC and a malformed response were indistinguishable in `last_error` — that blindness is what made
/// #132 cost a day of debugging. `Err(msg)` keeps the identical rollback semantics; only the diagnosis is added.
fn in_subtxn_msg<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    unsafe {
        pgrx::pg_sys::BeginInternalSubTransaction(std::ptr::null());
    }
    let res: Result<T, String> = PgTryBuilder::new(std::panic::AssertUnwindSafe(|| Ok(f())))
        .catch_others(|e| {
            let report = match &e {
                pgrx::pg_sys::panic::CaughtError::PostgresError(r)
                | pgrx::pg_sys::panic::CaughtError::ErrorReport(r) => r,
                pgrx::pg_sys::panic::CaughtError::RustPanic { ereport, .. } => ereport,
            };
            Err(format!("{:?}: {}", report.sql_error_code(), report.message()))
        })
        .execute();
    unsafe {
        if res.is_ok() {
            pgrx::pg_sys::ReleaseCurrentSubTransaction();
        } else {
            pgrx::pg_sys::RollbackAndReleaseCurrentSubTransaction();
        }
    }
    res
}

/// The `Option` view for call sites that only branch on success/failure (the cause is surfaced by the caller that
/// records `last_error`). Single source of subtransaction handling — no duplicated BEGIN/ROLLBACK logic.
fn in_subtxn<T>(f: impl FnOnce() -> T) -> Option<T> {
    in_subtxn_msg(f).ok()
}

/// Sanitize an error message before it is PERSISTED in `last_error` (council-security MEDIUM).
///
/// The embed path echoes up to 200 chars of the endpoint's response body into its error
/// (`embed.rs: "unexpected embedding response shape: {body}"`). That echo was previously log-only — logs rotate.
/// M132 persists the cause in a table row that survives in the dead-letter, so an endpoint mistakenly pointed at an
/// echo/debug service (which reflects request headers in its 200 body) would write `Authorization: Bearer <token>`
/// into durable storage. Redact credential-shaped runs FIRST, then bound the length.
///
/// Applied at the SINK (`_vectorizer_mark_failed`), not at the call site, so a future caller cannot bypass it.
/// No regex dependency (parsimony rung 2 — plain scanning is enough for these two shapes).
pub(crate) fn sanitize_error_text(cause: &str) -> String {
    const MAX: usize = 400;
    const REDACTED: &str = "«redacted»";
    // A credential run ends at whitespace or a JSON/quote delimiter.
    fn is_token_char(c: char) -> bool {
        !c.is_whitespace() && c != '"' && c != '\'' && c != ',' && c != '}' && c != ')'
    }
    // M144 T2.2: the patterns we match (`bearer `, `sk-`) are pure ASCII. Compare each ORIGINAL char
    // case-insensitively via `to_ascii_lowercase` (which never changes the char count) instead of
    // indexing a separately-built `to_lowercase()` vector with the SAME index — Unicode length-changing
    // lowercasing (e.g. 'İ' → "i̇") desyncs the two vectors and MISALIGNS the redaction, leaving stray
    // credential characters in the sanitized output (proven on the droplet — no full-secret leak found,
    // but the `sk`/scheme prefix survived). One index space removes the desync entirely.
    fn ascii_ci_prefix(chars: &[char], at: usize, pat: &[char]) -> bool {
        pat.iter()
            .enumerate()
            .all(|(k, &pc)| chars.get(at + k).is_some_and(|&c| c.to_ascii_lowercase() == pc))
    }
    let mut out = String::with_capacity(cause.len());
    let chars: Vec<char> = cause.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        // `Bearer <token>` (case-insensitive) → keep the scheme, drop the credential.
        let is_bearer = ascii_ci_prefix(&chars, i, &['b', 'e', 'a', 'r', 'e', 'r', ' ']);
        // `sk-…` style API keys (OpenAI and lookalikes) with a meaningful length.
        let is_sk = ascii_ci_prefix(&chars, i, &['s', 'k', '-']);
        if is_bearer {
            out.push_str("Bearer ");
            i += 7;
            while i < chars.len() && is_token_char(chars[i]) {
                i += 1;
            }
            out.push_str(REDACTED);
            continue;
        }
        if is_sk {
            let start = i;
            let mut j = i;
            while j < chars.len() && is_token_char(chars[j]) {
                j += 1;
            }
            if j - start >= 20 {
                out.push_str(REDACTED);
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    // Bound the stored text so one poison row cannot bloat the queue table. Char-boundary safe (byte slicing could
    // split a multi-byte UTF-8 sequence and panic).
    if out.chars().count() <= MAX {
        return out;
    }
    out.chars().take(MAX).collect::<String>() + "…(truncated)"
}

/// M132 (#132): the worker's view of the embedding config, as ONE log line at startup.
///
/// The most probable cause of the original report is a worker that booted WITHOUT the `ALTER SYSTEM` embedding
/// GUCs (a restart that silently did not take effect) — invisible today, and instantly obvious with this line.
/// The api key is reported by LENGTH ONLY: a secret must never reach the server log.
pub(crate) fn startup_config_line(
    endpoint: Option<&str>,
    model: Option<&str>,
    api_key: Option<&str>,
) -> String {
    format!(
        "theodb vectorizer worker: embedding_endpoint={} embedding_model={} api_key_len={}",
        if endpoint.is_some() { "set" } else { "MISSING" },
        if model.is_some() { "set" } else { "MISSING" },
        api_key.map(|k| k.len()).unwrap_or(0),
    )
}

// M145 T1.2: helpers extraídos de `theodb_embed_worker_main` (CC 41 → ≤ 25 por lizard). As duas closures
// (`renew`/`process_one`) viram fns livres com `owner: &str` explícito; o 3-phase embed vira `process_group`.
// Os LIMITES DE TRANSAÇÃO (M122 xmin / H-1 poison-isolation / H1 fencing) são movidos INTACTOS — nunca fundidos
// nem partidos. Comportamento preservado; a única nuance é a semântica do sigterm-break do embed (ver `process_group`).

/// Reaper (council HIGH-2) — dead-letter de órfãos no attempt-cap + purge M104 do dead-letter em disco. Um txn.
fn reap_and_purge() {
    use pgrx::bgworkers::BackgroundWorker;
    BackgroundWorker::transaction(|| {
        let _ = Spi::run_with_args(
            "SELECT theodb_rs._vectorizer_reap_orphans($1)",
            &[WORKER_MAX_ATTEMPTS.into()],
        );
        let keep = crate::am::guc::vectorizer_dead_letter_max();
        let _ = Spi::run_with_args(
            "SELECT theodb_rs._vectorizer_purge_dead_letters($1)",
            &[keep.into()],
        );
    });
}

/// Phase 1 — claim a batch no seu próprio txn (o lease committado protege os jobs através das fases).
fn claim_batch(owner: &str) -> Vec<(i64, i32, String, String)> {
    use pgrx::bgworkers::BackgroundWorker;
    BackgroundWorker::transaction(|| {
        Spi::connect_mut(|c| {
            let t = match c.update(
                "SELECT job_id, vectorizer_id, source_pk, op \
                 FROM theodb_rs._vectorizer_claim_batch($1, $2, $3, $4)",
                None,
                &[
                    owner.to_string().into(),
                    WORKER_BATCH.into(),
                    WORKER_LEASE_SECS.into(),
                    WORKER_MAX_ATTEMPTS.into(),
                ],
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
    })
}

/// Renova o lease de um job para o intervalo cheio antes do seu embed (council H-3 / M-2). Um txn.
fn renew_lease(owner: &str, job_id: i64) {
    use pgrx::bgworkers::BackgroundWorker;
    BackgroundWorker::transaction(|| {
        let _ = Spi::run_with_args(
            "SELECT theodb_rs._vectorizer_renew_lease($1, $2, $3)",
            &[vec![job_id].into(), owner.to_string().into(), WORKER_LEASE_SECS.into()],
        );
    });
}

/// Processa UM job (subtxn-isolado por H-1) + mark owner-guarded, cada um no seu txn (fallback + delete).
/// Um ERROR de embed/write capturado rola o subtxn de volta a um estado limpo e marca failed. Retorna o
/// resultado do mark OWNER-GUARDED (`false` = lease perdido → o job pertence a outro worker; não conta).
fn process_one(owner: &str, job_id: i64, vid: i32, pk: &str, is_delete: bool) -> bool {
    use pgrx::bgworkers::BackgroundWorker;
    let pk = pk.to_string();
    // M132: mantém a causa REAL para o `last_error` nomeá-la.
    let outcome = BackgroundWorker::transaction(|| {
        in_subtxn_msg(|| {
            let call = if is_delete {
                "SELECT theodb_rs._vectorizer_process_delete($1, $2)"
            } else {
                "SELECT theodb_rs._vectorizer_process_upsert($1, $2)"
            };
            Spi::run_with_args(call, &[vid.into(), pk.clone().into()])
                .expect("vectorizer process job failed");
        })
    });
    BackgroundWorker::transaction(|| {
        // Ambos os braços usam parâmetros ligados (council-security LOW: sem interpolação assimétrica).
        match &outcome {
            Ok(()) => Spi::get_one_with_args::<bool>(
                "SELECT theodb_rs._vectorizer_mark_done($1, $2)",
                &[job_id.into(), owner.to_string().into()],
            )
            .ok()
            .flatten()
            .unwrap_or(false),
            Err(cause) => {
                let _ = Spi::run_with_args(
                    "SELECT theodb_rs._vectorizer_mark_failed($1, $2, $3, $4)",
                    &[
                        job_id.into(),
                        owner.to_string().into(),
                        cause.clone().into(),
                        WORKER_MAX_ATTEMPTS.into(),
                    ],
                );
                false
            }
        }
    })
}

/// Processa UM grupo (mesmo vectorizer) via o 3-phase embed (M122): Phase A read (txn próprio, libera o snapshot
/// antes do embed) → Phase B embed SEM txn aberto (backend_xmin liberado no HTTP; PgTryBuilder trapa o longjmp) →
/// Phase C write+mark (txn fresco). Chunk-mode / single-txn GUC caem no caminho single-txn. Zero-row ou embed-fail
/// → per-job fallback (poison-row isolation). Retorna `(processed, failed)` do grupo.
///
/// Nuance de sigterm (preservada, não é mudança de comportamento): um sigterm no meio do embed (pós-Phase-B) faz
/// `return` deste grupo em vez de `break` do loop externo; o re-check `sigterm_received()` no topo do loop de
/// grupos do `main` quebra na próxima iteração — terminação equivalente (um re-check a mais, nenhum grupo extra).
fn process_group(owner: &str, vid: i32, group: Vec<(i64, String)>) -> (i64, i64) {
    use pgrx::bgworkers::BackgroundWorker;
    let (mut processed, mut failed) = (0i64, 0i64);
    let job_ids: Vec<i64> = group.iter().map(|(j, _)| *j).collect();
    let pks: Vec<String> = group.iter().map(|(_, p)| p.clone()).collect();

    // Phase A — READ content + resolve cfg no txn próprio; o commit LIBERA o snapshot antes do embed
    // (backend_xmin não fica pinado no HTTP). subtxn-isolado (H-1) → bad-cfg/SPI error → `None` → fallback.
    let read = BackgroundWorker::transaction(|| {
        in_subtxn(|| theodb_rs::_vectorizer_read_batch(vid, &pks))
    });

    let batch_done: Option<i64> = match read {
        None => None,
        Some(r) if r.cfg.chunk_strategy.is_some() || crate::am::guc::vectorizer_single_txn() => {
            // Chunk-mode (M66) / GUC single-txn (default off, A/B de medição) mantêm o caminho single-txn.
            BackgroundWorker::transaction(|| {
                in_subtxn(|| {
                    Spi::connect_mut(|c| {
                        c.update(
                            "SELECT theodb_rs._vectorizer_process_upsert_batch($1, $2, $3, $4)",
                            None,
                            &[
                                vid.into(),
                                job_ids.clone().into(),
                                pks.clone().into(),
                                owner.to_string().into(),
                            ],
                        )
                        .expect("vectorizer batch failed")
                        .first()
                        .get::<i64>(1)
                        .ok()
                        .flatten()
                        .unwrap_or(0)
                    })
                })
            })
        }
        Some(r) => {
            // Renova o lease do grupo inteiro (um txn) antes do embed ≤~90s.
            BackgroundWorker::transaction(|| {
                let _ = Spi::run_with_args(
                    "SELECT theodb_rs._vectorizer_renew_lease($1, $2, $3)",
                    &[job_ids.clone().into(), owner.to_string().into(), WORKER_LEASE_SECS.into()],
                );
            });
            // Phase B — EMBED sem txn aberto e sem SPI: backend_xmin liberado no HTTP inteiro. Um err_* tipado
            // longjmpa; sem txn para capturar aqui, o PgTryBuilder trapa e roteia ao per-job fallback.
            let items: Vec<Option<&str>> = r.contents.iter().map(|c| c.as_deref()).collect();
            let embedded: Option<Vec<String>> =
                PgTryBuilder::new(std::panic::AssertUnwindSafe(|| {
                    Some(crate::embed::run_batch_resolved(
                        &items,
                        &r.endpoint,
                        &r.model,
                        r.api_key.as_deref(),
                    ))
                }))
                .catch_others(|_| None)
                .execute();
            if BackgroundWorker::sigterm_received() {
                return (processed, failed); // ver a nuance de sigterm no doc-comment (equivalente ao break externo)
            }
            match embedded {
                None => None, // embed falhou (5xx/malformed/timeout) → per-job fallback
                // Phase C — WRITE os vetores + MARK done num txn fresco (overwrite idempotente; mark owner-guarded).
                Some(vecs) => BackgroundWorker::transaction(|| {
                    in_subtxn(|| {
                        theodb_rs::_vectorizer_write_batch(&r.cfg, &job_ids, &pks, &vecs, owner)
                    })
                }),
            }
        }
    };
    match batch_done {
        // M132 (ADR M132-2): só um batch que processou linhas conta como done; `Some(0)` cai no fallback observável.
        Some(n) if n > 0 => processed += n,
        _ => {
            for (job_id, pk) in &group {
                if BackgroundWorker::sigterm_received() {
                    break;
                }
                renew_lease(owner, *job_id);
                if process_one(owner, *job_id, vid, pk, false) {
                    processed += 1
                } else {
                    failed += 1
                }
            }
        }
    }
    (processed, failed)
}

#[pg_guard]
#[unsafe(no_mangle)]
pub extern "C-unwind" fn theodb_embed_worker_main(_arg: pgrx::pg_sys::Datum) {
    use pgrx::bgworkers::*;
    BackgroundWorker::attach_signal_handlers(SignalWakeFlags::SIGHUP | SignalWakeFlags::SIGTERM);
    BackgroundWorker::connect_worker_to_spi(Some(WORKER_DBNAME), None);
    // A globally-unique fencing token (uuid, NOT the reusable pid) so a restarted worker never inherits a
    // previous incarnation's identity — the fencing (owner=$owner) depends on this uniqueness (council M-1).
    let owner = BackgroundWorker::transaction(|| {
        Spi::get_one::<String>("SELECT 'bgw-' || gen_random_uuid()::text")
            .ok()
            .flatten()
            .unwrap_or_else(|| format!("bgw-{}", unsafe { pgrx::pg_sys::MyProcPid }))
    });

    // M132 (#132): report the worker's OWN view of the embedding config once at startup, subtxn-isolated (a
    // diagnostic must never kill the thing it exists to diagnose — council-rust-pgrx LOW).
    BackgroundWorker::transaction(|| {
        let _ = in_subtxn(|| {
            let line = startup_config_line(
                crate::pg::guc("theodb.embedding_endpoint").as_deref(),
                crate::pg::guc("theodb.embedding_model").as_deref(),
                crate::pg::guc("theodb.embedding_api_key").as_deref(),
            );
            pgrx::log!("{line}");
        });
    });

    while BackgroundWorker::wait_latch(Some(std::time::Duration::from_secs(WORKER_POLL_SECS))) {
        reap_and_purge();

        let jobs = claim_batch(&owner);
        if jobs.is_empty() {
            continue;
        }

        let (mut processed, mut failed) = (0i64, 0i64);

        // Deletes: per-job (no embed). Upserts: grouped by vectorizer for ONE embed_batch HTTP round-trip.
        let mut groups: std::collections::HashMap<i32, Vec<(i64, String)>> =
            std::collections::HashMap::new();
        for (job_id, vid, pk, op) in jobs {
            if BackgroundWorker::sigterm_received() {
                break;
            }
            if op == "delete" {
                if process_one(&owner, job_id, vid, &pk, true) {
                    processed += 1
                } else {
                    failed += 1
                }
            } else {
                groups.entry(vid).or_default().push((job_id, pk));
            }
        }
        for (vid, group) in groups {
            if BackgroundWorker::sigterm_received() {
                break;
            }
            let (p, f) = process_group(&owner, vid, group);
            processed += p;
            failed += f;
        }

        BackgroundWorker::transaction(|| {
            let _ = Spi::run_with_args(
                "SELECT theodb_rs._vectorizer_bump_stats($1, $2)",
                &[processed.into(), failed.into()],
            );
        });
    }
}

// M104 (review MEDIUM) — least-privilege: the `_vectorizer_*` internals are SECURITY INVOKER helpers the worker
// and enqueue trigger call; no external role should hold EXECUTE on them. The `theodb_rs` schema is not
// blanket-REVOKE'd, so `#[pg_extern]` functions default to PUBLIC EXECUTE — revoke the whole family (the existing
// claim/mark/process/reap set + the new dead-letter purge) to match the codebase's per-function REVOKE convention.
// Dynamic (`::regprocedure`) so there are no fragile hand-written signatures to drift; `~ '^_vectorizer_'` matches
// the family by literal prefix (no LIKE wildcard ambiguity).
extension_sql!(
    r#"
DO $$
DECLARE r record;
BEGIN
    FOR r IN SELECT p.oid::regprocedure AS sig
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
             WHERE n.nspname = 'theodb_rs' AND p.proname ~ '^_vectorizer_'
    LOOP
        EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', r.sig);
    END LOOP;
END $$;
"#,
    name = "theodb_vectorizer_revoke",
    requires = [
        _vectorizer_claim_batch,
        _vectorizer_mark_done,
        _vectorizer_mark_failed,
        _vectorizer_renew_lease,
        _vectorizer_process_upsert,
        _vectorizer_process_delete,
        _vectorizer_process_upsert_batch,
        _vectorizer_bump_stats,
        _vectorizer_reap_orphans,
        _vectorizer_purge_dead_letters,
    ],
);

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
        let claimed: i64 =
            Spi::get_one("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)")
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
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)",
        )
        .unwrap();
        Spi::run("UPDATE theodb.vectorizer_queue SET lease_deadline = now() - interval '1 second'")
            .unwrap();
        // w2 must be able to reclaim the expired job (visibility timeout).
        let reclaimed: i64 =
            Spi::get_one("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w2', 10, 60, 5)")
                .unwrap()
                .unwrap();
        assert_eq!(reclaimed, 1, "expired-lease job is reclaimable by a new owner");
        let owner: String =
            Spi::get_one("SELECT owner FROM theodb.vectorizer_queue").unwrap().unwrap();
        assert_eq!(owner, "w2", "reclaimed job is now owned by w2");
    }

    #[pg_test]
    fn stale_owner_mark_done_affects_zero_rows() {
        seed(1);
        // w1 claims; lease expires; w2 reclaims. w1's late mark_done MUST NOT delete w2's job (H1 fencing).
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)",
        )
        .unwrap();
        Spi::run("UPDATE theodb.vectorizer_queue SET lease_deadline = now() - interval '1 second'")
            .unwrap();
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w2', 10, 60, 5)",
        )
        .unwrap();
        let job_id: i64 =
            Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue").unwrap().unwrap();
        let w1_done: bool =
            Spi::get_one(&format!("SELECT theodb_rs._vectorizer_mark_done({job_id}, 'w1')"))
                .unwrap()
                .unwrap();
        assert!(!w1_done, "stale owner w1 must NOT complete the reclaimed job");
        let still_there: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.vectorizer_queue WHERE owner='w2'")
                .unwrap()
                .unwrap();
        assert_eq!(still_there, 1, "w2's job survives w1's stale mark_done");
        // The real owner w2 completes it → row removed.
        let w2_done: bool =
            Spi::get_one(&format!("SELECT theodb_rs._vectorizer_mark_done({job_id}, 'w2')"))
                .unwrap()
                .unwrap();
        assert!(w2_done, "true owner w2 completes the job");
        let remaining: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.vectorizer_queue").unwrap().unwrap();
        assert_eq!(remaining, 0, "job removed after w2 completes");
    }

    #[pg_test]
    fn mark_failed_dead_letters_at_attempt_cap() {
        seed(1);
        // max_attempts=1: the first claim burns attempts→1; a recoverable failure at the cap dead-letters.
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 1)",
        )
        .unwrap();
        let job_id: i64 =
            Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue").unwrap().unwrap();
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
        assert_eq!(
            state, "failed",
            "at the attempt cap the job is dead-lettered, not retried forever"
        );
        assert_eq!(err, "endpoint 500", "the typed error is recorded (Rule 8 — never swallow)");
    }

    #[pg_test]
    fn mark_failed_below_cap_returns_to_pending() {
        seed(1);
        // max_attempts=3: first claim burns attempts→1; a recoverable failure returns the job to pending.
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 3)",
        )
        .unwrap();
        let job_id: i64 =
            Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue").unwrap().unwrap();
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
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 30, 5)",
        )
        .unwrap();
        let ids: Vec<i64> = Spi::connect(|c| {
            c.select("SELECT job_id FROM theodb.vectorizer_queue ORDER BY job_id", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i64>(1).unwrap())
                .collect()
        });
        let arr = format!(
            "ARRAY[{}]::bigint[]",
            ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
        );
        // w2 (not the owner) renews nothing; w1 renews both.
        let w2n: i64 =
            Spi::get_one(&format!("SELECT theodb_rs._vectorizer_renew_lease({arr}, 'w2', 120)"))
                .unwrap()
                .unwrap();
        assert_eq!(w2n, 0, "a non-owner renews nothing (fencing)");
        let w1n: i64 =
            Spi::get_one(&format!("SELECT theodb_rs._vectorizer_renew_lease({arr}, 'w1', 120)"))
                .unwrap()
                .unwrap();
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
        assert_eq!(
            (pk.as_str(), op.as_str(), state.as_str()),
            ("42", "upsert", "pending"),
            "INSERT enqueues a pending upsert for the row PK"
        );
        // M104 coalescing: UPDATE then DELETE on the SAME never-processed PK collapse into the SINGLE pending
        // job, and the LAST op wins — the net state of insert+update+delete is "delete", so the worker does one
        // delete, not three redundant jobs (producer backpressure without losing the final intent).
        Spi::run("UPDATE docs SET body='changed' WHERE id=42").unwrap();
        Spi::run("DELETE FROM docs WHERE id=42").unwrap();
        let ops: Vec<String> = Spi::connect(|c| {
            c.select("SELECT op FROM theodb.vectorizer_queue ORDER BY job_id", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert_eq!(
            ops,
            vec!["delete"],
            "INSERT+UPDATE+DELETE on one unprocessed PK coalesce to a single 'delete'"
        );
    }

    // (M66) the `chunk_text_*` SPI tests were removed with the dead plpgsql `theodb.chunk_text`; the
    // chunker is now `theodb_rs::chunk` (pure Rust) with its own unit + pg_tests in chunk.rs.

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

    // ── M66 — chunk-table mode (opt-in): schema + catalog + delete (the embed path is droplet/e2e) ──

    #[pg_test]
    fn chunk_mode_creates_chunk_table_and_stores_config() {
        Spi::run("CREATE TABLE cdocs (id int PRIMARY KEY, body text)").unwrap();
        // Opt-in chunking: strategy 'recursive', size 100, overlap 20. target_table = the source (exists as
        // regclass); the chunk table `cdocs_chunks` is derived + provisioned by create_vectorizer.
        let vid: i32 = Spi::get_one(
            "SELECT theodb.create_vectorizer('cdocs'::regclass, 'id', 'body', 'cdocs', 'embedding', 'm', 3, 'recursive', 100, 20)",
        )
        .unwrap()
        .unwrap();
        assert!(vid > 0);
        // The catalog stored the chunking config.
        let (strat, size, ov): (String, i32, i32) = Spi::connect(|c| {
            let r = c
                .select("SELECT chunk_strategy, chunk_size, chunk_overlap FROM theodb.vectorizer WHERE id=$1",
                        None, &[vid.into()]).unwrap().first();
            (
                r.get::<String>(1).unwrap().unwrap(),
                r.get::<i32>(2).unwrap().unwrap(),
                r.get::<i32>(3).unwrap().unwrap(),
            )
        });
        assert_eq!((strat.as_str(), size, ov), ("recursive", 100, 20));
        // The sibling chunk table `cdocs_chunks` was provisioned (source_pk, chunk_index, chunk_text, embedding).
        let cols: i64 = Spi::get_one(
            "SELECT count(*) FROM information_schema.columns WHERE table_name='cdocs_chunks' \
             AND column_name IN ('source_pk','chunk_index','chunk_text','embedding')",
        )
        .unwrap()
        .unwrap();
        assert_eq!(cols, 4, "the target_chunks table has the 4 expected columns");
        Spi::run("DROP TABLE IF EXISTS cdocs_chunks; DROP TABLE cdocs").unwrap();
    }

    #[pg_test]
    fn default_mode_has_null_chunk_strategy() {
        // No chunk_strategy → the v1 in-place mode is preserved (non-breaking).
        Spi::run("CREATE TEMP TABLE ddocs (id int PRIMARY KEY, body text, emb vector(3))").unwrap();
        let vid: i32 = Spi::get_one(
            "SELECT theodb.create_vectorizer('ddocs'::regclass, 'id', 'body', 'ddocs', 'emb', 'm', 3)",
        )
        .unwrap()
        .unwrap();
        let is_null: bool = Spi::get_one(&format!(
            "SELECT chunk_strategy IS NULL FROM theodb.vectorizer WHERE id={vid}"
        ))
        .unwrap()
        .unwrap();
        assert!(is_null, "default (no chunk_strategy) → NULL → in-place mode preserved");
    }

    #[pg_test]
    fn chunk_mode_process_delete_removes_chunk_rows() {
        Spi::run("CREATE TABLE edocs (id int PRIMARY KEY, body text)").unwrap();
        let vid: i32 = Spi::get_one(
            "SELECT theodb.create_vectorizer('edocs'::regclass, 'id', 'body', 'edocs', 'embedding', 'm', 3, 'fixed', 50, 10)",
        )
        .unwrap()
        .unwrap();
        // Seed 3 chunk rows for pk '9' directly (simulating a prior upsert without needing the embed HTTP).
        Spi::run("INSERT INTO edocs_chunks VALUES ('9',0,'a','[1,2,3]'),('9',1,'b','[4,5,6]'),('9',2,'c','[7,8,9]')").unwrap();
        Spi::run(&format!("SELECT theodb_rs._vectorizer_process_delete({vid}, '9')")).unwrap();
        let n: i64 =
            Spi::get_one("SELECT count(*) FROM edocs_chunks WHERE source_pk='9'").unwrap().unwrap();
        assert_eq!(n, 0, "chunk-mode process_delete removes all N chunk rows of the doc");
        Spi::run("DROP TABLE IF EXISTS edocs_chunks; DROP TABLE edocs").unwrap();
    }

    #[pg_test]
    fn stats_reflect_worker_bumps() {
        Spi::run("SELECT theodb_rs._vectorizer_bump_stats(3, 1)").unwrap();
        Spi::run("SELECT theodb_rs._vectorizer_bump_stats(2, 0)").unwrap();
        let (processed, failed): (i64, i64) = Spi::connect(|c| {
            let r = c
                .select("SELECT processed, failed FROM theodb.vectorizer_stats()", None, &[])
                .unwrap()
                .first();
            (r.get::<i64>(1).unwrap().unwrap(), r.get::<i64>(2).unwrap().unwrap())
        });
        assert_eq!(
            (processed, failed),
            (5, 1),
            "vectorizer_stats() sums the worker's processed/failed bumps"
        );
    }

    #[pg_test]
    fn reaper_dead_letters_orphan_stuck_at_cap() {
        seed(1);
        // Claim with max=1 → attempts becomes 1 (== cap). Simulate the worker crashing before it could report:
        // back-date the lease. The claim can NEVER reclaim it (attempts<max is false), so without the reaper it
        // would leak in `processing` forever. The reaper must dead-letter it.
        Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 1)",
        )
        .unwrap();
        Spi::run("UPDATE theodb.vectorizer_queue SET lease_deadline = now() - interval '1 second'")
            .unwrap();
        let stuck: i64 =
            Spi::get_one("SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w2', 10, 60, 1)")
                .unwrap()
                .unwrap();
        assert_eq!(
            stuck, 0,
            "an orphan at the attempt cap is NOT reclaimable (attempts<max false)"
        );
        let reaped: i64 =
            Spi::get_one("SELECT theodb_rs._vectorizer_reap_orphans(1)").unwrap().unwrap();
        assert_eq!(reaped, 1, "the reaper dead-letters the stuck orphan");
        let state: String =
            Spi::get_one("SELECT state FROM theodb.vectorizer_queue").unwrap().unwrap();
        assert_eq!(state, "failed", "reaped orphan is failed, not stuck in processing forever");
    }

    // M104 — the dead-letter purge bounds the on-disk `failed` tombstones: keep the most recent N, delete older.
    #[pg_test]
    fn m104_dead_letter_purge_bounds_failed_rows() {
        seed(10);
        Spi::run("UPDATE theodb.vectorizer_queue SET state='failed'").unwrap();
        let before: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.vectorizer_queue WHERE state='failed'")
                .unwrap()
                .unwrap();
        assert_eq!(before, 10, "10 dead-letter rows before purge");
        let purged: i64 =
            Spi::get_one("SELECT theodb_rs._vectorizer_purge_dead_letters(3)").unwrap().unwrap();
        assert_eq!(purged, 7, "purge removed all but the most recent 3");
        let after: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.vectorizer_queue WHERE state='failed'")
                .unwrap()
                .unwrap();
        assert_eq!(after, 3, "dead-letter bounded to the retained cap");
    }

    // M104 producer backpressure: the enqueue trigger COALESCES — repeated writes to the SAME source row
    // produce at most ONE pending job (bounded queue depth), so a hot row cannot flood the single worker.
    #[pg_test]
    fn m104_enqueue_coalesces_repeated_writes_to_one_pending() {
        Spi::run("CREATE TABLE csrc(id int PRIMARY KEY, body text)").unwrap();
        Spi::run("CREATE TABLE cdst(id int PRIMARY KEY, emb text)").unwrap();
        let vid: i32 =
            Spi::get_one("SELECT theodb.create_vectorizer('csrc','id','body','cdst','emb','m',3)")
                .unwrap()
                .unwrap();
        // One INSERT + three UPDATEs to the SAME row: naive enqueue would create 4 pending jobs.
        Spi::run("INSERT INTO csrc VALUES (1,'a')").unwrap();
        Spi::run("UPDATE csrc SET body='b' WHERE id=1").unwrap();
        Spi::run("UPDATE csrc SET body='c' WHERE id=1").unwrap();
        Spi::run("UPDATE csrc SET body='d' WHERE id=1").unwrap();
        let pending: i64 = Spi::get_one(&format!(
            "SELECT count(*) FROM theodb.vectorizer_queue WHERE vectorizer_id={vid} AND source_pk='1' AND state='pending'"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            pending, 1,
            "4 writes to the same row coalesce into a single pending job (backpressure)"
        );

        // A DISTINCT row still enqueues independently — coalescing is per-(vectorizer,pk), not global.
        Spi::run("INSERT INTO csrc VALUES (2,'x')").unwrap();
        let total: i64 = Spi::get_one(&format!(
            "SELECT count(*) FROM theodb.vectorizer_queue WHERE vectorizer_id={vid} AND state='pending'"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(total, 2, "distinct rows are not coalesced together");
    }

    // M104 (review H1): the bounded-memory GUC is REGISTERED, so `SET` actually takes effect (not silently
    // ignored by an unregistered `current_setting`).
    #[pg_test]
    fn m104_dead_letter_max_guc_is_registered_and_settable() {
        Spi::run("SET theodb.vectorizer_dead_letter_max = 42").unwrap();
        let v: String = Spi::get_one("SELECT current_setting('theodb.vectorizer_dead_letter_max')")
            .unwrap()
            .unwrap();
        assert_eq!(v, "42", "a registered GUC round-trips through SET/current_setting");
    }

    // M104 (review MEDIUM): the `_vectorizer_*` internals are revoked from PUBLIC (least privilege).
    #[pg_test]
    fn m104_vectorizer_internals_revoked_from_public() {
        let purge_public: bool = Spi::get_one(
            "SELECT has_function_privilege('public', 'theodb_rs._vectorizer_purge_dead_letters(integer)', 'EXECUTE')",
        )
        .unwrap()
        .unwrap();
        assert!(!purge_public, "the dead-letter purge internal is NOT executable by PUBLIC");
        let claim_public: bool = Spi::get_one(
            "SELECT has_function_privilege('public', 'theodb_rs._vectorizer_reap_orphans(integer)', 'EXECUTE')",
        )
        .unwrap()
        .unwrap();
        assert!(
            !claim_public,
            "the whole _vectorizer_* family is revoked, not just the new function"
        );
    }

    /// M132 (#132) — the startup line must name what the worker sees WITHOUT ever leaking the key value.
    #[pg_test]
    fn test_m132_startup_log_never_logs_key_value() {
        let secret = "sk-verysecretkeyvalue-0123456789";
        let line = super::startup_config_line(
            Some("https://api.openai.com/v1/embeddings"),
            Some("text-embedding-3-small"),
            Some(secret),
        );
        assert!(
            line.contains(&format!("api_key_len={}", secret.len())),
            "must report the LENGTH: {line}"
        );
        assert!(!line.contains(secret), "the key value must NEVER reach the log: {line}");
        assert!(
            line.contains("embedding_endpoint=set") && line.contains("embedding_model=set"),
            "{line}"
        );

        // A GUC-blind worker (the probable cause of #132) must be identifiable from this one line.
        let blind = super::startup_config_line(None, None, None);
        assert!(blind.contains("embedding_endpoint=MISSING"), "{blind}");
        assert!(blind.contains("api_key_len=0"), "{blind}");
    }

    /// M132 — the caught cause is returned (was discarded), so `last_error` can name it.
    #[pg_test]
    fn test_m132_in_subtxn_returns_real_cause() {
        let ok = super::in_subtxn_msg(|| 7);
        assert_eq!(ok, Ok(7), "success path must be unchanged");

        let err = super::in_subtxn_msg(|| {
            Spi::run("SELECT 1/0").expect("division by zero");
        });
        let cause = err.expect_err("must surface the error");
        assert!(!cause.is_empty(), "the cause must not be empty");
        assert_ne!(cause, "embed/upsert failed", "must NOT be the old blanket literal");
        assert!(
            cause.to_lowercase().contains("divi") || cause.to_lowercase().contains("zero"),
            "the cause must name the real error, got: {cause}"
        );
    }

    /// M132 — the stored cause is bounded and never splits a multi-byte char.
    #[pg_test]
    fn test_m132_sanitize_is_bounded_and_char_safe() {
        assert_eq!(super::sanitize_error_text("short"), "short");
        let long = "é".repeat(1000); // multi-byte: a byte-slice truncation would panic
        let out = super::sanitize_error_text(&long);
        assert!(out.ends_with("…(truncated)"), "must mark truncation");
        assert!(out.chars().count() < 1000, "must be bounded");
    }

    /// M132 (council-security MEDIUM) — a credential echoed back by a misconfigured endpoint must NEVER be
    /// persisted in `last_error`. The embed path echoes up to 200 chars of the response body; an echo/debug
    /// service reflects the request headers, so this is the realistic leak shape.
    #[pg_test]
    fn test_m132_sanitize_redacts_credentials_before_persisting() {
        let echoed = r#"theodb.embed_batch: unexpected embedding response shape: {"headers": {"Authorization": "Bearer sk-proj-AAAABBBBCCCCDDDDEEEEFFFFGGGG"}}"#;
        let safe = super::sanitize_error_text(echoed);
        assert!(
            !safe.contains("sk-proj-AAAABBBBCCCCDDDDEEEEFFFFGGGG"),
            "token must be redacted: {safe}"
        );
        assert!(safe.contains("«redacted»"), "must mark the redaction: {safe}");
        assert!(
            safe.contains("unexpected embedding response shape"),
            "the diagnostic must survive: {safe}"
        );

        // A bare `sk-…` run (no Bearer scheme) is redacted too.
        let bare = super::sanitize_error_text("key was sk-abcdefghijklmnopqrstuvwxyz0123 rejected");
        assert!(
            !bare.contains("sk-abcdefghijklmnopqrstuvwxyz0123"),
            "bare key must be redacted: {bare}"
        );

        // A short `sk-` fragment is NOT a credential — do not mangle ordinary text.
        assert_eq!(super::sanitize_error_text("sk-short"), "sk-short");
    }

    // M144 T2.2 (EC): a Unicode char whose lowercase changes length (İ → "i̇") used to desync the
    // original vs lowercase index vectors, MISALIGNING the redaction — it left stray credential
    // characters in the output (proven on the droplet: `"İİ sk-…"` → old `"İİ sk«redacted»"`, i.e. the
    // `sk` prefix survives; `"…İ Bearer sk-…"` → old `"…İ BBearer «redacted»"`, a corrupted scheme). It
    // did NOT fully leak the secret token in any of the 48+ inputs brute-forced, so this is a redaction-
    // correctness fix, not a proven full-secret leak. The fix compares each ORIGINAL char via
    // `to_ascii_lowercase` (one index space) → clean, aligned redaction. This is a true RED→GREEN: the
    // exact-output assertion FAILS on the old desynced code and PASSES on the fix.
    #[pg_test]
    fn sanitize_redacts_credential_cleanly_after_length_changing_unicode() {
        // 'İ' (U+0130) lowercases to TWO chars. Two of them maximize the desync before the credential.
        let input = "İİ sk-verysecrettoken1234567890abcdefghij";
        let out = super::sanitize_error_text(input);
        // Exact clean output — old code produced "İİ sk«redacted»" (stray "sk"); the fix produces this.
        assert_eq!(
            out, "İİ «redacted»",
            "redaction must be aligned — no stray credential chars after length-changing unicode: {out}"
        );
        // Belt-and-suspenders: no fragment of the credential (not even the `sk` scheme) survives.
        assert!(!out.contains("sk"), "no credential fragment may leak: {out}");
    }

    // M144 T1.3: a delete whose SPI UPDATE fails must DIVERGE (never return `Ok`) so the worker's
    // `in_subtxn_msg` (M132) records `last_error` and the job goes to M122 dead-letter, never `done` —
    // otherwise the removed doc's embedding stays searchable (PII, finding #76).
    //
    // HONEST NOTE (proven on the droplet, 2026-07-23): pgrx 0.19 `Spi::run_with_args` LONGJMPs an
    // elog(ERROR) from the DML (`spi.rs:400-427` — "Postgres will do that for us automatically"); it only
    // returns `Err(SpiError(code))` for a NEGATIVE SPI status (malformed SPI usage), which a fixed
    // `UPDATE … WHERE …` template with bound args never produces. So the raw PG message ("column … does
    // not exist") propagates — the `.unwrap_or_else` prefix fires ONLY on the rare SpiError-code path
    // (defensive, and consistent with the sibling upsert arm at :447). This test therefore locks the
    // SAFETY PROPERTY — process_delete raises (does not silently succeed) on a failed delete — asserting
    // the real propagated substring. Finding #76 is defense-in-depth (audit marked it `heuristic`): with
    // the `let _ =`, the longjmp already bypassed it, so this trigger cannot black-box-distinguish old vs
    // new; the fix hardens the SpiError-code path and removes the Result-discarding smell. See ADR-3.
    #[pg_test(error = "does not exist")]
    fn process_delete_failure_does_not_mark_done() {
        Spi::run("CREATE TABLE dst_bad(id int)").unwrap(); // valid relation, NO 'emb' column
        Spi::run(
            "INSERT INTO theodb.vectorizer (source_table, source_pk_col, content_col, target_table, target_col, model, dims) \
             VALUES ('src','id','body','dst_bad','emb','m',3)",
        )
        .unwrap(); // chunk_size/chunk_overlap use their column DEFAULTs (512/64)
        let vid: i32 = Spi::get_one("SELECT max(id) FROM theodb.vectorizer").unwrap().unwrap();
        // UPDATE dst_bad SET emb = NULL WHERE id::text = '1' → ereport 'column "emb" … does not exist'.
        // Reaching `.unwrap()` would mean process_delete returned Ok (the swallow bug); it must diverge.
        Spi::run(&format!("SELECT theodb_rs._vectorizer_process_delete({vid}, '1')")).unwrap();
    }

    // M144 T1.3 (EC-2 EDGE): deleting an ABSENT doc affects 0 rows — that is `Ok`, not a failure. The
    // fix must propagate only real SPI errors, never treat an empty result as a failure.
    #[pg_test]
    fn process_delete_of_absent_doc_marks_done() {
        Spi::run("CREATE TABLE dst_ok(id int, emb vector)").unwrap(); // valid target
        Spi::run(
            "INSERT INTO theodb.vectorizer (source_table, source_pk_col, content_col, target_table, target_col, model, dims) \
             VALUES ('src','id','body','dst_ok','emb','m',3)",
        )
        .unwrap();
        let vid: i32 = Spi::get_one("SELECT max(id) FROM theodb.vectorizer").unwrap().unwrap();
        // pk '999' does not exist → UPDATE affects 0 rows → Ok. Reaching the end (no ereport) proves it.
        Spi::run(&format!("SELECT theodb_rs._vectorizer_process_delete({vid}, '999')")).unwrap();
    }

    // M144 T2.3: a failed-but-recoverable job returns to `pending` with a FUTURE backoff deadline (not
    // NULL) so the claim skips it until the backoff elapses — no tight re-fire loop on a transient outage.
    #[pg_test]
    fn retry_sets_backoff_deadline() {
        let _vid = seed(1); // 1 pending upsert job
        // claim it: attempts → 1, processing, owned by w1
        let _ = Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 5)",
        );
        let job_id: i64 =
            Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue ORDER BY job_id LIMIT 1")
                .unwrap()
                .unwrap();
        // mark it failed with attempts(=1) < max(=5) → pending + backoff deadline
        let ok: bool = Spi::get_one(&format!(
            "SELECT theodb_rs._vectorizer_mark_failed({job_id}, 'w1', 'transient 503', 5)"
        ))
        .unwrap()
        .unwrap();
        assert!(ok, "mark_failed matched the owned processing row");
        let future: bool = Spi::get_one(&format!(
            "SELECT state='pending' AND lease_deadline > now() FROM theodb.vectorizer_queue WHERE job_id={job_id}"
        ))
        .unwrap()
        .unwrap();
        assert!(future, "failed job re-queued to pending with a FUTURE backoff deadline, not NULL");
    }

    // M144 T2.3 (EC-3 EDGE): the backoff exponent is capped at 12 before power() and the result capped at
    // 300s — a large `attempts` (60) must saturate at 300s, never overflow. This asserts the exact SQL
    // expression used in `_vectorizer_mark_failed`.
    #[pg_test]
    fn backoff_saturates_for_large_attempts() {
        // Exercise the REAL `_vectorizer_mark_failed` path (not a re-computed formula): a job with a large
        // `attempts` must saturate the backoff at the 300s cap without overflowing `power(2, …)`.
        let _vid = seed(1);
        let _ = Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._vectorizer_claim_batch('w1', 10, 60, 1000)",
        );
        let job_id: i64 =
            Spi::get_one("SELECT job_id FROM theodb.vectorizer_queue ORDER BY job_id LIMIT 1")
                .unwrap()
                .unwrap();
        // Force a large attempts count while the job stays owned+processing, then fail it below max(=1000).
        Spi::run(&format!(
            "UPDATE theodb.vectorizer_queue SET attempts = 60 WHERE job_id = {job_id}"
        ))
        .unwrap();
        let ok: bool = Spi::get_one(&format!(
            "SELECT theodb_rs._vectorizer_mark_failed({job_id}, 'w1', 'transient', 1000)"
        ))
        .unwrap()
        .unwrap();
        assert!(ok, "mark_failed matched the owned processing row");
        // 2^least(60,12) = 4096 → least(4096, 300) = 300. Deadline is ~300s ahead, never more.
        let capped: bool = Spi::get_one(&format!(
            "SELECT state='pending' AND lease_deadline > now() + interval '298 seconds' \
                AND lease_deadline <= now() + interval '301 seconds' \
             FROM theodb.vectorizer_queue WHERE job_id = {job_id}"
        ))
        .unwrap()
        .unwrap();
        assert!(capped, "backoff saturates at the 300s cap for large attempts, no overflow");
    }
}
