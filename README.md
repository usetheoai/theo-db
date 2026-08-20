# TheoDB

**PostgreSQL com superpoderes de IA e analytics — open-source, rodando em qualquer lugar.**

TheoDB é um banco de dados open-source e 100% compatível com PostgreSQL, empacotado como uma
edição única para download que roda no seu laptop, on-premises, na borda, em qualquer nuvem,
em Kubernetes ou bare metal. Num só pacote você tem busca vetorial + híbrida (BM25+vetor) para
aplicações de IA, grafo para GraphRAG, analytics colunar sobre dados transacionais vivos e
lakehouse Parquet own-code (sem DuckDB) — tudo em SQL, com código próprio, sem licença por vCPU
e sem lock-in.

> ⚠️ **Status:** em desenvolvimento ativo, ainda **pré-1.0** (releases 0.x — ver [`CHANGELOG.md`](./CHANGELOG.md)).
> Sem afirmação de "production-ready" até haver evidência de uso sustentado (`public-copy.md`).
> O documento de produto está em [`PRD.md`](./PRD.md).

---

## Missão

A missão do TheoDB é **igualar ou superar o AlloyDB** para quem roda OSS, on-premises e na borda —
entregando as mesmas capacidades (busca vetorial/IA, analytics, operação) com **superioridade estrutural**
em abertura, custo, portabilidade e **independência de modelo** (qualquer modelo local ou remoto, sem
lock-in). Metas de performance são **metas comprovadas por benchmark reproduzível** em `wiki/benchmarks/` —
nunca afirmações sem evidência. Estratégia completa: [`wiki/decisions/0002-north-star-equal-or-superior-to-alloydb.md`](./wiki/decisions/0002-north-star-equal-or-superior-to-alloydb.md).

> **Estado medido FINAL do pilar vetorial (honesto — M73/M74, `wiki/decisions/0035`, `wiki/decisions/0036`).** Depois de
> medir head-to-head reproduzível vs **ScaNN OSS** (o algoritmo do AlloyDB) e vs **pgvector** no SIFT1M:
> - **Paridade de recall own-code classe-pgvector: ALCANÇADA** (M60/M69/M70) — o tipo `vector` e os índices ANN
>   são 100% próprios, sem pgvector/pgvectorscale.
> - **Throughput multi-cliente competitivo-a-superior** vs pgvector no regime 128d clusterizado (M72, +11% QPS
>   a recall casado) — [`wiki/benchmarks/`](./wiki/benchmarks/).
> - **Superioridade de QPS vetorial sobre o ScaNN/AlloyDB: MEDIDA como NÃO-ALCANÇÁVEL** por uma extensão
>   PostgreSQL permissiva ([`wiki/benchmarks/m73-headtohead-verdict.md`](./wiki/benchmarks/m73-headtohead-verdict.md)) —
>   o gap (~25–44× @ 0,99) é de **paradigma** (AH-LUT anisotrópico + não pagar o imposto MVCC/WAL), não de
>   engenharia. Posicionamento honesto: **paridade de recall + memória billion-scale + AI-native/HTAP/aberto** —
>   **nunca** "mais rápido que o AlloyDB no vetor".
>
> Onde o TheoDB pode ser genuinamente superior é a **superfície AI-native híbrida** (as duas pernas próprias,
> vetor + lexical, no mesmo banco transacional) — o reposicionamento do North Star está em
> [`wiki/decisions/0033`](./wiki/decisions/) (proposto, decisão do owner; o mandato LOCKED do ADR-0002 permanece até assinatura).

---

## Por que TheoDB

- **Open-source de verdade.** Sem licença por vCPU, sem open-core escondendo os recursos centrais. Auditável, customizável, contribuível.
- **Bateria inclusa, código próprio.** PostgreSQL 18 + a extensão `theodb_rs` (tipo `vector` own-code, índices ANN, busca híbrida, grafo, colunar) pré-instalada e tunada — você não monta as peças nem aluga o vetor.
- **Roda em qualquer lugar.** A mesma imagem vai do laptop ao bare metal regulado.
- **100% compatível com PostgreSQL no protocolo.** Seus drivers e aplicações falam com o TheoDB como falam com um PostgreSQL 18 — é o que ele é. Uma ressalva honesta sobre **poolers**: o ajuste de busca é feito por GUC de sessão (`SET theodb_hnsw.ef_search = …`), então sob *transaction pooling* ele precisa de `SET LOCAL` dentro da transação, como qualquer GUC de sessão do PostgreSQL. Compatibilidade com PgBouncer nos três modos ainda **não foi medida** ([B-055](BACKLOG.md)) — o que está escrito aqui é análise do código, não resultado de execução.
- **IA no banco onde seus dados já estão.** Embeddings, busca vetorial + híbrida (BM25+vetor+RRF), rerank, NL→SQL e GraphRAG via SQL — sem ETL para um sistema separado.

