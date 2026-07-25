---
generated_by: roadmap-init
generated_on: 2026-06-26
last_updated: 2026-07-24
slug: theodb
peer_count_cloned: 33
peer_count_registered_below: 21
peer_count_skipped: 0
landscape_catalogued: 6
papers_local: 25
---

# References catalog

State-of-the-art peer projects gathered at project inception by `/roadmap-init`.
This file is the contract `/discover-plan` reads when investigating a peer.

> **⚠️ Fonte primária local — leia ANTES de buscar na web.** Este acervo (código de peers +
> `papers/` com 25 PDFs) está no disco e é grep-ável. A ordem correta de investigação é
> **local → web**, nunca o contrário: `rules/discover-phd-rigor.md` R0 exige evidência web,
> R0.1 exige que o acervo local seja consultado primeiro (é mais barato, mais rápido e já
> foi curado). Ver `../../CLAUDE.md § Acervo de referências`.

> **Location note:** the skill template places this catalog at
> `knowledge-base/references/_catalog.md`, but this project's `boundary-check.sh` hook makes
> `knowledge-base/references/` strictly read-only (study material). The catalog therefore lives
> here, adjacent to the references folder, at `.claude/knowledge-base/references-catalog.md`.
>
> **Lifecycle:** every peer below has lifecycle `cloned` (folder present under
> `.claude/knowledge-base/references/`). No peer was skipped — the 3 AGPL-3.0 candidates were cloned
> `clone-anyway-study-only` by explicit owner decision (study only; copying their code into the
> distribution is forbidden by D1).
>
> **Honest note on license detection:** `pgvector` and `pgbackrest` show `NOASSERTION` on the GitHub
> API (the detector does not recognize their LICENSE file), but they are PostgreSQL License and MIT
> respectively — both permissive (PRD §11).

---

## pgvector

- **Folder:** `.claude/knowledge-base/references/pgvector/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/pgvector/pgvector
- **License:** `PostgreSQL License` (GitHub API: `NOASSERTION`)
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★21.9k at clone time)

### Why this peer is here

O tipo vetorial + operadores de similaridade (`<=>`) e os índices HNSW/IVFFlat do TheoDB são o
baseline e o alvo de customização do pilar killer (P2). É a peça central de D3 e o ponto onde a
Política de Fork pode disparar (sob benchmark).

### What to study in it

- Implementação do tipo `vector` e dos operadores de distância.
- Estrutura do índice HNSW (baseline a superar com pgvectorscale).
- Integração com o planner do Postgres (custo/seletividade).

### Supports ROADMAP milestone(s)

- M0 — *because:* o walking skeleton precisa de `CREATE EXTENSION vector` + query `<=>` end-to-end.
- M2 — *because:* base do pilar vetorial e alvo da Política de Fork.

---

## pgvectorscale

- **Folder:** `.claude/knowledge-base/references/pgvectorscale/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/timescale/pgvectorscale
- **License:** `PostgreSQL License`
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★3.0k at clone time)

### Why this peer is here

Traz o índice **StreamingDiskANN** + Statistical Binary Quantization — o "índice ANN avançado além do
HNSW" que diferencia o TheoDB e espelha o ScaNN do AlloyDB com peça permissiva (D3).

### What to study in it

- Algoritmo StreamingDiskANN e trade-off recall/latência/escala.
- Statistical Binary Quantization (compressão de vetores).
- Como compõe com pgvector sem reimplementar o tipo vetorial.

### Supports ROADMAP milestone(s)

- M2 — *because:* índice ANN avançado exigido pelo critério de ship do MVP.

---

## supabase-postgres

- **Folder:** `.claude/knowledge-base/references/supabase-postgres/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/supabase/postgres
- **License:** `PostgreSQL License`
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★1.7k at clone time)

### Why this peer is here

A distro Postgres OSS mais próxima do modelo do TheoDB (mesma licença escolhida — Apache 2.0 no código
próprio) — referência direta de **como empacotar uma distribuição Postgres** com extensões
pré-instaladas, builds de imagem e tuning conjunto.

### What to study in it

- Build da imagem container (Dockerfile, base, extensões pré-instaladas).
- Como versionam extensões contra majors do Postgres.
- Estratégia de migração/compat entre versões da distro.

### Supports ROADMAP milestone(s)

- M0 — *because:* referência do empacotamento mínimo (container PG + extensão).
- M1 — *because:* referência do core + suíte de compatibilidade + tuning conjunto.

---

## duckdb

- **Folder:** `.claude/knowledge-base/references/duckdb/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/duckdb/duckdb
- **License:** `MIT`
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★39k at clone time)

