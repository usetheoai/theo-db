# ADR 0017 — Manutenção do índice HNSW a escala: híbrido tombstone-in-place + fold-para-compaction

**Status:** Accepted · **Date:** 2026-07-07 · **Milestone:** M55 · **Deciders:** CTO (paulohenriquevn)
**Relacionado:** ADR `0014` (M48 — fold crash-safe, fundação da compaction), ADR-1/M35 (grafo imutável + rebuild total — a origem do muro), ADR `0002` (North Star), ADR `0006` (own-code)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m55-vacuum-fold-decision-blueprint.md`
**Evidência:** `docs/benchmarks/m55-vacuum-wall.{md,json}` (baseline do muro), `docs/benchmarks/m48-am-maintenance.md` (WAL do fold)

## Contexto e problema

O desenho do ADR-1/M35 (grafo HNSW imutável + rebuild total no VACUUM) tem um **muro estrutural** confirmado no código: o fold do VACUUM materializa o corpus O(N) em RAM (`build.rs:228,239`, múltiplas cópias vivas no pico) sob o advisory lock **EXCLUSIVE** (`build.rs:176` → `lock.rs:25`), que cobre ~todo o rebuild. Na escala North-Star (1M+×768d) isso significa **GBs de RAM** (pico estimado ~6-10 GB, medido/projetado em `m55-vacuum-wall`) e **parada total de queries vetoriais durante o VACUUM** — além de um scan longo (SHARE) poder bloquear o VACUUM indefinidamente. O mesmo `collect_corpus` sem teto (`build.rs:28`) limita o **BUILD** (CREATE INDEX/REINDEX). É dívida **classe-bloqueador** de qualquer claim v1.0/produção. A decisão exige discover próprio (este ADR), não um fix apressado.

## Drivers da decisão

1. **North Star = igualar/superar AlloyDB** — que pressupõe manutenção sem cliff de latência a escala.
2. **Anti-sunk-cost + reuso (CLAUDE.md, Regra 9):** o M48 já entregou o fold crash-safe (`fold.rs` — pivot atômico + reclaim boundado), declarado "fundação de qualquer fold incremental" (`ADR 0014:52`). Reusar, não descartar.
3. **Measurement-first (ADR 0002):** a decisão é ancorada no baseline medido do muro (`m55-vacuum-wall`).
4. **Evidência de peers permissivos (Regra 9):** pgvector (in-place 4-pass com reparo) e pgvectorscale/DiskANN (tombstone-only + compaction diferida) são dois pontos do espectro, ambos production-viable.
5. **Esforço ≠ Complexidade:** a implementação decorrente é ALTO esforço e ganha milestone próprio; a complexidade escolhida é a mínima que fecha o muro sem big-bang.

## Opções consideradas

- **(A) Fold incremental puro** (gerações parciais sobre o meta-pivot do M48).
- **(B) In-place completo à la pgvector** (`hnswvacuum.c` — 4 passes page-level + máquina de `version`).
- **(C) Híbrido faseado** — tombstone in-place por página p/ DELETE + fold O(N) do M48 p/ compaction. ← **escolhida**
- **(status quo)** rebuild total sob EXCLUSIVE.

## Decisão

**Escolhida: (C) híbrido faseado.**

- **Fase 1 — tombstone-only in-place** (espelha pgvectorscale `plain/node.rs:60`, `vacuum.rs:23`): DELETE marca a element-tuple in-place por página (O(#deletados) RAM, buffer-level lock, **sem O(N), sem parada total**); o scan filtra tombstones. Reúso de slot no INSERT (padrão `hnswinsert.c:45`).
- **Compaction** — reusa o **fold O(N) crash-safe do M48 (`fold.rs`)**, disparado por threshold (`theodb.vacuum_pending_threshold`, já medido). O meta-pivot atômico continua o único ponto de rewrite, e continua crash-safe.
- **Teto de memória do BUILD** (no escopo desta decisão): `collect_corpus` (`build.rs:28`) passa a alimentar o grafo **incrementalmente** (via o próprio caminho de insert do grafo) em vez de materializar o `Vec` inteiro — alinha build e maintenance no mesmo caminho incremental. Raiz idêntica ao fold.

### Alternativas rejeitadas

- **(A) fold incremental puro:** REJEITADA como opção autônoma — HNSW não faz merge de gerações barato; ou vira edição in-place (=B) ou multiplica o custo de scan por #gerações (colide com a vitória O(ef·M) do M35). Absorvida como o *lado compaction* de C.
- **(B) in-place completo (4 passes + version machine):** REJEITADA como *primeira* fase — quebra a invariante grafo-imutável do M35, exige a máquina de `version`, é reescrita grande (`ADR 0014:60`). **Mantida como fase 2 opcional de C** (adicionar o `RepairGraph` in-place do pgvector, `hnswvacuum.c:371`) **se** a medição de recall entre compactions sob tombstone-only mostrar degradação inaceitável — a incerteza-chave declarada no blueprint (nosso grafo pode não ter a redundância de α-pruning do DiskANN).
- **(status quo):** REJEITADA — dívida classe-bloqueador de qualquer claim v1.0 (`ROADMAP.md`, `public-copy.md §3`).

## Plano de milestone(s) de implementação (decorrente — via `/roadmap-feature` após este ADR)

1. **M-impl-1 (fase 1):** tombstone in-place (`aminsert`/`ambulkdelete`/element-tuple `deleted`+`version`) + reúso de slot no INSERT + scan filtra tombstones. **Medir recall entre compactions** (decide se fase 2 é necessária).
2. **M-impl-2:** compaction por threshold reusando `fold.rs`; teto de memória do BUILD (`collect_corpus` batched).
3. **M-impl-3 (fase 2, condicional):** `RepairGraph` in-place se M-impl-1 mostrar degradação.

## Consequências

- **Bom:** fecha o muro de RAM + parada-total no caminho DELETE (o caso comum) sem jogar fora M35/M48; compaction O(N) rara e já crash-safe; mesma decisão resolve o teto do BUILD.
- **Ruim / risco:** fragmentação/degradação de recall entre compactions (a medir); a implementação da fase 1 quebra parcialmente a imutabilidade do M35 no caminho de delete (magic bump + REINDEX + CHANGELOG `Changed`).
- **Trigger v1.0 (LOCKED por este ADR):** a implementação da fase 1 é **pré-requisito de qualquer claim produção/v1.0** (`public-copy.md §3`). Enquanto o rebuild-total-sob-EXCLUSIVE for o único mecanismo, nenhum claim de "production-ready" é honesto na escala North-Star.

## Quando este ADR pode mudar

Novo ADR. Re-open trigger: a medição de recall da fase 1 exigir a fase 2 (in-place completo), OU um peer permissivo entregar um merge-de-gerações HNSW barato que torne A viável.
