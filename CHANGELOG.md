# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/),
e este projeto adere ao [Semantic Versioning](https://semver.org/).

> Nota: o projeto está em fase inicial de design (pré-código, sem release). O tracker
> de issues/PRs ainda não está configurado, por isso as entradas abaixo ainda não
> referenciam números de ticket. A partir da configuração do tracker, toda entrada
> passará a citar o issue/PR correspondente.

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.1.0] - 2026-06-27

### Added

- **Harness de benchmark vetorial** `benchmarks/theodb_bench` — o **gate measurement-first do M2** (ADR 0002). Mede **recall@k** (semântica distance-thresholded do ANN-Benchmarks, não id-overlap), **latência p50/p95/p99**, **QPS** (best-of-N), **build-time** e **tamanho de índice** de um índice `pgvector` contra o container `theo-db:dev`, com **ground-truth brute-force exato** e **dataset seedado** (reprodutível); emite relatório JSON+markdown em `docs/benchmarks/`. Pirâmide de teste: 42 testes (unit puro + integração contra container real), coverage 98% (recall/metrics/dataset 100%), `ruff` limpo. Adapter `db.py` isola o driver (DIP); `psycopg2` é dev-only (não vai na imagem distribuída — D1 N/A). **Primeira evidência medida** (`docs/benchmarks/2026-06-27-pgvector-l2.json`, seed=42 n=5000 dim=128, números exatos por-run no JSON sha-stamped): HNSW recall@10 **~0.84** (ef_search=40) e **~0.96** (ef_search=100) — recall estável a menos da pequena variância de build paralelo do HNSW; QPS na faixa de ~1.7k–3.7k single-thread best-of-N (curva recall×QPS completa no JSON, varia com a carga da máquina). Converte o `UNBENCHMARKED` do pgvector em número reproduzível e destrava a decisão de índice + o gatilho de fork D3.

