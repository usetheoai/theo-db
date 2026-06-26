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

- Perfil de rigor PhD do `cycle-discover` para o TheoDB (projeto de fronteira): novo `rules/discover-phd-rigor.md` (contrato de rigor — SOTA-anchoring, ≥2 fontes primárias por técnica, evidência de benchmark ou marcador honesto `UNBENCHMARKED`, budget de fronteira) + ADR `knowledge-base/adrs/0001-discover-phd-rigor.md` que documenta as mudanças nas regras locked. `rules/discover-web-allowlist.txt` populado com domínios SOTA autoritativos (arXiv/DOI/venues, AlloyDB/ScaNN, pgvector/pgvectorscale/DuckDB/Postgres) — antes vazio, o discover era cego à literatura.
- `CLAUDE.md` — regras do projeto para o Claude Code. Princípio guia "Esforço ≠ Complexidade" (complexidade medida pela necessidade do projeto, não pelo esforço; esforço alto é bem-vindo quando há necessidade real, complexidade desnecessária é proibida sempre; anti-sunk-cost) + regras específicas do TheoDB (SOTA-anchored, Apache 2.0/AGPL-proibida, Política de Fork, sem fork do engine, performance só com benchmark, honestidade).
- `LICENSE` — Apache License 2.0 (texto oficial), a mesma licença do Supabase (decisão D1).
- Decisões D1–D7 fechadas no PRD §15 (antes "Questões em aberto"), ancoradas no SOTA AlloyDB: D1 licença Apache 2.0; D2 columnar DuckDB-powered permissivo (`pg_mooncake` MIT / `pg_analytics`); D3 índice ANN `pgvector` + `pgvectorscale`; D4 telemetria opt-in/anônima/desligada por padrão; D5 PostgreSQL 17 (MVP) → 18; D6 governança via DCO sem CLA; D7 control plane managed fora do v1.
- PRD inicial do TheoDB (`PRD.md`): define o produto inteiro — visão, problema/oportunidade, posicionamento vs AlloyDB Omni, personas, princípios, arquitetura ("PostgreSQL + extensões + pgvector customizado, sem fork"), os 10 pilares de capacidade (P1–P10), requisitos funcionais/não-funcionais, modelo open-source e licenciamento, riscos e recorte de MVP candidato.
- README inicial (`README.md`): posicionamento orientado a outcome, público-alvo, seção "como funciona" e roadmap macro de milestones (M0–M9).
- Seção de Referências no README: whitepaper ScaNN for AlloyDB (pesquisa aplicada do concorrente) e 24 papers seminais verificados, agrupados por pilar — vetorial/ANN (ScaNN, HNSW, DiskANN, Product Quantization, Faiss), embeddings/busca híbrida/reranking (Sentence-BERT, DPR, ColBERT, RRF, BEIR), text-to-SQL e segurança (Spider, BIRD, Indirect Prompt Injection), columnar/HTAP (C-Store, MonetDB/X100, HyPer, Citus), replicação/HA/DR (Raft, ARIES, Aurora, Spanner) e auto-tuning (Learned Indexes, OtterTune, AutoAdmin, Database Cracking).

### Changed

- `cycle-discover` endurecido para rigor PhD (ADR `0001-discover-phd-rigor`): budget de perguntas ampliado para fronteira (6–14 total, ≤5/corner, técnicas ≥2) via `skills/discover-plan-confidence/scripts/check_plan_completeness.py` (mantém-se dentro do hard cap locked ≤15); bands de verdict mais agressivos (SHIPPABLE 92, CAVEATS 75) em `discover-plan-thresholds.txt` e `discover-blueprint-thresholds.txt`; seção `§ 3.1 — Project rigor profile` adicionada aos dois golden rules locked de discover; `discover-plan/SKILL.md`, `discover-execute/SKILL.md` e o template de plano passam a exigir SOTA-anchoring + ≥2 fontes primárias + benchmark/`UNBENCHMARKED` na corner de técnicas; `cycle-discover.md` cross-referencia o perfil de rigor.
- PRD §11 (licenciamento): licença travada em Apache 2.0; due-diligence de dependências atualizada com licenças verificadas (pgvector/pgvectorscale/pg_analytics = PostgreSQL License; pg_mooncake = MIT; Citus/Hydra columnar/ParadeDB pg_search = AGPL → barradas).
- PRD §7/§8 (pilares P2/P3): P2 passa a citar `pgvector` + `pgvectorscale`; P3 passa de "columnar in-memory" para columnar DuckDB-powered permissivo (alinhado às decisões D2/D3).
- PRD D3 (§6/§13): adicionada **Política de Fork** — fork de `pgvector`/`pgvectorscale` autorizado quando houver avanço mensurável, sob contrato (upstream-first, gatilho por benchmark, diff mínimo, CI de rebase contínuo, desfazer quando o upstream alcançar). A regra "sem fork" segue valendo só para o engine PostgreSQL.
- README (Licença): de "a definir" para Apache 2.0 com link para `LICENSE`.
