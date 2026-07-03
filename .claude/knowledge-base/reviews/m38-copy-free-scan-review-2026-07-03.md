# Review — m38-copy-free-scan

**Date:** 2026-07-03 · **Verdict:** READY_TO_MERGE · **Milestone:** M38 (fechado como MEDIÇÃO, não win de QPS)
**Method:** 2 specialist agents (Rust/pgrx FFI-safety + byte-identity · scientific-honesty do desfecho negativo) sobre o commit `8ebafcb`.

## Verdict path

Ambos os agentes: **READY_TO_MERGE**. O ponto central deste milestone é a **honestidade do resultado negativo** — e ambos confirmaram: (a) o refactor é byte-idêntico (recall inalterado) + FFI-seguro; (b) M38 é fechado honestamente, sem claim de QPS escondido em nenhuma das 4 superfícies (artefato .md/.json, CHANGELOG, ROADMAP). O único LOW (figura "~400 syscalls" imprecisa) foi corrigido em `<este commit>` (suavizado para hipótese).

## Findings & resolutions

| # | Sev | Finding | Resolution |
|---|---|---|---|
| 1 | LOW | "~400 syscalls/query" era uma figura de mecanismo imprecisa (o profiler amostra a granularidade de lista ~200; `Instant::now()` usa vDSO, não syscall) | suavizado para "hipótese, não medida" no .md + .json; o **fato medido** (cópia não é o gargalo) permanece, independente da causa exata |
| 2 | INFO | `read_page_item` virou um wrapper de 3 linhas | ≥4 callers vivos (não é dead code); DRY correto — mantido |
| 3 | INFO | bloco M38 do roadmap omite o DoD original | rastreabilidade preservada no blueprint (cita o DoD original + cláusula de escalada) — transparente |

## Confirmed positives (independently verified)

- **Byte-identidade (o gate de recall) — PROVADO (agente Rust, hand-verified):** `read_page_item_into` faz
  `extend_from_slice(from_raw_parts(ptr,len))` num Vec vazio/contínuo → exatamente os mesmos bytes que o antigo
  `to_vec()`+`extend_from_slice`, incluindo o caso de página vazia (append nada). `read_chunked`/`read_blob`
  byte-idênticos. Recall inalterado.
- **FFI seguro:** buffer liberado em TODO path (early-return de página vazia + path normal); a cópia acontece
  ANTES do `UnlockReleaseBuffer` (sem ponteiro pendente); sem leak; sem nova fronteira `extern "C"`; error-handling
  preservado.
- **Honestidade total (agente de honestidade):** NÃO há claim de QPS em nenhuma superfície — título, verdict
  (`Win de QPS? Não.`), JSON `qps_win:false`, CHANGELOG ("sem claim de performance"), ROADMAP ("measurement, não
  um win de QPS") concordam. O ratio favorável `1.52` **não** é cherry-picked — é explicitamente rotulado ruído
  contra 50% de variância. A variância (thermal/carga) é disclosada. A explicação profiler-enganoso é apresentada
  como lição/hipótese, não vira um claim de perf backdoor. O `[x]` é honestidade measurement-first (autorizada pelo
  CLAUDE.md anti-sunk-cost), não goal-massaging.
- **F1 SBQ falsificado honesto:** recall 0.77–0.95 vs 1.0 baseline em SIFT real, números consistentes
  artefato↔blueprint, teoria (escalar perde ranking/byte vs PQ) correta e ancorada em `sbq.rs`.

## Gate results

- Build: `cargo pgrx install --release` 0 warnings.
- Coexistência: **61 testes verdes** (mesmos kNN ids — recall byte-idêntico).
- Artefato `docs/benchmarks/m38-copy-free-scan.{md,json}`: 3 achados honestos (SBQ falsificado; cópia não é o
  gargalo end-to-end; single-copy como byproduct de code-quality). Sem claim de QPS.

## O valor de M38 (honesto)

M38 é um **resultado negativo bem-medido** — o tipo de milestone que a disciplina measurement-first entrega e que
a maioria dos times esconde. Falsificou 2 hipóteses (SBQ-recall, cópia-é-o-gargalo) + entregou um byproduct de
code-quality recall-idêntico. O lever vetorial real restante (PQ) está registrado para um milestone futuro. Nenhum
workaround, nenhum win oco.
