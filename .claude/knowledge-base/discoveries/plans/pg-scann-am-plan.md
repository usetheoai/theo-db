# Discovery Plan: pg_scann — ScaNN (IVF + AVQ + AH) como Access Method PostgreSQL

> **Version 1.0** — Investiga como transformar o algoritmo do ScaNN (partition IVF + Anisotropic Vector
> Quantization + Asymmetric-Hashing LUT SIMD + rerank) num scan path/Access Method próprio que preserve
> MVCC/WAL/VACUUM/planner. Âncora crítica: o TheoDB **já tem o algoritmo** (own-code, validado por correção) —
> `am/aq.rs` (AVQ anisotrópico) + `vec/ah.rs` (AH-LUT16 pshufb) + `ann/ivf.rs` (k-means++ IVF). O M59/ADR-0019
> mediu que o ganho NÃO materializa no carrier HNSW e apontou o carrier IVF batch-scan com **layout de códigos
> contíguos** como a peça deferida. O blueprint deve fornecer o design de (a) layout de página, (b) scan path
> IVF-AQ+AH, (c) lifecycle transacional, (d) integração com o planner — grounded no SOTA (ScaNN/AlloyDB) e nos
> peers permissivos (vectorchord = IVF+RaBitQ em Postgres; rabitq-rs = IVF FastScan em Rust; pgvector = AM/WAL).

**Slug:** `pg-scann-am`
**Owner:** paulohenriquevn
**Created:** 2026-07-10
**Time budget:** 12h (per-project breakdown em ADR D1). Perfil PhD-rigor (`rules/discover-phd-rigor.md`): tópico P2
vetor/algoritmo → R0 (web obrigatória) + R1 (SOTA anchoring) + R4 (techniques ≥ 2).

## Context

O veredito medido do pilar vetorial (M73/ADR-0035, v0.65.0) fechou a superioridade de QPS vs ScaNN/AlloyDB como
**não-alcançável pelos levers já tentados** (SBQ/M57, AQ+AH-no-HNSW/M59, RaBitQ-1bit/M74). Mas o M59/ADR-0019
concluiu mecanicamente (medido) que o AQ+AH **está correto** (175 pg_tests GREEN) e que o ganho exige um **carrier
de batch-scan contíguo (IVF)**, não o pointer-chasing do HNSW — com o **layout de códigos AQ contíguos separados do
f32** como causa-raiz primária. Esse caminho (AQ+AH-no-IVF) é a **hipótese NÃO-REFUTADA** do pilar. A pesquisa
externa (paper arXiv 2026 sobre filter-agnostic vector search em Postgres, citado pelo owner) reforça: índices de
cluster (tipo ScaNN) podem superar grafos (HNSW/ACORN) em banco real. Este discovery produz o blueprint de design
que permite MEDIR (spike D3) e depois CONSTRUIR o pg_scann. Anchoring: `CLAUDE.md` North Star (Opção α, SOTA
AlloyDB), Rule 9 (compor sobre `am/aq.rs`/`vec/ah.rs`/`ann/ivf.rs` + AM scaffold), D1 (Apache/MIT/BSD/PG; AVQ é
paper implementável), measurement-first/D3, `rules/architecture.md` (domínio sem `pg_sys`; AM na camada de adapter).

## Objective

**Produzir o blueprint de design do pg_scann** (layout + scan + lifecycle + planner) suficiente para desenhar o
spike D3 de viabilidade e, se validado, planejar o AM completo — grounded no ScaNN/AlloyDB (SOTA) e nos peers
permissivos.