### Why this peer is here

Engine columnar que `pg_mooncake` usa por baixo (DuckDB-powered, D2). Entender o motor é pré-requisito
para avaliar o pilar de analytics colunar/HTAP de forma honesta (é lakehouse, não in-memory como o
AlloyDB).

### What to study in it

- Execução vetorizada (vectorized query processing).
- Formato colunar e integração com Iceberg.
- Trade-offs de um engine analítico embutido vs. row-store transacional.

### Supports ROADMAP milestone(s)

- M6 — *because:* fundamenta o columnar DuckDB-powered (`pg_mooncake`).

---

## pg_mooncake

- **Folder:** `.claude/knowledge-base/references/pg_mooncake/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/Mooncake-Labs/pg_mooncake
- **License:** `MIT`
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★2.0k at clone time)

### Why this peer is here

Peça **primária** do pilar analytics colunar (D2) — columnar lakehouse permissivo (MIT) acelerado por
DuckDB, substituindo as opções AGPL (Citus/Hydra/ParadeDB) barradas por D1.

### What to study in it

- Como expõe armazenamento colunar como extensão do Postgres.
- Sincronização row↔colunar sobre dados transacionais vivos.
- Suporte a majors do Postgres (risco de M6).

### Supports ROADMAP milestone(s)

- M6 — *because:* peça primária do columnar/HTAP.

---

## cloudnative-pg

- **Folder:** `.claude/knowledge-base/references/cloudnative-pg/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/cloudnative-pg/cloudnative-pg
- **License:** `Apache-2.0`
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★8.9k at clone time)

### Why this peer is here

Operador Kubernetes de referência para Postgres (HA declarativa, failover, backup) — substrato do
P8/operador e a "porta aberta" para um eventual managed (D7). Mesma licença do TheoDB.

### What to study in it

- Padrão operator + CRDs + reconciliation loop para um cluster Postgres.
- Como modela primary/standby e failover declarativo.
- Integração de backup/PITR no operador.

### Supports ROADMAP milestone(s)

- M4 — *because:* referência de HA orquestrada.
- M5 — *because:* referência do operador Kubernetes de produção.

---

## patroni

- **Folder:** `.claude/knowledge-base/references/patroni/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/zalando/patroni
- **License:** `MIT`
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★8.5k at clone time)

### Why this peer is here

Ferramenta consagrada de failover automático / HA citada no PRD §6 como peça de composição. Referência
para primary+standby+failover do M4.

### What to study in it

- Eleição de líder e prevenção de split-brain.
- Integração com DCS (etcd/Consul) e healthchecks.
- Tempo-alvo de failover e como medi-lo.

### Supports ROADMAP milestone(s)

- M4 — *because:* referência do failover automático.

---

## pgbackrest

- **Folder:** `.claude/knowledge-base/references/pgbackrest/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/pgbackrest/pgbackrest
- **License:** `MIT` (GitHub API: `NOASSERTION`)
- **License-gate decision:** auto-approved-permissive
- **Last release / last commit:** active (★4.2k at clone time)

### Why this peer is here

Ferramenta consagrada de backup contínuo + PITR citada no PRD §6. Referência para backup/restore
validado do M4.

### What to study in it

- Backup full/incremental/diferencial e PITR.
- Restore validado e verificação de integridade.
- Retenção e armazenamento (local/S3).

### Supports ROADMAP milestone(s)

- M4 — *because:* referência de backup contínuo + PITR.

---

## paradedb *(study-only)*

