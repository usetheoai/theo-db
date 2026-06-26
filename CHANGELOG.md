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

- PRD inicial do TheoDB (`PRD.md`): define o produto inteiro — visão, problema/oportunidade, posicionamento vs AlloyDB Omni, personas, princípios, arquitetura ("PostgreSQL + extensões + pgvector customizado, sem fork"), os 10 pilares de capacidade (P1–P10), requisitos funcionais/não-funcionais, modelo open-source e licenciamento, riscos e recorte de MVP candidato.
- README inicial (`README.md`): posicionamento orientado a outcome, público-alvo, seção "como funciona" e roadmap macro de milestones (M0–M9).
- Seção de Referências no README: whitepaper ScaNN for AlloyDB (pesquisa aplicada do concorrente) e 24 papers seminais verificados, agrupados por pilar — vetorial/ANN (ScaNN, HNSW, DiskANN, Product Quantization, Faiss), embeddings/busca híbrida/reranking (Sentence-BERT, DPR, ColBERT, RRF, BEIR), text-to-SQL e segurança (Spider, BIRD, Indirect Prompt Injection), columnar/HTAP (C-Store, MonetDB/X100, HyPer, Citus), replicação/HA/DR (Raft, ARIES, Aurora, Spanner) e auto-tuning (Learned Indexes, OtterTune, AutoAdmin, Database Cracking).
