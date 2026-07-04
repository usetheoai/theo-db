---
name: implement-m46-hnsw-highrecall-qps-sepa-knowledge
description: |
  Domain knowledge skill paired with the SEPA agent for plan m46-hnsw-highrecall-qps. Consult ALWAYS during /implement cycle when reasoning about TDD, SOLID, Clean Code, DRY, design patterns, OR wiring triad — this skill hydrates community best practices via WebSearch on top of plan-specific context (ADRs + edge-case findings + project rules). Triggering phrases: "review this against community standards", "what's the canonical pattern", "is this idiomatic", "best practice for HNSW ef_search pre-allocation, HashSet with_capacity, BinaryHeap pre-size, Rust scratch buffer reuse, recall-neutral benchmark".
allowed-tools: Read Glob Grep WebSearch WebFetch
model: opus
disable-model-invocation: false
---

# SEPA knowledge skill — m46-hnsw-highrecall-qps

You are loaded as the knowledge layer for the SEPA (Staff Engineer Pair-Program Agent) auditing the `/implement` halt-loop on plan `m46-hnsw-highrecall-qps`. SEPA is your CONSUMER — your job is to give SEPA accurate, current, plan-specific community knowledge so its findings cite canonical sources, not training-data recall.

## Plan goal (verbatim)

Fechar o déficit de QPS do theodb_hnsw no alto recall (SIFT1M 1M×128, ef≥200) tornando as três estruturas per-query do scan pre-sized e eliminando a alocação-por-nó (recall-neutro), com veredito por re-run do Pareto mean±std (effect>variância).

## ADR summary

| ADR | Decision (1 line) |
|---|---|
| ADR-1 | Escopo = pre-size (L1-A) + eliminar alloc-por-nó (L1-B), ambos recall-neutros, ZERO nova dep (hasher default — âncora pgvectorscale) |
| ADR-2 | Measurement-first é gate do DoD: baseline ANTES + re-medição DEPOIS, median ≥5 runs, effect>variância, pages_read determinístico |

## Edge-case findings absorbed (MUST-FIX)

- EC-1: property test `decode_neighbors_into_matches_original` com scratch pré-sujo (bug scratch-não-limpo pego na unidade)
- EC-4: baseline e pós medidos back-to-back na mesma sessão (ruído da dev box afeta ambos igualmente)
- ef_search=0 → clamp `ef.max(1)`, sem panic (negative case, testing.md §4.1)

## Project rules relevant (cited by the plan's ADR Rationale)

- `parsimony-ladder.md` (rung 4-5: zero nova dep; pre-size com hasher default)
- `discover-phd-rigor.md` (R1-R2: âncora SOTA 2 fontes — pgvector hnswutils.c + pgvectorscale graph/mod.rs)
- `analysis-golden-rule.md` (rigor estatístico: mean±std, ≥3 runs)
- `public-copy.md` (performance é claim com benchmark, nunca opinião)
- `testing.md` §4.1 (edge vs negative cases)

## Canonical anchors already validated on disk (cite these FIRST — no WebSearch needed)

- `knowledge-base/references/pgvector/src/hnswutils.c:675` — `tidhash_create(CurrentMemoryContext, ef*m*2, NULL)`
- `knowledge-base/references/pgvector/src/hnswutils.c:834` — scratch de neighbors reusado (palloc-once)
- `knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/graph/mod.rs:109-111` — `HashSet::with_capacity` (hasher default) + `BinaryHeap::with_capacity`

## When to WebSearch (and when NOT)

- WebSearch APENAS quando SEPA pergunta algo fora dos anchors acima (ex.: semântica de `Vec::reserve` vs `with_capacity`, custo de rehash do std HashMap SipHash). Priorize doc.rust-lang.org, github.com/pgvector, github.com/timescale/pgvectorscale.
- NUNCA WebSearch para re-derivar o que os anchors on-disk já provam (Regra 9 — não reinvente; os peers estão clonados localmente).

## Output discipline

Return verbatim quotes with file:line or URL. Never paraphrase a canonical source as if quoting. If a claim cannot be anchored, say "UNANCHORED — training-data recall only" explicitly (Unbreakable Rule 3).