- **Folder:** `.claude/knowledge-base/references/paradedb/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/paradedb/paradedb
- **License:** `AGPL-3.0`
- **License-gate decision:** clone-anyway-study-only — ⚠️ AGPL: copiar código para a distribuição é PROIBIDO por D1. Só estudo.
- **Last release / last commit:** active (★9.0k at clone time)

### Why this peer is here

`pg_search` (BM25 full-text) é a referência mais forte de busca textual/híbrida no ecossistema
Postgres. O TheoDB precisa de hybrid search (M7), mas `pg_search` é **AGPL → barrado no pacote**.
Clonado só para estudar a abordagem e buscar uma alternativa permissiva.

> **Honest discrepancy:** o PRD §11 listava `pg_analytics` como PostgreSQL License; o repo
> `paradedb/paradedb` está hoje **AGPL-3.0**. A premissa permissiva de `pg_analytics` precisa ser
> reverificada no `cycle-discover` de M6/M7.

### What to study in it

- Arquitetura do índice BM25 sobre Postgres (`pg_search`) — só o **design**.
- Como integram full-text + vetorial (hybrid).
- O que seria preciso para uma alternativa permissiva equivalente.

### Supports ROADMAP milestone(s)

- M7 — *because:* referência de hybrid search / BM25 (alternativa permissiva a achar).

---

## citus *(study-only)*

- **Folder:** `.claude/knowledge-base/references/citus/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/citusdata/citus
- **License:** `AGPL-3.0`
- **License-gate decision:** clone-anyway-study-only — ⚠️ AGPL: barrado no pacote por D1 (PRD §11). Só estudo.
- **Last release / last commit:** active (★12.6k at clone time)

### Why this peer is here

Referência canônica de **colunar + escala-out como extensão do Postgres** (modelo de composição que o
TheoDB adota). SIGMOD 2021. Barrado no pacote (AGPL), mas o **design** informa M6/M8.

### What to study in it

- Columnar storage como extensão (design, não código).
- Sharding e distribuição de queries.
- Como expõem escala-out preservando wire-compat.

### Supports ROADMAP milestone(s)

- M6 — *because:* design de columnar-as-extension.
- M8 — *because:* design de escala/distribuição de leitura.

---

## hydra *(study-only)*

- **Folder:** `.claude/knowledge-base/references/hydra/`
- **Lifecycle:** cloned
- **Repo:** https://github.com/hydradatabase/hydra
- **License:** `AGPL-3.0` (columnar engine; repo top-level mostra Apache-2.0 — ambíguo)
- **License-gate decision:** clone-anyway-study-only — ⚠️ engine columnar é AGPL (PRD §11 baniu). Só estudo.
- **Last release / last commit:** active (★3.0k at clone time)

### Why this peer is here

Columnar engine (fork de Citus columnar) empacotado como distro Postgres analítica. Referência de
**design** do pilar columnar. PRD §11 marcou o "Hydra columnar engine" como AGPL → barrado; o repo é
ambíguo (Apache no topo, engine AGPL) — tratado conservadoramente como study-only.

### What to study in it

- Design do columnar engine e integração com o Postgres.
- Empacotamento de uma distro Postgres analítica.
- O que difere de `pg_mooncake` (nossa peça permissiva primária).

### Supports ROADMAP milestone(s)

- M6 — *because:* design de columnar engine (comparar com `pg_mooncake`).

---

## pinecone-python-client

- **Folder:** `.claude/knowledge-base/references/pinecone-python-client/`
- **Lifecycle:** cloned (2026-06-28, `--depth 1`)
- **Repo:** https://github.com/pinecone-io/pinecone-python-client
- **License:** `Apache-2.0` (client SDK; the Pinecone **engine** is proprietary/SaaS — not here)
- **License-gate decision:** auto-approved-permissive (SDK only — engine is closed, nothing to import)
- **Last release / last commit:** active

### Why this peer is here

Pinecone é o **alvo competitivo de mercado** (vector DB gerenciado, fechado). O engine não é OSS — só
os SDKs cliente (Apache). Referência de **ergonomia de API / DX** para a superfície de produto/BaaS do
TheoDB (a north-star de migração saindo de pago).

### What to study in it

- Forma da API de gestão (create index/collection, upsert, query, namespaces).
- Modelagem de erros + tipos do cliente (DX).
- Convenções de paginação/batch que esperamos espelhar.

### Supports ROADMAP milestone(s)

- M8 — *because:* referência de DX/API para o caminho gerenciado (e a discovery `baas-control-plane`).

---

## vectorchord *(study-only)*

- **Folder:** `.claude/knowledge-base/references/vectorchord/`
- **Lifecycle:** cloned (2026-06-28, `--depth 1`)
- **Repo:** https://github.com/tensorchord/VectorChord
- **License:** `AGPL-3.0 OR Elastic License v2` (dual)
- **License-gate decision:** clone-anyway-study-only — ⚠️ **AGPL/ELv2: barrado no pacote por D1** (mesmo veredito do `VectorChord-bm25` no M7). Copiar/derivar código é PROIBIDO. Só estudo clean-room.
- **Last release / last commit:** v1.1.1 (2026-02-28)