---

## Para quem é

- Quem constrói aplicações gen-AI (RAG/agentes) e quer embeddings + vector search no mesmo banco dos dados operacionais.
- Times de plataforma e DBAs que precisam de um pacote PostgreSQL com IA, analytics e operação — sem custo de licença.
- Cenários on-prem, edge e regulados que precisam rodar fora da nuvem, com código auditável.
- Quem busca uma alternativa aberta para fugir de lock-in e de custo de licença por vCPU.

---

## Como funciona

TheoDB **não é um fork** do PostgreSQL. É o **PostgreSQL 18 upstream** + uma extensão Rust própria
(`theodb_rs`) que traz os pilares de IA, vetorial, grafo e colunar **como código próprio** (não peças
alugadas), empacotados e tunados numa imagem única:

```
TheoDB = PostgreSQL 18 (upstream, sem fork)
       + theodb_rs (extensão Rust própria):
           · tipo `vector` own-code + AMs ANN próprios (theodb_hnsw / theodb_ivfflat)
           · superfície AI-native SQL: embed, hybrid_search_rrf (BM25+vetor+RRF), rerank, NL→SQL, grafo
           · TableAM colunar próprio (theodb_columnar) para analytics sobre dados transacionais vivos
           · engine de grafo nativo (persisted-CSR) para GraphRAG
           · lakehouse Parquet own-code (ler/escrever/agregar arquivos externos via DataFusion, sem DuckDB)
         empacotado como uma imagem única que roda em qualquer lugar
```

A diferenciação técnica é ter as **duas pernas próprias** (vetorial + lexical + grafo) **dentro de um banco
transacional**, numa superfície SQL AI-native — não uma biblioteca in-memory nem um bolt-on de busca externa.
Toda afirmação de performance é medida em [`wiki/benchmarks/`](./wiki/benchmarks/), nunca sem evidência.
Detalhes de arquitetura, pilares e decisões travadas (D1–D7) estão no [`PRD.md`](./PRD.md) e nos [`wiki/decisions/`](./wiki/decisions/).

---

## Instalação

TheoDB roda como uma imagem container com **uma extensão instalável** que provisiona toda a superfície
de IA + vetorial:

```bash
docker pull ghcr.io/usetheoai/theo-db:latest
docker run -d --name theodb -e POSTGRES_PASSWORD=postgres -p 5432:5432 ghcr.io/usetheoai/theo-db:latest
```

A imagem cria a extensão automaticamente no primeiro init. Roda em **PostgreSQL 18** (a distribuição migrou
do 17 para o 18 no M135; o tipo `vector` e os índices ANN são **own-code**, sem depender de pgvector/pgvectorscale):

```sql
CREATE EXTENSION theodb_rs CASCADE;   -- CASCADE puxa theodb_rs (o tipo `vector` own-code + a superfície ai.*)
ALTER EXTENSION theodb_rs UPDATE;  -- upgrade in-place da extensão (cadeia de upgrade própria, M137)
```

Passo a passo das 12 capacidades em [`wiki/guides/quickstart.md`](./wiki/guides/quickstart.md).

> **Sem `plpython3u` (desde M19):** toda a superfície de IA (`ai.*`, NL→SQL, generativas, embed) é servida
> pela extensão Rust **`theodb_rs`** — o `theodb` não requer mais a linguagem *untrusted* `plpython3u`. A
> antiga limitação em PostgreSQL gerenciado (que não habilita `plpython3u`) **deixou de existir**: as 12
> capacidades ficam disponíveis sem depender de uma linguagem untrusted.

---

## Status & roadmap

