# /review — M55 Decisão: manutenção do índice a escala (fold vs in-place)

Date: 2026-07-07 · Slug: `vacuum-fold-decision` · milestone_id: M55 · Range: `v0.45.0..HEAD`

## Verdict: READY_TO_MERGE (após review-fixes)

Milestone de **DECISÃO+MEDIÇÃO** (não implementação — a implementação decorrente ganha milestone próprio via `/roadmap-feature` após o ADR). Nenhum código de AM mudou; os artefatos são o blueprint, o ADR 0017, e o harness+benchmark. Review de honestidade do baseline por council-benchmark: **NEEDS_FIXES → READY** (zero HIGH; fixes aplicados).

## DoD (4 itens) — todos cumpridos com evidência

1. **Discover blueprint** — `m55-vacuum-fold-decision-blueprint.md` (2 council agents): fold-incremental vs in-place (pgvector `hnswvacuum.c`) vs híbrido, com evidência dos peers (pgvector repara no vacuum; pgvectorscale tombstone-only + compaction diferida — file:line reais). ✅
2. **Baseline medido** — `docs/benchmarks/m55-vacuum-wall.{md,json}` (harness `run_m55_vacuum_wall.py`): 100k×768d = EXCLUSIVE **~86 s** (parada total, medido, lock real casou) + RSS **~1,44 GB** (VmHWM) + WAL **~340 MB**; projeção O(N) ponto-único 1M = ~14 GB / ~14 min / ~3,4 GB (confiança baixa, marcada). ✅ (250k/1M não couberam na box — RAM gate honesto)
3. **ADR (MADR 3.0)** — `docs/adr/0017`: híbrido tombstone-in-place + fold-para-compaction; alternativas rejeitadas (fold-incremental-puro, in-place-big-bang, status-quo); plano de milestones de implementação; teto de memória do BUILD no escopo. ✅
4. **Trigger v1.0 registrado** — o ADR 0017 fixa a implementação da fase 1 como pré-requisito de qualquer claim produção/v1.0 (`public-copy.md §3`). ✅

## Reviewer + findings (zero HIGH)

**council-benchmark: NEEDS_FIXES → READY** — lente "mediu ou supôs?":
- **Baseline prova o muro honestamente:** `.md`↔`.json` batem campo-a-campo (nenhum número fabricado); os 86 s de EXCLUSIVE são medidos (lock poller não-nulo — o mapeamento advisory casou com o lock real); 1M é PROJEÇÃO marcada como suposição em TODO lugar (nunca reportado como fato).
- **Achado central O(N) CONFIRMADO no código pelo reviewer:** `enumerate_entries` (`hnsw_page.rs:462`) varre todas as páginas e `HnswIndex::build` (`build.rs:239`) reconstrói o grafo inteiro dos 100k — os 86 s/340 MB só existem por O(N), não pelas 500 linhas de pending (que apenas disparam o threshold). A warning `SET …=0 failed` é benigna (default 16 + 500 linhas ≫ 16 → dispara).
- **MEDIUM-C FIXED:** ADR atribuía "~6-10 GB, medido" ao artefato (que projeta 14 GB VmHWM e NÃO mediu private RSS — deu None). Separado: VmHWM projetado vs estimativa analítica; "medido" só p/ os 86 s reais.
- **LOW-A/D FIXED:** CHANGELOG marca a projeção como ponto-único/confiança-baixa; harness default alinhado a 100k/250k.
- Gap honesto (não-bloqueante): `peak_private_rss` deu None (falha do smaps via docker exec) → VmHWM (com shared_buffers=128MB ≈ 9% ruído) é o proxy, divulgado.

## Hard gates
Failing tests: N/A (decisão+medição, sem código de AM). Sem secrets; sem commit em main; sem Co-Authored-By; CHANGELOG + blueprint + ADR + roadmap-run + backlog registrados.

**Verdict:** READY_TO_MERGE