- [ ] Todas as research questions respondidas com citações a `.claude/knowledge-base/references/` + web (R0)
- [ ] Tabela cross-cutting populada (ScaNN/AlloyDB vs vectorchord vs rabitq-rs vs pgvector vs nosso aq/ah/ivf)
- [ ] Recommendations com ≥ 1 proposta de decisão concreta por research question (esp. o layout v4 + o design do spike)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/vectorchord/` | `src/index/` (storage.rs, opclass.rs, scanners.rs, mod.rs), `crates/index/src/` (packed.rs, accessor.rs, fetch.rs), `crates/rabitq/src/` | **O análogo permissivo mais próximo**: IVF+RaBitQ como índice PostgreSQL real (layout, scan, lifecycle, opclass, WAL via pgrx) |
| `.claude/knowledge-base/references/rabitq-rs/src/` | `ivf.rs`, `fastscan.rs`, `fastscan_kernel.rs`, `kmeans.rs` | IVF + FastScan LUT em Rust puro — referência de layout contíguo + batch-scan (o "v4 layout") |
| `.claude/knowledge-base/references/pgvector/src/` | `ivfflat.c`, `ivfbuild.c`, `ivfscan.c`, `ivfinsert.c`, `ivfvacuum.c`, `README.md` | Contrato IndexAmRoutine + page format + WAL + VACUUM + amcostestimate de referência (C canônico, PG license) |
| `.claude/knowledge-base/discoveries/blueprints/` | `m33-scann-headtohead-blueprint.md`, `m59-anisotropic-ah-blueprint.md` | **Prior art nosso** (R2): o que já medimos/decidimos sobre AQ+AH, layout e o gap ScaNN |
| Web (allowlist: arxiv.org, research.google, cloud.google.com, *.github.io) | arXiv:1908.10396 (AVQ), AlloyDB blogs, kernelmaker.github.io/pgvector, arXiv 2026 filter-agnostic | R0 SOTA anchoring — algoritmo + Postgres-integration + justificativa técnica |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/*/target/`, `build/`, `docs/`, `tests/fixtures/` | Build artifacts / não-fonte |
| `.claude/knowledge-base/references/vectorchord/` código AGPL para COPIAR | D1: VectorChord é AGPL — **estudar o design, jamais copiar código** ([[vectorchord-agpl-study-only]]). Só rabitq-rs (Apache-2.0) e pgvector (PG license) são vendoráveis |
| Implementação/código do pg_scann | Discovery ASKS, não implementa (é um documento) |
| DiskANN/FAISS clone | Não clonados; cobertos indiretamente por pgvectorscale/rabitq-rs (FastScan) e web se necessário |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** vectorchord: 4h (o análogo mais próximo, mais profundo); rabitq-rs: 3h (layout+FastScan Rust);
pgvector: 3h (contrato AM/WAL/VACUUM); prior-art nossos + web (AVQ/AlloyDB/filter-agnostic): 2h.

**Rationale:** vectorchord é o único peer que faz IVF-quantizado-em-Postgres real (layout+lifecycle+opclass), então
merece o dive mais fundo; rabitq-rs dá o layout contíguo + FastScan sem a camada PG; pgvector dá o contrato AM
canônico. O AVQ/AlloyDB são web (algoritmo + arquitetura proprietária revelada).

**Alternatives considered:** split igual (rejeitado — vectorchord é desproporcionalmente relevante); só vectorchord
(rejeitado — perde o contrato AM canônico do pgvector e o layout puro do rabitq-rs).

**Stop condition — per question (mandatory):** quando a Fase A de uma questão retorna vazio após 3 retries com
variantes diferentes, marcar BLOCKED com razão "Fase A exhausted" e seguir. NUNCA preencher com hotspots de outra questão.

**Stop condition — per project (mandatory):** budget esgotado com N questões pending → marcar as remanescentes
BLOCKED "budget exhausted" e avançar. Se todas as restantes estão `done`/`blocked`, emitir
`<promise>BLUEPRINT_BLOCKED</promise>` com o relatório honesto (NUNCA `BLUEPRINT_COMPLETE` com questões blocked).

**Anti-pattern:** NUNCA fabricar respostas Fase B (Regra 3). Para as fontes WEB (R0), se um arXiv ID/URL não
resolve, marcar a claim `UNVERIFIED` e não citar como fato (honestidade R0/R3) — em especial o arXiv 2026 filter-agnostic.

**Consequences:** o halt-loop para por projeto no budget; questões blocked viram seed do próximo discovery.