- M0 Walking Skeleton: `Dockerfile` — imagem `postgres:17-bookworm` com `pgvector v0.8.3` (Apache 2.0) compilado via `make OPTFLAGS=""` (binário portátil, sem `-march=native`); `HEALTHCHECK pg_isready` para sinalização automática de saúde do container.
- M0 Walking Skeleton: `smoke.sh` — smoke test automatizado que executa `pg_isready` (loop 10×1s), `CREATE EXTENSION IF NOT EXISTS vector` e `SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector` via `psql -v ON_ERROR_STOP=1`; imprime `SMOKE PASSED` e encerra com exit 0.
- M0 Walking Skeleton: `docs/adr/0001-no-engine-fork.md` — ADR que formaliza a decisão de não forkar o engine PostgreSQL, documenta 3 alternativas (extension model, engine fork, scratch) com rejeição explícita de A2/A3, e registra as consequências e riscos (ADR correspondente ao DoD-3 do M0).
- ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (LOCKED, mandato do CTO) — north-star do produto: TheoDB busca ser **igual ou superior ao AlloyDB** para usuários OSS/on-prem (Opção α). Codifica a doutrina measurement-first (harness de recall é o 1º item de M2), fork condicional ao gatilho D3, rota ScaNN-as-PG-AM (algoritmo Apache-2.0), superioridade estrutural OSS (abertura/custo/portabilidade/model-agnostic), e a divergência honesta em columnar (lakehouse, D2) / HA (Patroni) forçada pela licença (D1 barra AGPL). Resumo propagado e cross-linkado em `CLAUDE.md`, `README.md` (§ Missão), `PRD.md` (§1), `ROADMAP.md` (§ North Star) e `.claude/rules/README.md`.
- Discovery `alloydb-vector-ai-implementation` (cycle-discover, perfil PhD): plano + blueprint sobre **como o AlloyDB implementa o motor vetorial/IA** (ScaNN: tree-partition + anisotropic quantization + rescore; filtered search adaptativo; AI engine `ai.*`), reconstruído de fontes primárias allowlisted (paper ScaNN arXiv:1908.10396, docs AlloyDB, Google Research) + deep-read dos análogos OSS `pgvectorscale` (StreamingDiskANN+SBQ) e `pgvector` (HNSW/IVFFlat). Verdict `/discover-confidence` **SHIPPABLE 98.7** (4/4 corners, 20 citações verificadas, claims sem benchmark marcados `UNBENCHMARKED`/`BLOCKED`). Decisões: adotar pgvectorscale p/ M2; **não** forkar agora (gatilho D3 sem benchmark reproduzível); construir harness de recall@k como 1º item de M2; hybrid search é o win fácil de M7. Em `.claude/knowledge-base/discoveries/{plans,blueprints}/`.
- `docs/notebook/theodb.py` + `docs/notebook/vector_store.ipynb` — **SDK Python de marca do TheoDB** + notebook oficial de vector store. O módulo `theodb` é uma fachada fina (aliases reais, Regra 9 — compor não reinventar) sobre a stack OSS permissiva `langchain-postgres`, expondo a superfície estável de produto: `TheoDBEngine`, `TheoDBVectorStore`, `TheoDBColumn`, `TheoDBHNSWIndex`, `TheoDBIVFFlatIndex`, `TheoDBHybridSearchConfig`. O notebook foi portado de um tutorial proprietário para essa fachada, com embeddings locais (`sentence-transformers/all-mpnet-base-v2`, 768d) e conexão ao container do M0; sem cloud/auth proprietária; `ScaNN`→`HNSW`; banners honestos marcam multimodal e hybrid/RRF como alvo de roadmap (M7) e StreamingDiskANN como M2. Honestidade: a fachada é de referência em-repo, ainda não publicada no PyPI.
- Documentação oficial de especificação dos recursos vetoriais/IA do TheoDB em `docs/features/` (12 páginas enumeradas, kebab-case): `01-busca-similaridade-vetorial`, `02-indice-hnsw`, `03-indice-ivfflat`, `04-indice-ivf`, `05-indice-scann`, `06-busca-hibrida` (M2 — Vetorial/IA) e `07-funcoes-ia-sql`, `08-acelerar-consultas`, `09-ranquear-resultados`, `10-analise-sentimento`, `11-sumarizacao-conteudo`, `12-linguagem-natural` (M7 — IA avançada). A página `12-linguagem-natural` consolida a referência de API do `theodb_ai_nl` com um exemplo end-to-end. Cada página define a API-alvo do TheoDB e carrega um banner de status indicando o milestone-alvo e que o recurso ainda **não** está implementado na release atual (honestidade — Regra 3).