> Estado real, não promessa. A lista completa e viva de milestones está em [`ROADMAP.md`](./ROADMAP.md)
> (**75 de 76 entregues** até a v0.131.0 — só falta o M141, o dogfood de produção). Nada aqui é data ou
> promessa; performance é sempre medida em [`wiki/benchmarks/`](./wiki/benchmarks/). Continua **pré-1.0**
> (ver o disclaimer de status acima).
>
> **Nível de maturidade (honesto):** **Estágio 1 (Experimental) entrando no 2 (Preview)** — invariantes
> documentados, regressão automatizada, 100+ benchmarks reproduzíveis, cadeia de upgrade e crash-safety
> provadas no binário shipado. **Não é production-ready**: falta a única coisa que benchmark não dá — uso
> sustentado em produção real (o M141, ≥30 dias + ≥2 operadores). Pela `dogfood-golden-rule`, esse dogfood é
> o gate para sequer começar a alegar production-ready.

**O que já existe e foi medido:**

- **Pilar vetorial** — tipo `vector` e índices ANN (`theodb_hnsw`, `theodb_ivfflat`) **own-code**, com paridade de recall classe-pgvector (ver o estado medido na Missão). Sem pgvector/pgvectorscale.
- **Superfície AI-native (SQL)** — embeddings (`theodb.embed`), busca **híbrida** `ai.hybrid_search_rrf` (BM25/`ts_rank_cd` + vetor via RRF), rerank, NL→SQL com defesa a injeção, extração de grafo. Servida 100% pela extensão Rust — **sem `plpython3u`**.
- **Colunar (in-DB)** — TableAM colunar próprio (`theodb_columnar`) com pushdown de agregação/GROUP-BY/zone-map sobre dados transacionais vivos (own-code).
- **Lakehouse Parquet (own-code)** — ler/escrever/agregar arquivos Parquet externos **own-code** via DataFusion/Arrow, **sem DuckDB** (M143 removeu o `pg_duckdb` por completo — ADR-0057). Superfície: `theodb.htap_refresh(rel)` (materializa uma tabela num snapshot Parquet) e `theodb.olap(rel)` (lê+agrega o snapshot); as primitivas `public.read_parquet(path)`/`write_parquet(rel,path)` são superuser-only (least-privilege — escrita de arquivo server-side). Uma imagem só, sem componente C++/httpfs; o lakehouse own-code custa +12 MB no build default vs os 118 MB do bundle DuckDB (`wiki/benchmarks/m143-pgduckdb-removal.md`).
- **Grafo nativo** — engine de grafo persisted-CSR para GraphRAG (`theodb.graph_*`).
- **Fundação de banco** — **PostgreSQL 18**, cadeia de upgrade própria (`ALTER EXTENSION ... UPDATE`), gates mecânicos de qualidade no CI (clippy `-D warnings`, rustfmt, Postgres `--enable-cassert`, license-gate D1, pgspot).

**Concluído recentemente (medido):**

- **Engine lexical própria (M140)** — BM25 own-code sobre Tantivy (MIT), in-PG e transacional (MVCC/WAL/crash provados no binário shipado); ganho honesto na busca lexical standalone + moat de consolidação, **não** no retrieval híbrido dominado pelo vetor (M138).
- **Remoção total do `pg_duckdb` (M142→M143)** — o lakehouse (ler/escrever/agregar Parquet) virou **own-code** (DataFusion/Arrow, sem DuckDB) no build default; o **último componente C++/httpfs saiu** do projeto. 118 MB de C++ fora, +12 MB de Rust dentro.

**O único que falta (não é engenharia):**

- **M141 — dogfood `running`** — mover uma capability theo-data (theo-rag/theo-memory/theo-lens) para produção sobre TheoDB self-hosted por ≥30 dias, com ≥2 operadores e evidência. É o gate real para reivindicar production-ready (nenhum benchmark substitui uso real) — corre no calendário, não em código.

Nota de escopo: HA/replicação/control-plane e deploy K8s **não fazem parte deste repositório** — o foco é o
banco de dados (o engine + a extensão). Referências científicas de cada pilar abaixo.

---

## Documentação