### D2 — Investigation depth

**Decision:** Read end-to-end os arquivos de scan/layout/lifecycle dos peers (vectorchord `storage.rs`/`scanners.rs`,
rabitq-rs `fastscan.rs`/`ivf.rs`, pgvector `ivfscan.c`/`ivfvacuum.c`); Grep/ast-grep para mapear os hotspots primeiro.
Para AVQ/AlloyDB (web), WebFetch das URLs allowlisted + citar seção/parágrafo.

**Rationale:** o design de layout+scan+lifecycle exige leitura profunda (intent, edge-cases, WAL/crash-safety), não
só assinatura. Alternativa (só grep) rejeitada — perde o "porquê" do layout.

**Consequences:** dive profundo em poucos arquivos-chave; trade-off: não cobre cada arquivo dos peers (aceitável — foco no scan path).

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — map) | Fase B (deep — Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como o ScaNN/AVQ arranja os códigos AQ **contíguos por partition** e faz o batch-scan AH-LUT (o "v4 layout" que o M59 pediu)? | techniques | rabitq-rs, web (arXiv:1908.10396) | `grep -nE "fn .*scan|pack|block|lut|transpose" rabitq-rs/src/fastscan.rs` + WebFetch arXiv:1908.10396 §AVQ/AH | Read `fastscan.rs`/`fastscan_kernel.rs` end-to-end; ler a seção AVQ+AH do paper | Diagrama do layout contíguo (códigos packed por bloco) + pseudo do batch-scan LUT, com `path:line` + cita arXiv |
| Q2 | Como o **vectorchord** implementa o índice IVF-quantizado em Postgres — page storage, opclass, scan, WAL/pgrx? | techniques | vectorchord | `grep -rnE "GenericXLog|WriteBuffer|page|Opaque|amgettuple|Scanner" vectorchord/src/index/storage.rs vectorchord/src/index/scanners.rs` | Read `storage.rs`, `scanners.rs`, `opclass.rs`, `mod.rs` | Descrição do page format + scan loop + como usa pgrx/WAL, com `path:line` |
| Q3 | Como o **AlloyDB ScaNN** lida com **INSERT/updates transacionais** num índice cujo AQ é treinado offline (pending region? re-partition? re-train trigger?)? | techniques | web (cloud.google.com AlloyDB blogs) | WebFetch understanding-the-scann-index-in-alloydb + how-scann-for-alloydb-vs-pgvector-hnsw | Ler as seções de index maintenance/updates/memory footprint | Prosa: estratégia de update incremental + footprint + citação da URL (marcar UNVERIFIED se não resolver) |
| Q4 | Qual o contrato **IndexAmRoutine** (ambuild/aminsert/amgettuple/amvacuumcleanup) + page format + WAL para um índice IVF crash-safe? | tools | pgvector | `grep -nE "amroutine|ambuild|aminsert|amgettuple|amvacuum|GenericXLog|PageInit" pgvector/src/ivfflat.c pgvector/src/ivfbuild.c pgvector/src/ivfinsert.c` | Read `ivfflat.c`, `ivfbuild.c`, `ivfinsert.c`, `ivfscan.c` nos hotspots | Tabela: callback → responsabilidade → page/WAL usados, com `path:line` |
| Q5 | Como o **VACUUM** e a região pending de um índice IVF-quantizado funcionam (tombstone de tuplas mortas, rebuild parcial)? | tools | pgvector, vectorchord | `grep -nE "vacuum|bulkdelete|tombstone|dead|pending" pgvector/src/ivfvacuum.c vectorchord/src/index/*.rs` | Read `ivfvacuum.c` + o path de vacuum do vectorchord | Descrição do vacuum lifecycle + como marcar/limpar dead codes, com `path:line` |
| Q6 | Que **deps/licenças** o rabitq-rs e o vectorchord puxam para o IVF-FastScan-lifecycle (o que é reusável sob D1)? | deps | rabitq-rs, vectorchord | `grep -nE "^\[dependencies|^\w+ =" rabitq-rs/Cargo.toml vectorchord/Cargo.toml` + `head` dos LICENSE | Read os Cargo.toml + LICENSE; classificar Apache/MIT/BSD vs AGPL | Tabela dep → versão → licença → reusável(sim/estudar-só), com `path:line` |
| Q7 | Como **provar recall×QPS** do scan IVF-AQ+AH vs o frontier ScaNN M33 (SIFT1M) — o design do spike D3 de viabilidade? | tests | prior-art nosso (m33 blueprint), rabitq-rs | `grep -nE "recall|qps|bench|frontier|ground" .claude/knowledge-base/discoveries/blueprints/m33-scann-headtohead-blueprint.md` + `ls rabitq-rs/example*.sh` | Read o blueprint m33 + os exemplos de bench do rabitq-rs | Design do harness do spike (dataset, GT, matched-recall, gate ~2×), com `path:line` |
| Q8 | O que o **nosso m59/m33** já estabeleceu sobre AQ+AH/layout, e o que o **arXiv 2026 filter-agnostic** conclui sobre cluster-vs-grafo em Postgres (SOTA anchor R1/R2)? | prior art | m59/m33 blueprints, web (arXiv 2026) | Read m59-anisotropic-ah-blueprint.md + WebFetch/WebSearch do arXiv 2026 filter-agnostic (VERIFICAR ID) | Ler o veredito mecânico do m59 + a conclusão cluster-vs-grafo do paper | Síntese: o que já sabemos (medido) + o que o SOTA externo confirma; arXiv marcado UNVERIFIED se não resolver |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q7 | Covered |
| Dependencies | Q6 | Covered |
| Tools | Q4, Q5 | Covered |
| Techniques | Q1, Q2, Q3 | Covered (≥2 — R4 PhD-rigor) |
| Prior art (R2 PhD-rigor) | Q8 | Covered |

**Coverage: 5/5 corners covered (100%)** (4 canônicos + prior-art do perfil PhD-rigor).

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | O `.claude/knowledge-base/references/{...}` (ou URL allowlisted) declarado existe/resolve | Marcar Qx BLOCKED "path/URL não resolve", seguir |
| Web source (R0) | WebFetch da URL retornou conteúdo citável (arXiv/blog) | Se não resolve, marcar a claim UNVERIFIED (não fabricar) e seguir |
| Per-question Fase A budget | Fase A retornou ≥1 hotspot OU 3 retries | Após 3 retries vazios, BLOCKED "Fase A exhausted" |
| After answering Qx | Seção do blueprint sob Qx tem ≥1 citação | Re-iterar Qx (1 retry) |
| Mid-loop sanity | Citações a references ≥ N / 200 palavras de prosa | Adicionar citações (1 retry) |
| Before promising complete | Os 5 corners têm seção populada + ≥1 ADR de síntese | Recusar promise, continuar |

## Acceptance Criteria

- [ ] Todas as questões respondidas OU BLOCKED com razão
- [ ] Os 5 corners com seção populada no blueprint
- [ ] Toda citação a `.claude/knowledge-base/references/{...}` resolve; toda claim web citada ou marcada UNVERIFIED (R0/R3)
- [ ] ≥1 ADR de síntese no blueprint (esp. a recomendação de layout v4 + o design do spike D3)
- [ ] SOTA anchoring presente (R1): ScaNN/AlloyDB nomeados + o gap que o pg_scann fecha
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint salvo em `.claude/knowledge-base/discoveries/blueprints/pg-scann-am-blueprint.md`

## Global Definition of Done

- [ ] Todas as fases (plan → edge-cases → plan-confidence → execute → confidence → improve se preciso)
- [ ] Verdict final registrado no header do blueprint
- [ ] Zero citações fabricadas; toda claim de performance/web com fonte ou UNVERIFIED
- [ ] Coverage Matrix 100%
- [ ] ADRs referenciam ≥1 princípio/regra (Rule 9 compor-sobre-existente, D1 licença, architecture.md camadas, measurement-first/D3)