- `ROADMAP.md` na raiz (via `/roadmap-init`, slug `theodb`): roadmap macro M0–M8 promovido do roadmap do README (M0–M9), adaptado ao teto de 9 marcos e à doutrina walking-skeleton — M0 virou a fatia fina end-to-end (container PG17+pgvector com vector search) e o antigo M8 (Escala & observabilidade) foi fundido com o M9 (Ecossistema & DX). Inclui visão, problema, usuários, escopo in/out, constraints (Apache 2.0/AGPL-proibida, sem fork do engine, PG17→18), critério de ship V1 mensurável e north-star (migração AlloyDB→TheoDB). Grill sintetizada do PRD/README em `.claude/knowledge-base/grills/theodb-roadmap-grill.md`.
- 11 referências SOTA clonadas (shallow + blob:none) em `.claude/knowledge-base/references/` + catálogo em `.claude/knowledge-base/references-catalog.md`: 8 permissivas (pgvector, pgvectorscale, supabase/postgres, duckdb, pg_mooncake, cloudnative-pg, patroni, pgbackrest) e 3 AGPL-3.0 `clone-anyway-study-only` (paradedb, citus, hydra — risco legal reconhecido, só estudo; copiar código AGPL para a distribuição é proibido por D1). Catálogo registra a discrepância honesta de `pg_analytics`/paradedb (PRD §11 dizia PostgreSQL License; repo está AGPL hoje).
- Perfil de rigor PhD do `cycle-discover` para o TheoDB (projeto de fronteira): novo `rules/discover-phd-rigor.md` (contrato de rigor — SOTA-anchoring, ≥2 fontes primárias por técnica, evidência de benchmark ou marcador honesto `UNBENCHMARKED`, budget de fronteira) + ADR `knowledge-base/adrs/0001-discover-phd-rigor.md` que documenta as mudanças nas regras locked. `rules/discover-web-allowlist.txt` populado com domínios SOTA autoritativos (arXiv/DOI/venues, AlloyDB/ScaNN, pgvector/pgvectorscale/DuckDB/Postgres) — antes vazio, o discover era cego à literatura.
- `CLAUDE.md` — regras do projeto para o Claude Code. Princípio guia "Esforço ≠ Complexidade" (complexidade medida pela necessidade do projeto, não pelo esforço; esforço alto é bem-vindo quando há necessidade real, complexidade desnecessária é proibida sempre; anti-sunk-cost) + regras específicas do TheoDB (SOTA-anchored, Apache 2.0/AGPL-proibida, Política de Fork, sem fork do engine, performance só com benchmark, honestidade).
- `LICENSE` — Apache License 2.0 (texto oficial), a mesma licença do Supabase (decisão D1).
- Decisões D1–D7 fechadas no PRD §15 (antes "Questões em aberto"), ancoradas no SOTA AlloyDB: D1 licença Apache 2.0; D2 columnar DuckDB-powered permissivo (`pg_mooncake` MIT / `pg_analytics`); D3 índice ANN `pgvector` + `pgvectorscale`; D4 telemetria opt-in/anônima/desligada por padrão; D5 PostgreSQL 17 (MVP) → 18; D6 governança via DCO sem CLA; D7 control plane managed fora do v1.
- PRD inicial do TheoDB (`PRD.md`): define o produto inteiro — visão, problema/oportunidade, posicionamento vs AlloyDB Omni, personas, princípios, arquitetura ("PostgreSQL + extensões + pgvector customizado, sem fork"), os 10 pilares de capacidade (P1–P10), requisitos funcionais/não-funcionais, modelo open-source e licenciamento, riscos e recorte de MVP candidato.
- README inicial (`README.md`): posicionamento orientado a outcome, público-alvo, seção "como funciona" e roadmap macro de milestones (M0–M9).
- Seção de Referências no README: whitepaper ScaNN for AlloyDB (pesquisa aplicada do concorrente) e 24 papers seminais verificados, agrupados por pilar — vetorial/ANN (ScaNN, HNSW, DiskANN, Product Quantization, Faiss), embeddings/busca híbrida/reranking (Sentence-BERT, DPR, ColBERT, RRF, BEIR), text-to-SQL e segurança (Spider, BIRD, Indirect Prompt Injection), columnar/HTAP (C-Store, MonetDB/X100, HyPer, Citus), replicação/HA/DR (Raft, ARIES, Aurora, Spanner) e auto-tuning (Learned Indexes, OtterTune, AutoAdmin, Database Cracking).


### Changed