- [`PRD.md`](./PRD.md) — documento de produto completo (visão, pilares, requisitos, riscos, MVP).
- [`CHANGELOG.md`](./CHANGELOG.md) — registro de mudanças ([Keep a Changelog](https://keepachangelog.com/) + SemVer).
- [`wiki/guides/quickstart.md`](./wiki/guides/quickstart.md) — passo a passo das capacidades; [`wiki/benchmarks/`](./wiki/benchmarks/) — evidência medida.

---

## Referências

Base científica e de estado da arte que fundamenta as técnicas de cada pilar do TheoDB.
Todas as citações foram verificadas (link estável: arXiv / DOI / venue oficial).

### Concorrente de referência (pesquisa aplicada)

- [ScaNN for AlloyDB — whitepaper (Google)](https://services.google.com/fh/files/misc/scann_for_alloydb_whitepaper.pdf) — algoritmo de busca vetorial por vizinho aproximado (ANN) usado pelo AlloyDB; insumo direto para o pilar de IA/vetorial (P2).

### P2 — Vetorial / ANN (índices de vizinho aproximado)

- [Accelerating Large-Scale Inference with Anisotropic Vector Quantization](https://arxiv.org/abs/1908.10396) — Guo, Sun, Lindgren, Geng, Simcha, Chern, Kumar · ICML 2020 — algoritmo **ScaNN**; quantização anisotrópica para MIPS, base do índice vetorial-alvo.
- [Efficient and robust approximate nearest neighbor search using Hierarchical Navigable Small World graphs](https://arxiv.org/abs/1603.09320) — Malkov, Yashunin · IEEE TPAMI 2018 — **HNSW**, índice grafo padrão do pgvector (baseline a superar).
- [DiskANN: Fast Accurate Billion-point Nearest Neighbor Search on a Single Node](https://proceedings.neurips.cc/paper/2019/hash/09853c7fb1d3f8ee67a61b6bf4a7f8e6-Abstract.html) — Subramanya, Devvrit, Kadekodi, Simhadri, Krishnaswamy · NeurIPS 2019 — ANN em SSD para escala de bilhões de vetores num único nó.
- [Product Quantization for Nearest Neighbor Search](https://doi.org/10.1109/TPAMI.2010.57) — Jégou, Douze, Schmid · IEEE TPAMI 2011 — **PQ**, base de compressão para IVF/quantização escalar.
- [Billion-scale similarity search with GPUs](https://arxiv.org/abs/1702.08734) — Johnson, Douze, Jégou · 2017 — engine **Faiss**; referência de implementação de IVF/PQ em escala.
- [The Faiss library](https://arxiv.org/abs/2401.08281) — Douze et al. · 2024 — visão de engenharia atualizada da biblioteca Faiss.

### P2 — Embeddings, busca híbrida e reranking

- [Sentence-BERT: Sentence Embeddings using Siamese BERT-Networks](https://arxiv.org/abs/1908.10084) — Reimers, Gurevych · EMNLP 2019 — geração de embeddings densos para busca semântica.
- [Dense Passage Retrieval for Open-Domain Question Answering](https://aclanthology.org/2020.emnlp-main.550/) — Karpukhin et al. · EMNLP 2020 — recuperação densa dual-encoder, base de RAG.
- [ColBERT: Efficient and Effective Passage Search via Contextualized Late Interaction over BERT](https://people.eecs.berkeley.edu/~matei/papers/2020/sigir_colbert.pdf) — Khattab, Zaharia · SIGIR 2020 — late interaction multi-vetor (alta precisão de reranking).
- [Reciprocal Rank Fusion outperforms Condorcet and individual Rank Learning Methods](https://doi.org/10.1145/1571941.1572114) — Cormack, Clarke, Buettcher · SIGIR 2009 — **RRF**, fusão de rankings para busca híbrida (texto + semântica).
- [BEIR: A Heterogeneous Benchmark for Zero-shot Evaluation of Information Retrieval Models](https://arxiv.org/abs/2104.08663) — Thakur, Reimers, Rücklé, Srivastava, Gurevych · NeurIPS 2021 (D&B) — benchmark padrão para avaliar recall/qualidade de recuperação.

### P2 — Text-to-SQL / linguagem natural + segurança

- [Spider: A Large-Scale Human-Labeled Dataset for Complex and Cross-Domain Semantic Parsing and Text-to-SQL Task](https://aclanthology.org/D18-1425/) — Yu et al. · EMNLP 2018 — benchmark cross-domain de NL→SQL.
- [Can LLM Already Serve as A Database Interface? A Big Bench for Large-Scale Database Grounded Text-to-SQLs](https://arxiv.org/abs/2305.03111) — Li et al. · NeurIPS 2023 — **BIRD**, benchmark NL→SQL sobre bancos reais e grandes.
- [Not what you've signed up for: Compromising Real-World LLM-Integrated Applications with Indirect Prompt Injection](https://arxiv.org/abs/2302.12173) — Greshake, Abdelnabi, Mishra, Endres, Holz, Fritz · ACM AISec 2023 — fundamenta a defesa contra prompt injection (views parametrizadas seguras).

### P3 — Columnar / HTAP

- [C-Store: A Column-oriented DBMS](https://dblp.org/rec/conf/vldb/StonebrakerABCCFLLMOORTZ05.html) — Stonebraker, Abadi et al. · VLDB 2005 — arquitetura colunar seminal.
- [MonetDB/X100: Hyper-Pipelining Query Execution](https://dblp.uni-trier.de/rec/conf/cidr/BonczZN05.html) — Boncz, Zukowski, Nes · CIDR 2005 — execução vetorizada (vectorized query processing).
- [HyPer: A Hybrid OLTP&OLAP Main Memory Database System Based on Virtual Memory Snapshots](https://doi.org/10.1109/ICDE.2011.5767867) — Kemper, Neumann · ICDE 2011 — HTAP num único engine (analytics sobre dados transacionais vivos).
- [Citus: Distributed PostgreSQL for Data-Intensive Applications](https://doi.org/10.1145/3448016.3457551) — Cubukcu, Erdogan, Pathak, Sannakkayala, Slot · SIGMOD 2021 — escala-out e colunar como **extensão** do PostgreSQL (modelo de composição do TheoDB).

### P4 — Replicação, alta disponibilidade e recuperação

- [In Search of an Understandable Consensus Algorithm (Raft)](https://www.usenix.org/conference/atc14/technical-sessions/presentation/ongaro) — Ongaro, Ousterhout · USENIX ATC 2014 — consenso para failover/replicação confiável.
- [ARIES: A Transaction Recovery Method ... Using Write-Ahead Logging](https://doi.org/10.1145/128765.128770) — Mohan, Haderle, Lindsay, Pirahesh, Schwarz · ACM TODS 1992 — WAL + recovery, base de PITR.
- [Amazon Aurora: Design Considerations for High Throughput Cloud-Native Relational Databases](https://doi.org/10.1145/3035918.3056101) — Verbitski et al. · SIGMOD 2017 — storage desagregado (mesma família de design do AlloyDB).
- [Spanner: Google's Globally-Distributed Database](https://www.usenix.org/conference/osdi12/technical-sessions/presentation/corbett) — Corbett et al. · OSDI 2012 — replicação síncrona global e consistência externa.

### P7 — Auto-tuning (index advisor, learned indexes, MVCC)

- [The Case for Learned Index Structures](https://doi.org/10.1145/3183713.3196909) — Kraska, Beutel, Chi, Dean, Polyzotis · SIGMOD 2018 — índices aprendidos.
- [Automatic Database Management System Tuning Through Large-scale Machine Learning (OtterTune)](https://doi.org/10.1145/3035918.3064029) — Van Aken, Pavlo, Gordon, Zhang · SIGMOD 2017 — auto-tuning de configuração via ML.
- [AutoAdmin "What-if" Index Analysis / Cost-Driven Index Selection](https://www.microsoft.com/en-us/research/project/autoadmin/publications/) — Chaudhuri, Narasayya · VLDB 1997 / SIGMOD 1998 — seleção automática de índices (fundamenta o index advisor).
- [Database Cracking](https://www.cidrdb.org/cidr2007/papers/cidr07p07.pdf) — Idreos, Kersten, Manegold · CIDR 2007 — indexação adaptativa orientada ao workload.

> **Nota honesta:** estas referências são fundamentos *científicos* — não implicam que o TheoDB já as use. A escolha exata de cada técnica (ex.: qual índice ANN, qual peça columnar) será fechada no `cycle-discover`/`cycle-plan`, e a licença de cada dependência passa por due-diligence (PRD §11).

---

## Licença

[Apache License 2.0](./LICENSE) — a mesma do Supabase. Open-source permissiva, sem open-core.
Dependências AGPL são proibidas na distribuição. Ver [`PRD.md` §11 e §15](./PRD.md#11-modelo-open-source-e-licenciamento).
