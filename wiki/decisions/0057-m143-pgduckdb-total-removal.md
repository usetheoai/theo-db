---
type: Decision
title: ADR 0057 — Remoção total do pg_duckdb: lakehouse Parquet em código próprio
description: O último componente C++ do projeto sai; ler, escrever e agregar Parquet passa a ser código próprio sobre DataFusion, custando +9 MB contra os 118 MB do bundle removido.
resource: git:f7c7b93:docs/adr/0057-m143-pgduckdb-total-removal.md
tags: [adr, pg-duckdb, parquet, datafusion, lakehouse, own-code, m143]
adr_id: "0057"
adr_status: Accepted
decision_date: 2026-07-22
milestone: M143
owner: human:paulohenriquevn
amends: ["0020", "0056"]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0057
    resource: git:f7c7b93:docs/adr/0057-m143-pgduckdb-total-removal.md
    title: ADR 0057 — Remoção total do pg_duckdb
    last_modified: 2026-07-22
---

Conclui a jornada iniciada no [ADR 0020](/decisions/0020-m61-embed-pgduckdb.md) e continuada no
[ADR 0056](/decisions/0056-m142-pgduckdb-htap-tiering.md): o
[pg_duckdb](/technologies/pg-duckdb.md) — **único componente C++ do projeto**, com bundle estático de
118 MB — é **removido por completo**.

O que tornou isso possível: um spike **mediu** que ler Parquet em código próprio via
[DataFusion](/technologies/datafusion.md) — Apache-2.0, **já presente no binário** — é viável, com
paridade byte a byte a **+9 MB**, contra os 118 MB do DuckDB
([spike](/benchmarks/parquet-reader-owncode-spike.md)).

# Decisões

**D1 — leitura própria.** `read_parquet(path)` retorna `SETOF jsonb`, com cada linha do Parquet virando
um jsonb — o que **cobre todos os tipos, inclusive aninhados**, sem a complexidade de `SETOF record`
dinâmico no pgrx. E `olap(path)` retorna o agregado tipado, com paridade byte a byte contra o
pg_duckdb, provada. Reusa a API e o runtime já existentes no executor.

**D2 — escrita própria, e o colapso do codegen.** `write_parquet(rel, path)` lê a tabela, converte
para Arrow e escreve com writer atômico (temp mais rename).

E o desenho de **codegen** do [ADR 0021](/decisions/0021-m62-htap-codegen-surface.md) — funções que
*retornavam texto* para o cliente executar — **colapsa**. Ele existia **só** porque o pg_duckdb proibia
DuckDB dentro de função; essa restrição **some** com código próprio. As funções agora escrevem,
registram, leem e agregam **dentro da própria função**.

**D3 — uma imagem só.** O lakehouse passa a vir no build **default**, e a imagem htap é **aposentada**.
Custo medido: **+9 MB** contra os **118 MB** removidos.

**D4 — tipos: amplos na leitura, escalares na escrita v1, fail-closed.** A leitura cobre tudo; a
escrita v1 cobre os escalares comuns, e um tipo não suportado gera **erro tipado fail-closed** — o dado
continua legível pela leitura, e a escrita ampla é follow-on.

**Least-privilege:** as primitivas de leitura e escrita de arquivo do lado do servidor são
**superuser-only**, como o `COPY … TO file`, e a superfície de usuário também é revogada — um papel sem
privilégio não contorna chamando a primitiva direto.[^adr0057]

# Alternativas rejeitadas

**Manter o tier-out** — o spike mediu que o código próprio custa **1/13 do tamanho com paridade**;
manter 118 MB de C++ e httpfs por uma capacidade que o código próprio entrega é complexidade
acidental. **`SETOF record` dinâmico** — complexo no pgrx, e tipos aninhados não mapeiam para colunas
escalares. **Manter o codegen chamando código próprio** — manteria a dança de "o cliente executa" **sem
a restrição que a justificava**. **Reescrever o motor DuckDB inteiro** — anos de trabalho, quando o
DataFusion já resolve e só faltava ligar o leitor.

# Consequências

Lakehouse Parquet completo em código próprio no build default, **sem DuckDB**, numa imagem só e **sem
componente C++**.

**Restringe:** a escrita v1 cobre escalares. **Compatibilidade:** as antigas funções de codegen foram
**removidas** no upgrade; quem as chamava migra para as novas, mais simples. O raio de dano é baixo,
por ser pré-1.0 e sem uso em produção.

**Validação medida:** round-trip completo sem pg_duckdb, leitor multi-tipo, escrita falhando fechado em
tipo não suportado, catálogo de extensões sem o pg_duckdb, e o delta de tamanho confirmado
([m143](/benchmarks/m143-pgduckdb-removal.md)).

[^adr0057]: ADR 0057 — Remoção total do pg_duckdb: lakehouse Parquet own-code