- `ROADMAP.md` alinhado ao North Star (ADR 0002, mandato CTO): **M2** reescrito de "adotar pgvectorscale" (paridade) para **superioridade com evidência** — harness de benchmark recall@k é o **gate / 1º item**, e a escolha do índice (pgvectorscale / fork D3 / **ScaNN-as-PG-AM**, algoritmo Apache-2.0) passa a ser **decidida pelo benchmark**, mirando igualar ou superar o ScaNN; **M4** ganhou nota de divergência honesta (Patroni ≠ storage desagregado do AlloyDB — aposta competitiva, não cópia); **M7** ganhou nota de sequência (hybrid primeiro; BM25 → discovery própria pelo gap AGPL; model-agnostic como alavanca de superioridade).
- `ROADMAP.md`: cada milestone (M0–M8) ganhou um bloco explícito **Entregáveis (artefatos concretos)** + **Documentação**, tornando inequívoco o que cada marco deve produzir. Os milestones de IA/vetorial passam a referenciar diretamente a documentação de especificação em `docs/features/` — M2 → `01-busca-similaridade-vetorial`/`02-indice-hnsw`/`03-indice-ivfflat`/`04-indice-ivf`/`05-indice-scann`; M7 → `06`/`07`/`08`/`09`/`10`/`11`/`12` (todos cobrem o DoD). Banner de `06-busca-hibrida.md` corrigido de M2 → M7 (hybrid search é M7 no roadmap). Per protocolo de revisão do ROADMAP.
- `ROADMAP.md` M7 (**escopo expandido por decisão do CTO**, 2026-06-26): DoD passa a incluir **funções de IA generativa em SQL** — `ai.generate`, `ai.if`, `ai.analyze_sentiment`, `ai.summarize`/`ai.agg_summarize` — sobre modelo configurável local/remoto, com versões otimizadas (`ai.*` aceleradas) e teste de contrato por função; Objective e Entregáveis atualizados; os docs `07-funcoes-ia-sql`/`08-acelerar-consultas`/`10-analise-sentimento`/`11-sumarizacao-conteudo` deixam de ser "extensão a confirmar" e passam a cobrir o DoD.
- `cycle-discover` endurecido para rigor PhD (ADR `0001-discover-phd-rigor`): budget de perguntas ampliado para fronteira (6–14 total, ≤5/corner, técnicas ≥2) via `skills/discover-plan-confidence/scripts/check_plan_completeness.py` (mantém-se dentro do hard cap locked ≤15); bands de verdict mais agressivos (SHIPPABLE 92, CAVEATS 75) em `discover-plan-thresholds.txt` e `discover-blueprint-thresholds.txt`; seção `§ 3.1 — Project rigor profile` adicionada aos dois golden rules locked de discover; `discover-plan/SKILL.md`, `discover-execute/SKILL.md` e o template de plano passam a exigir SOTA-anchoring + ≥2 fontes primárias + benchmark/`UNBENCHMARKED` na corner de técnicas; `cycle-discover.md` cross-referencia o perfil de rigor.
- PRD §11 (licenciamento): licença travada em Apache 2.0; due-diligence de dependências atualizada com licenças verificadas (pgvector/pgvectorscale/pg_analytics = PostgreSQL License; pg_mooncake = MIT; Citus/Hydra columnar/ParadeDB pg_search = AGPL → barradas).
- PRD §7/§8 (pilares P2/P3): P2 passa a citar `pgvector` + `pgvectorscale`; P3 passa de "columnar in-memory" para columnar DuckDB-powered permissivo (alinhado às decisões D2/D3).
- PRD D3 (§6/§13): adicionada **Política de Fork** — fork de `pgvector`/`pgvectorscale` autorizado quando houver avanço mensurável, sob contrato (upstream-first, gatilho por benchmark, diff mínimo, CI de rebase contínuo, desfazer quando o upstream alcançar). A regra "sem fork" segue valendo só para o engine PostgreSQL.
- README (Licença): de "a definir" para Apache 2.0 com link para `LICENSE`.

### Fixed

- `.claude/rules/discover-plan-thresholds.txt` — migrado do formato `band.x = N` (equals) para o formato canônico pipe-delimitado que `run_discover_plan_score.py` realmente lê (split `|`). O formato errado deixava as bandas vazias e colapsava o verdict de `/discover-plan-confidence` para INVALID independente do score; agora SHIPPABLE/CAVEATS/NEEDS_REVISION/INVALID resolvem corretamente.
- `.claude/skills/plan-confidence/scripts/check_evidence_citations.py` — `_scan_blueprint_refs` passa a procurar blueprints também sob `.claude/knowledge-base/discoveries/blueprints/` (antes só `knowledge-base/...`), espelhando o `_resolve_rule_file`. Sem isso, qualquer citação `Blueprint §X` num plano resolvia como fabricada (hard cap INVALID) neste layout `.claude/`.


### Security

- M0 Walking Skeleton: `Dockerfile` — base image fixada ao digest imutável `postgres:17-bookworm@sha256:17b6c778...` (H-1); garante que re-builds futuros não consumam silenciosamente uma imagem diferente caso o tag seja remapeado no Docker Hub.
- M0 Walking Skeleton: `Dockerfile` — pgvector referenciado pelo commit SHA imutável `#586e7515...` em vez da tag mutável `#v0.8.3` (H-2); elimina risco de `git tag -f` no upstream substituir a fonte na próxima build.