### Why this peer is here

Sucessor do pgvecto.rs — índice `vchordrq` com **RaBitQ** (quantização 1/4/8-bit + reranking). É a rota
de quantização concorrente ao SBQ/StreamingDiskANN (pgvectorscale, que shipamos). Estudo do **algoritmo**
para o landscape de quantização (M14/M2) — NÃO importável (D1).

### What to study in it

- Design do RaBitQ (compressão binária + reranking) — só o **design**, clean-room.
- Como compõem com tipos do pgvector (compat declarada).
- Comparação de memória-a-recall vs SBQ (DiskANN) — gatilho #2 do ADR 0004.

### Supports ROADMAP milestone(s)

- M2 / M14 — *because:* rota de quantização alternativa no landscape do pilar vetorial (não-adotável).

---

## Landscape catalogados (não-clonados — estudo via web/docs)

Standalone vector DBs e managed-Postgres relevantes ao landscape competitivo. **Não clonados** (disco +
baixo retorno: são bancos standalone, não extensões Postgres — adotá-los romperia o gate "100%
wire-compatible Postgres", logo são referência **competitiva/de técnica**, não código a importar).
Licenças confirmadas por busca 2026 (fontes no histórico da sessão).

| Peer | Licença | Tipo | Veredito p/ TheoDB |
|---|---|---|---|
| Qdrant | Apache-2.0 | Standalone DB (Rust) | Permissivo, mas standalone → não-importável (quebra wire-compat). Estudo de **filtered ANN** / payload filtering. |
| Milvus | Apache-2.0 | Standalone DB (Go/C++) | Permissivo; billion-scale + GPU. Não-importável. Estudo de **escala/sharding** (informa M8). |
| Weaviate | BSD-3 / Apache-2.0 | Standalone DB (Go) | Permissivo; vectorization embutida. Não-importável. Estudo de **RAG/hybrid UX**. |
| Chroma | Apache-2.0 | Embedded/standalone (Py/JS) | Permissivo; foco em DX. Não-importável. Estudo de **DX de embedding/dev-loop**. |
| Neon | Apache-2.0 | Serverless Postgres (storage desagregado) | Permissivo, mas storage desagregado é complexo de self-host (Databricks comprou 2025). Estudo de **arquitetura control-plane / branching / scale-to-zero**. |
| Supabase (stack) | Apache-2.0 | BaaS Postgres OSS completo | Permissivo, self-host Docker. Já temos `supabase-postgres` (só a imagem). O stack (`supabase/supabase`: studio/gotrue/postgrest/storage/realtime) é a **referência primária de BaaS OSS** — clonar quando a discovery `baas-control-plane` exigir + houver disco. |
| pgvecto.rs (tensorchord) | Apache-2.0 | Extensão PG (Rust, vector search) | **DESCONTINUADO** — o README diz "migrate to VectorChord"; última release v0.4.0 (nov/2024). Permissivo (código legalmente reutilizável), MAS projeto morto → **não adotar** (Regra 9: abandonado = alerta vermelho). `pgvectorscale` (vivo, mantido, PostgreSQL License, Rust) já cobre DiskANN+SBQ. |

> **Padrão de licença TensorChord (alerta):** pgvecto.rs (Apache, descontinuado) → **VectorChord** (AGPL/Elastic,
> sucessor "mais rápido"). Apache → copyleft no sucessor = **rug-pull de licença**. Razão para NÃO depender de
> nada da TensorChord; reforça `pgvector` + `pgvectorscale` (permissivos **e vivos e mantidos**) como a escolha
> certa do pilar vetorial. Confirma o ADR 0005 (unificação; performance competitiva — não reescrever em Rust).
>
> Estes alimentam a discovery `baas-control-plane` (Neon/Supabase) e o landscape do pilar vetorial
> (Qdrant/Milvus/Weaviate/Chroma). Promover qualquer um a `cloned` exige espaço em disco (hoje 99%).

---

## Skipped peers (license gate)

> None. Nenhum candidato foi rejeitado no license gate — os 3 AGPL-3.0 (paradedb, citus, hydra) foram
> clonados `clone-anyway-study-only` por decisão explícita do owner (2026-06-26). O risco legal está
> reconhecido: **estudo apenas; copiar código AGPL para a distribuição Apache 2.0 é proibido por D1.**

| Peer | Repo | License | Reason for skip |
|---|---|---|---|
| — | — | — | (nenhum) |

---

## Lote 2026-07-24 — trilha técnica (8 repos)

Clonados `--depth 1` para fechar as lacunas apontadas na análise da trilha de leitura: segurança de
memória Rust/FFI, metodologia de profiling, spec colunar, e as âncoras do SOTA que faltavam.

| Peer | Folder | Licença | Gate | Por que está aqui | Serve |
|---|---|---|---|---|---|
| **pgrx** | `references/pgrx/` | MIT | auto-approved-permissive | O framework que o `theodb_rs` inteiro usa. **384 ocorrências de `unsafe`** no nosso código atravessam esta fronteira. Fonte-de-verdade para memory contexts, `pg_guard`, SPI, `#[pg_extern]`, panics-através-de-C. | transversal (todo `theodb_rs`); reviews do `council-rust-pgrx` |
| **tantivy** | `references/tantivy/` | MIT | auto-approved-permissive | O motor lexical que shipamos (M139/M140). Arquitetura de segmentos, `Directory`, merge policy — o que o IIR (teoria BM25) não cobre. | pilar lexical; ADR-0051..0055 |
| **FlameGraph** | `references/FlameGraph/` | CDDL-1.0 | ⚠️ **study/tooling-only** — CDDL não está na lista permitida (D1: Apache/MIT/BSD/PostgreSQL). Usar como ferramenta externa; **nunca** vendorizar no pacote. | Ferramental de profiling do Brendan Gregg. | **M148** (spike ativo), M149–M151 |
| **parquet-format** | `references/parquet-format/` | Apache-2.0 | auto-approved-permissive | Spec do formato, não a lib. Organização física, row groups, encodings, estatísticas de página (a base do nosso zone-map skip-pruning). | M148–M151; lakehouse |
| **graphrag** | `references/graphrag/` | MIT | auto-approved-permissive | Implementação completa de referência de GraphRAG (extração de grafo → recuperação local/global). Estudo comparativo, **não** arquitetura obrigatória. | pilar grafo (M108–M113) |
| **beir** | `references/beir/` | Apache-2.0 | auto-approved-permissive | Harness de avaliação de recuperação — usamos os datasets nos M123/M138/M140.1. Ter o código local evita reimplementar a métrica. | benchmark de recuperação |
| **faiss** | `references/faiss/` | MIT | auto-approved-permissive | Referência de implementação de IVF/PQ em escala. Permissivo → legalmente reutilizável (≠ VectorChord). | pilar vetorial; quantização |
| **hnswlib** | `references/hnswlib/` | Apache-2.0 | auto-approved-permissive | A implementação de referência do próprio paper do HNSW (Malkov). Baseline de leitura do nosso `ann/hnsw.rs`. | pilar vetorial |

---

## Papers e livros de acesso livre — `references/papers/`

**25 PDFs, 74 MB, gitignored.** Só material distribuído livremente (arXiv, cópias de autor, venues
abertos). Nenhum livro pago foi baixado — ver § *Livros pagos* abaixo.

| Arquivo | Trabalho | Aterrissa em |
|---|---|---|
| `hnsw-malkov-2018.pdf` | HNSW (TPAMI 2018) | `ann/hnsw.rs`, `am/hnsw_page.rs` |
| `scann-anisotropic-vq-guo-2020.pdf` | Quantização anisotrópica / ScaNN (ICML 2020) | **âncora do North Star** (CLAUDE.md regra 1); ADR-0035 |
| `rabitq-gao-2024.pdf` | RaBitQ | ADR-0036 (lever condicional, veredito medido) |
| `symphonyqg-gou-2024.pdf` | SymphonyQG | spike E2 (gate não atingido) |
| `diskann-subramanya-2019.pdf` | DiskANN (NeurIPS 2019) | pgvectorscale; regime out-of-RAM |
| `faiss-gpu-johnson-2017.pdf` · `faiss-library-douze-2024.pdf` | Faiss | IVF/PQ |
| `iir-manning-2008-BOOK.pdf` | *Introduction to Information Retrieval* (livro completo) | BM25 own-code, avaliação |
| `rrf-cormack-2009.pdf` | Reciprocal Rank Fusion | `ai.hybrid_search_rrf` |
| `beir-thakur-2021.pdf` | BEIR | M123/M138/M140.1 |
| `sentence-bert-reimers-2019.pdf` · `dpr-karpukhin-2020.pdf` · `colbert-khattab-2020.pdf` | Embeddings densos / late interaction | superfície AI-native |
| `columnar-survey-abadi-2013.pdf` | *(não baixado — 403)* | ver § *Não obtidos* |
| `cstore-stonebraker-2005.pdf` | C-Store | origem do colunar |
| `monetdb-x100-boncz-2005.pdf` | MonetDB/X100 | execução vetorizada |
| `morsel-parallelism-leis-2014.pdf` | Morsel-driven parallelism | paralelismo do scan colunar |
| `database-cracking-idreos-2007.pdf` | Database Cracking | indexação adaptativa |
| `aries-mohan-1992.pdf` | ARIES | WAL/recovery — base do `GenericXLog` |
| `mvcc-serializable-ports-2012.pdf` | SSI no PostgreSQL | MVCC do TableAM colunar |
| `rigorous-perf-eval-georges-2007.pdf` | Rigor estatístico em medição | **disciplina de benchmark** (lição M123/M130/M131) |
| `tail-at-scale-dean-2013.pdf` | The Tail at Scale | p95/p99 |
| `graphrag-edge-2024.pdf` · `graphrag-survey-han-2025.pdf` | GraphRAG + survey | pilar grafo |
| `indirect-prompt-injection-greshake-2023.pdf` | Prompt injection indireto | `nl.rs`, NL→SQL seguro |
| `bird-nl2sql-li-2023.pdf` | BIRD | benchmark NL→SQL |

### Não obtidos (403/paywall — ler online, não versionar)

- **Abadi, Boncz, Harizopoulos et al., *The Design and Implementation of Modern Column-Oriented
  Database Systems*** (FnT in Databases, 2013) — host bloqueia download automatizado (403).
  Ler em `https://stratos.seas.harvard.edu/publications/design-and-implementation-modern-column-oriented-database-systems`.
  **É a leitura nº 1 do M149/M150** (projection pushdown, late materialization).
- **Kemper & Neumann, *HyPer*** (ICDE 2011) — URLs testadas retornaram 404.

### Livros pagos — comprar, não baixar

Não estão no acervo e **não devem** ser obtidos por download. Registro só para a trilha:

| Livro | Uso |
|---|---|
| Kleppmann & Riccomini, *Designing Data-Intensive Applications*, 2ª ed. | vocabulário comum |
| Petrov, *Database Internals* | storage engines, B-trees, WAL |
| Campbell & Majors, *Database Reliability Engineering* | operação — **fora do escopo deste repo** (HA/plataforma) |

Gratuitos online, não espelhados aqui (ler no site): *The Internals of PostgreSQL* (Suzuki,
`interdb.jp`) e os livros de SRE do Google (`sre.google/books`).

---

## Clones não registrados (dívida honesta)

O catálogo listava 13 peers enquanto o disco tinha 25 pastas. Os 12 abaixo foram clonados em
sessões posteriores sem entrada no catálogo — ficam registrados aqui como inventário mínimo até
alguém escrever a entrada completa (`Why / What to study / Milestones`):

`arrow-rs` · `cstore_fdw` · `datafusion` · `lance` · `lotus` · `mcp-go-sdk` · `palimpzest` ·
`postgres` · `postgresml` · `RaBitQ-Library` · `rabitq-rs` · `SymphonyQG`

> `datafusion` + `arrow-rs` são hoje o **motor de execução do colunar in-DB** (não só lakehouse) —
> desde o M143/ADR-0057, com o `pg_duckdb` removido. Merecem entrada completa na próxima passada.

---

## Cleanup protocol

- **Remove a peer:** delete its folder under `.claude/knowledge-base/references/` AND remove its entry from this catalog in the same commit.
- **Update a peer (refresh clone):** `cd .claude/knowledge-base/references/{peer}/ && git pull` — record the new commit SHA in this catalog.
- **Replace a peer with a better one:** treat as remove + add. Do NOT rename folders; symbolic continuity is meaningless when the underlying repo changed.
