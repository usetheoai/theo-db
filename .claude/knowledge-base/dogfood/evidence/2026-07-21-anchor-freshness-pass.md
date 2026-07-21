---
scenario: theo-data-capability-on-theodb
date: 2026-07-21
operator: paulohenriquevn
outcome: pass
summary: The anchor's freshness half passes end-to-end on self-hosted TheoDB — the async vectorizer background worker embeds 5/5 fresh rows with the queue draining to zero failures, superseding the 2026-07-20 partial (#132 did not reproduce).
---

# Anchor evidence — vectorizer freshness half passes (supersedes the 2026-07-20 partial)

## What was exercised

The **freshness** half of the anchor `theo-data-capability-on-theodb`: a declared vectorizer
(`theodb.create_vectorizer`) whose **background worker** keeps the embedding fresh as content changes — on a
self-hosted TheoDB the team runs (droplet 165.227.121.20, PG17.10 pgrx-install, `shared_preload_libraries =
'theodb_rs'`, real OpenAI embeddings, `text-embedding-3-small`, 1536-d).

The 2026-07-20 evidence recorded `outcome: partial` because this half appeared broken (issue #132: every embed job
dead-lettered). That is what this run re-tests.

## Result — pass

| Check | Observed |
|---|---|
| Queue cleared, 5 fresh rows inserted into the source table | `pending 5` (trigger + enqueue work) |
| Worker polled (90 s) | **queue EMPTY — 0 rows, 0 `failed`** |
| `count(*), count(embedding)` in the chunk table for those 5 pks | **5 chunks, 5 embeddings** |
| Worker startup line | `theodb vectorizer worker: embedding_endpoint=set embedding_model=set api_key_len=164` |

Repeated before **and** after the M132 code change — both runs 5/5 with an empty queue, so the pass is not an
artifact of the change.

## Why the 2026-07-20 run looked broken

Two things, neither of which was a worker defect:

1. **Chunk-mode reads the wrong column.** All vectorizers on the source table declare `chunk_strategy='fixed'`, and
   chunk-mode writes embeddings to the **chunk table** (`source_pk, chunk_index, chunk_text, embedding`), NOT to the
   source table's column. `df_docs.embedding IS NULL` is the expected shape; the freshness check must query the
   chunk table.
2. **Probable (not proven): the worker booted without the embedding GUCs.** The repro requires `ALTER SYSTEM` plus a
   restart so the worker starts with them; a restart that silently fails leaves the worker blind while later
   sessions succeed — exactly the discriminator reported. The historical log was rotated, so this stays *probable*.
   The M132 startup line now answers it definitively if it recurs.

## Honest scope — what this evidence does NOT claim

- It does **not** move the anchor to `running`. Per `rules/dogfood-golden-rule.md § 2`, `running` means the anchor is
  **actively used by the team on real infrastructure** over a sustained window — a single passing exercise is not
  that. Status stays `wired`; this records that the blocker which justified `partial` is gone.
- The query half (`ai.hybrid_search_rrf` two-leg RRF fusion) was proven separately on 2026-07-20
  (`2026-07-20-anchor-smoke.md`); this run covers the freshness half only.
- Self-hosted, single operator. The golden rule's soft cap "evidence from ≥ 2 different operators" is still unmet.

## Failure modes still open

- 7 duplicate vectorizers have accumulated on the same source table across dogfood runs. Harmless here but it makes
  queue accounting confusing; noted as a follow-up rather than silently cleaned.
- `create_vectorizer` does not backfill pre-existing rows (recorded in the 2026-07-20 evidence, still true).
