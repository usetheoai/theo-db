# Discovery Plan: AlloyDB Vector/AI Engine — How It's Implemented

> **Version 1.0** — Deep dive em **como o AlloyDB implementa seu motor vetorial/IA** (índice ScaNN + AI engine), reconstruído a partir de material publicado (paper ScaNN, docs oficiais, blog de engenharia) e dos **análogos OSS permissivos** já clonados (`pgvectorscale` StreamingDiskANN+SBQ, `pgvector` HNSW/IVFFlat). O blueprint resultante deve informar **M2** (pilar vetorial/IA) e **M7** (IA avançada) do TheoDB e produzir a evidência que sustenta — ou não — o gatilho de **Política de Fork D3** (PRD).

**Slug:** `alloydb-vector-ai-implementation`
**Owner:** paulohenriquevn (CTO)
**Created:** 2026-06-26
**Time budget:** 9h (per-project breakdown em ADR D1)

## Context

O TheoDB é SOTA-anchored no AlloyDB (CLAUDE.md, regra TheoDB 1) e seu **pilar killer** é o vetorial/IA (`ROADMAP.md` M2/M7; PRD P2). Antes de qualquer aposta de implementação ou do gatilho de fork de `pgvector`/`pgvectorscale` (PRD D3), precisamos entender **as técnicas SOTA que o AlloyDB usa** — não por opinião, mas por evidência (`rules/discover-phd-rigor.md` R1–R3; `public-copy.md`).

Evidência que dispara a discovery AGORA:
- `ROADMAP.md` M2 DoD exige "índice ANN além do HNSW (pgvectorscale StreamingDiskANN), com **recall medido** e **benchmark reproduzível**". Não sabemos como o ScaNN do AlloyDB se compara nem qual técnica fechar.
- `ROADMAP.md` M7 DoD (recém-expandido) exige filtered vector search, hybrid+reranking, e funções `ai.*` — todos espelhados nos `docs/features/` (01–12), que hoje são **especificação sem implementação**.
- PRD **D3** só autoriza fork mediante **benchmark de gatilho reproduzível**. Esta discovery é onde a evidência começa.

**Restrição de honestidade (Regra 3 + `discover-phd-rigor.md` R6):** AlloyDB é **closed-source**. "Como eles implementam" será reconstruído de material **publicado** (paper ScaNN, docs `cloud.google.com`, `research.google`) + **inferido** dos análogos OSS. Onde a evidência primária não for alcançável, a questão é marcada **BLOCKED**; comparações de performance sem número reproduzível são marcadas **UNBENCHMARKED**. Nunca inventar internals proprietários.

## Objective

**Uma frase:** o blueprint deve permitir decidir **qual stack de índice ANN + camada de IA o TheoDB adota para igualar o motor vetorial/IA do AlloyDB usando apenas peças OSS permissivas**, e **se/quando** o fork D3 se justifica.

Critérios de sucesso mensuráveis do blueprint:

