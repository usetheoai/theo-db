# TheoDB

**PostgreSQL com superpoderes de IA e analytics — open-source, rodando em qualquer lugar.**

TheoDB é um banco de dados open-source e 100% compatível com PostgreSQL, empacotado como uma
edição única para download que roda no seu laptop, on-premises, na borda, em qualquer nuvem,
em Kubernetes ou bare metal. Num só pacote você tem busca vetorial para aplicações de IA,
analytics colunar sobre dados transacionais vivos e as ferramentas de operação que normalmente
exigem montagem manual — sem licença por vCPU e sem lock-in.

> ⚠️ **Status:** projeto em fase inicial de design (Draft v0.1). Ainda não há release.
> O documento de produto está em [`PRD.md`](./PRD.md).

---

## Por que TheoDB

- **Open-source de verdade.** Sem licença por vCPU, sem open-core escondendo os recursos centrais. Auditável, customizável, contribuível.
- **Bateria inclusa.** PostgreSQL + extensões maduras (incluindo um pgvector customizado) pré-instaladas e tunadas em conjunto — você não monta as peças.
- **Roda em qualquer lugar.** A mesma imagem vai do laptop ao bare metal regulado.
- **100% compatível com PostgreSQL.** Seus drivers, ferramentas e aplicações funcionam sem mudança.
- **IA no banco onde seus dados já estão.** Embeddings e busca vetorial via SQL, sem ETL para um sistema separado.

---

## Para quem é

- Quem constrói aplicações gen-AI (RAG/agentes) e quer embeddings + vector search no mesmo banco dos dados operacionais.
- Times de plataforma e DBAs que precisam de um pacote PostgreSQL com IA, analytics e operação — sem custo de licença.
- Cenários on-prem, edge e regulados que precisam rodar fora da nuvem, com código auditável.
- Quem busca uma alternativa aberta para fugir de lock-in e de custo de licença por vCPU.

---

## Como funciona

TheoDB **não é um fork** do PostgreSQL nem um engine novo. É uma distribuição que compõe o
PostgreSQL upstream com um conjunto curado de extensões e ferramentas, empacotados e tunados
em conjunto:

```
TheoDB = PostgreSQL (upstream) + pgvector customizado + camada columnar
         + HA/replicação + integração de IA/ML + tooling (operador, CLI, MCP, migração)
         empacotado como uma imagem única que roda em qualquer lugar
```

A diferenciação técnica está no **pgvector customizado** (índice ANN de alta performance
integrado ao planner) e no **empacotamento integrado** dos pilares de IA, analytics e operação.
Detalhes de arquitetura, pilares de capacidade e o recorte de MVP estão no [`PRD.md`](./PRD.md).

---

## Roadmap macro (inicial)

> Visão macro de alto nível. Os marcos serão refinados em planos detalhados antes de cada
> implementação. Nada aqui é uma data ou promessa de entrega.

- [ ] **M0 — Fundação & decisões.** Licença do projeto, due-diligence de licença das dependências, escolha dos majors do PostgreSQL suportados, ADRs de arquitetura ("sem fork").
- [ ] **M1 — Core + empacotamento.** Distribuição PostgreSQL-compatível em imagem container, extensões pré-instaladas, suíte de compatibilidade passando.
- [ ] **M2 — Vetorial / IA (pilar killer).** pgvector customizado com índice ANN avançado + geração de embeddings via SQL. *(MVP candidato)*
- [ ] **M3 — Migração mínima.** Import/export e caminho de entrada a partir do PostgreSQL vanilla.
- [ ] **M4 — Operação básica.** Alta disponibilidade (failover automático), backup contínuo + PITR.
- [ ] **M5 — Deploy em produção.** Operador Kubernetes; orquestrador para RPM/bare metal.
- [ ] **M6 — Analytics colunar / HTAP.** Camada de armazenamento colunar com escolha de plano row vs colunar.
- [ ] **M7 — IA avançada.** Filtered vector search, hybrid search + reranking, NL → SQL com views seguras.
- [ ] **M8 — Escala & observabilidade.** Read pools com load-balancing, index advisor, autovacuum adaptativo, métricas OTel/Prometheus.
- [ ] **M9 — Ecossistema & DX.** MCP server, integrações LangChain/LlamaIndex, UI de administração, migração a partir de AlloyDB.

O recorte exato do MVP (provavelmente **M0 → M2**) será fechado no próximo passo de planejamento.

---

## Documentação

- [`PRD.md`](./PRD.md) — documento de produto completo (visão, pilares, requisitos, riscos, MVP).
- `CHANGELOG.md` — registro de mudanças (a ser criado quando o desenvolvimento começar).

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

A definir (proposta em avaliação: Apache 2.0). Ver [`PRD.md` §11](./PRD.md#11-modelo-open-source-e-licenciamento).