- [ ] Todas as research questions respondidas com citações a `.claude/knowledge-base/references/` (ou BLOCKED honesto)
- [ ] Tabela comparativa SOTA preenchida: ScaNN (AlloyDB) × StreamingDiskANN/SBQ (pgvectorscale) × HNSW/IVFFlat (pgvector)
- [ ] ≥ 1 proposta de decisão concreta por research question (ex.: "adotar StreamingDiskANN como índice ANN de M2 porque …")
- [ ] Toda afirmação de performance carrega metodologia+números+fonte **ou** marcador `UNBENCHMARKED` (R3)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/` (`sbq/`, `graph/`, `labels/`, `scan.rs`, `storage.rs`), `README.md`, `DEVELOPMENT.md`, `Cargo.toml`, `tests/`, `scripts/` | Análogo OSS mais próximo do ScaNN (StreamingDiskANN + SBQ) — o ANN avançado de M2 (D3) |
| `.claude/knowledge-base/references/pgvector/` | `src/` (`hnsw*.c`, `ivf*.c`, `vector.c`), `README.md`, `test/`, `META.json`, `Makefile` | Baseline vetorial (tipo + HNSW/IVFFlat) e alvo de customização (D3) |
| Allowlisted web (ScaNN paper + AlloyDB/ScaNN docs) | `arxiv.org` (ScaNN, DiskANN, HNSW papers), `cloud.google.com`, `research.google`, `docs.timescale.com` | Fonte primária da técnica SOTA do AlloyDB (closed-source) — reconstrução publicada |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| Código-fonte do AlloyDB | **Closed-source** — inacessível; reconstruído só de material publicado (R6) |
| `.claude/knowledge-base/references/{citus,hydra,pg_mooncake,duckdb}/` | Subsistema columnar/HTAP (M6) — fora do escopo vetorial/IA deste deep dive |
| `.claude/knowledge-base/references/{patroni,pgbackrest,cloudnative-pg}/` | Subsistema HA/deploy (M4/M5) — fora do escopo |
| `.claude/knowledge-base/references/paradedb/` | `pg_search` BM25 é **AGPL** (D1 — só estudo, não copiar); fora do índice ANN |
| `.claude/knowledge-base/references/*/{target,build,dist}/`, vendor trees | Artefatos de build |
| Web fora de `rules/discover-web-allowlist.txt` | R5 — só fontes autoritativas |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** `pgvectorscale`: 4h · `pgvector`: 2h · web/papers (ScaNN/DiskANN/HNSW + AlloyDB docs): 3h. Total 9h.

**Rationale:** `pgvectorscale` é o análogo mais próximo do ScaNN (StreamingDiskANN+SBQ) e o candidato a fork de D3 → dive mais fundo. `pgvector` é baseline conhecido → menos tempo. A reconstrução do ScaNN do AlloyDB depende de papers/docs (closed-source) → orçamento de web dedicado.

**Alternatives considered:** split igual (rejeitado — pgvectorscale merece mais); só papers (rejeitado — perde os internals reais do análogo OSS); sem budget (rejeitado — halt-loop precisa de stop).

**Stop condition — per question (mandatory):** Fase A vazia após 3 retries com variações → marca a questão **BLOCKED** ("Fase A exhausted") e segue. NUNCA preencher com hotspots de outra questão.

**Stop condition — per project (mandatory):** budget esgotado com questões pendentes → marca-as BLOCKED ("budget exhausted") e avança. Se todas as questões restantes estão `done` ou honestamente `blocked`, emitir `<promise>BLUEPRINT_BLOCKED</promise>` (não `BLUEPRINT_COMPLETE`).

**Anti-pattern:** NUNCA fabricar respostas de Fase B (incl. internals do AlloyDB) para fechar questão exausta. BLOCKED honesto é obrigatório (Regra 3 / R6).

**Consequences:** o halt-loop pára por projeto ao esgotar budget; o blueprint expõe BLOCKED como semente da próxima discovery.

### D2 — Investigation depth

**Decision:** `pgvectorscale`/`pgvector` → ler os arquivos de `access_method`/`src` ponta a ponta (algoritmo + comentários + edge-cases); ScaNN/AlloyDB → ler paper+docs e extrair a técnica, **sem** reivindicar internals não publicados.

**Rationale:** o valor está na técnica (KISS — não auditar o crate inteiro). Internals proprietários do AlloyDB são inferidos só do que é publicado (R6).

**Consequences:** trade-off explícito — profundidade real no OSS; reconstrução publicada (não código) no AlloyDB. Diferenças não publicadas viram `UNBENCHMARKED`/open question.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — ast-grep/grep map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | Como `pgvectorscale` e `pgvector` testam **correção/recall** do índice ANN contra um Postgres real? | tests | `.claude/knowledge-base/references/pgvectorscale/tests/`, `.claude/knowledge-base/references/pgvector/test/` | Glob/grep por arquivos de teste de recall/regressão (`*.rs`/`*.sql`/`*.sql`) e harness de recall em `pgvectorscale/scripts/` | Ler cada fixture + setup; capturar qual container PG/pgvector e qual assertion de recall@k | Tabela: teste → fixture → tipo de assertion (recall@k / correctness), com `path:line` por linha |
| Q2 | Qual a **cadeia de dependências/versões** do `pgvectorscale` (pgrx/Rust, suporte a major do PG) e do build do `pgvector`? | deps | `.claude/knowledge-base/references/pgvectorscale/Cargo.toml`, `.claude/knowledge-base/references/pgvector/{META.json,Makefile}` | Grep por `pgrx`, `pgvector`, `rust-version`, `PG_MAJOR`/`PG_CONFIG` (text-shape — Fase A pode ser pulada) | Ler cada match em contexto; capturar range de versão + majors PG suportados | Range de versões + majors PG suportados + restrições do build chain (informa CI de rebase D3) + citações |
| Q3 | Qual o **build/dev + benchmark** do `pgvectorscale` (cargo pgrx, Docker) e como ele mede recall (harness reproduzível)? | tools | `.claude/knowledge-base/references/pgvectorscale/{DEVELOPMENT.md,Makefile,scripts/}`, `.claude/knowledge-base/references/pgvector/Makefile` | SKIP Fase A (text-shape). Glob por `Makefile`, `DEVELOPMENT.md`, `scripts/*`, `docker*` | Ler cada arquivo; extrair comandos de build, teste e benchmark de recall | Passo-a-passo de build + comando de benchmark reproduzível (alimenta R3 + `docs/benchmarks/`) + citações |
| Q4 | **(SOTA)** Como o **ScaNN** do AlloyDB funciona — anisotropic vector quantization, partitioning (tree), scoring/reordering? Qual o gap p/ peça OSS? | techniques | Allowlist: `arxiv.org` (ScaNN, Guo et al.), `cloud.google.com` (AlloyDB ScaNN), `research.google` | WebFetch (allowlist) do paper ScaNN + doc oficial; grep no `google-research/scann` (github) pela entrypoint | Ler metodologia do paper + doc; extrair as 3 fases (quantize → partition → reorder) | Descrição do algoritmo ScaNN + tabela do gap vs OSS. **R1** anchor AlloyDB/ScaNN; **R2** ≥2 fontes (paper + doc); **R3** números só com metodologia ou `UNBENCHMARKED` |
| Q5 | **(SOTA)** Como o `pgvectorscale` implementa **StreamingDiskANN + Statistical Binary Quantization (SBQ)** — o análogo OSS do ScaNN? | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/` (`sbq/quantize.rs`, `sbq/mod.rs`, `graph/`, `storage.rs`, `scan.rs`), `README.md` (§StreamingDiskANN) | `ast-grep`/grep por `fn quantize`, `struct .*Quantizer`, `Bq`/`Sbq`, `diskann` em `access_method/sbq/` e `graph/` | Ler `quantize.rs` + `graph/` + README §StreamingDiskANN ponta a ponta | Descrição SBQ+graph build/search + parâmetros de tuning. **R1** anchor ScaNN; **R2** repo (in-refs) + DiskANN paper (arxiv); **R3** benchmark do README com metodologia ou `UNBENCHMARKED` |
| Q6 | **(SOTA)** Como `pgvector` implementa **HNSW** e **IVFFlat**, e quais trade-offs (recall/latência/build) vs DiskANN/ScaNN? | techniques | `.claude/knowledge-base/references/pgvector/src/` (`hnswbuild.c`, `hnsw.c`, `hnswscan.c`, `ivfflat.c`, `ivfbuild.c`, `ivfscan.c`), `README.md` | `ast-grep`/grep por `HnswBuild`, `ivfflat`, `m`, `ef_construction`, `lists`, `probes` em `src/` | Ler os builders/scanners + README §HNSW/§IVFFlat | Tabela HNSW×IVFFlat (parâmetros, build cost, recall). **R1** anchor HNSW vs ScaNN; **R2** src + paper HNSW (Malkov&Yashunin, arxiv); **R3** números com fonte ou `UNBENCHMARKED` |
| Q7 | **(SOTA)** Como o AlloyDB faz **filtered vector search** (filtro + ANN no planner) e como `pgvectorscale` faz **label-based filtering**? (M7 DoD) | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/labels/`, `README.md` (§Filtered Vector Search); allowlist `cloud.google.com` (AlloyDB filtered) | grep por `label`, `filter` em `access_method/labels/`; WebFetch doc AlloyDB filtered (allowlist) | Ler `labels/` + README §Filtered; ler doc AlloyDB | Como o filtro integra ao ANN (pré/pós/in-filter) nos dois + gap. **R1/R2/R3** idem |
| Q8 | **(SOTA)** Como o AlloyDB implementa o **AI engine** — geração de embeddings em SQL + funções `ai.*` + hybrid search/reranking (RRF)? Qual a peça OSS permissiva? | techniques | Allowlist: `cloud.google.com` (AlloyDB AI / `ai.*`); `.claude/knowledge-base/references/pgvector/README.md` (§Hybrid); `docs/features/{07,08,09,12}-*.md` (nossa spec) | WebFetch docs AlloyDB AI (allowlist); grep `tsvector`/`rrf` no pgvector README | Ler docs AlloyDB AI + spec interna; mapear o que tem análogo OSS vs gap | Mapa `ai.*` → peça OSS (RRF/tsvector/modelo) + **gap honesto** (sem BM25 permissivo — AGPL barra paradedb). **R1/R2**; perf `UNBENCHMARKED` |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1 | Covered |
| Dependencies | Q2 | Covered |
| Tools | Q3 | Covered |
| Techniques | Q4, Q5, Q6, Q7, Q8 | Covered (5 — frontier R4, ≤ 5 cap) |

**Coverage: 4/4 corners covered (100%)** · Total **8 questões** (dentro de 6–14; ≤ 5 por corner; técnicas = 5 ≥ 2).

> **TheoDB frontier rigor** (`rules/discover-phd-rigor.md`): cada questão de `techniques` é (R1) ancorada no SOTA AlloyDB/ScaNN com o gap declarado, (R2) lastreada por ≥ 2 fontes primárias (paper via allowlist + doc/repo em `references/`), e (R3) qualquer claim de performance carrega metodologia+números+fonte OU `UNBENCHMARKED`.

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | path `.claude/knowledge-base/references/{...}` declarado existe | BLOCKED "path not found", segue |
| Per-question Fase A budget | Fase A retornou ≥1 hotspot OU 3 retries | após 3 retries vazios → BLOCKED "Fase A exhausted"; segue |
| Web source (Q4/Q7/Q8) | URL ∈ `discover-web-allowlist.txt` E retorna conteúdo | fora da allowlist ou 404 → BLOCKED "source unreachable" (R6); registra como UNBENCHMARKED/open |
| After answering Qx | seção do blueprint sob Qx tem ≥1 citação | re-itera Qx (1 retry) |
| Técnica com claim de perf | claim tem metodologia+números+fonte OU marca `UNBENCHMARKED` | adiciona `UNBENCHMARKED` (R3) |
| Per-project time budget | budget do projeto não esgotado | esgotado → questões restantes BLOCKED "budget exhausted"; avança |
| Before promising complete | 4 corners com seções populadas | recusa promise, continua |

## Acceptance Criteria

- [ ] Todas as research questions respondidas OU BLOCKED com razão
- [ ] Os 4 corners com seções populadas no blueprint
- [ ] Toda citação aponta a path real em `.claude/knowledge-base/references/{...}`
- [ ] **Frontier rigor (R1/R2/R3):** cada técnica ancorada no SOTA + ≥ 2 fontes primárias; cada claim de perf benchmarked OU `UNBENCHMARKED`
- [ ] ≥ 1 seção ADR no blueprint sintetiza as decisões (incl. recomendação sobre o gatilho de fork D3)
- [ ] Time budget respeitado por projeto
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint salvo em `.claude/knowledge-base/discoveries/blueprints/alloydb-vector-ai-implementation-blueprint.md`

## Global Definition of Done

- [ ] Todas as fases completas (plan → edge-cases → plan-confidence → execute → confidence → improve se preciso)
- [ ] Verdict final de `/discover-confidence` registrado no header do blueprint
- [ ] Zero citações fabricadas
- [ ] Coverage Matrix 100%
- [ ] ADRs referenciam ≥ 1 regra do projeto: `rules/discover-phd-rigor.md` (R1–R6), `architecture.md` (DIP — extensão atrás de interface), `public-copy.md` (claim só com benchmark), `parsimony-ladder.md` (não reinventar — Regra 9) e PRD **D3** (gatilho de fork).
