---
slug: pgduckdb-total-removal
generated_by: roadmap-feature
milestone_id: M143
date: 2026-07-22
status: completed
---

# Grill — M143 remoção total do pg_duckdb (lakehouse Parquet own-code)

## Q1 — O que é e por que agora?

Remover o `pg_duckdb` **inteiro** do TheoDB, substituindo o lakehouse de arquivos externos por um caminho
**own-code** em Rust (DataFusion + Arrow, já no binário; Apache-2.0). Por quê agora: o **spike da Fase 4**
(`docs/benchmarks/parquet-reader-owncode-spike.md`) **mediu** que o leitor Parquet own-code é **VIÁVEL** —
paridade **byte-a-byte** vs `pg_duckdb.read_parquet` a **+9 MB** no `.so` vs **118 MB** do bundle DuckDB
(~13× menor), sem C++/httpfs. Viabilidade provada → agora é executar a substituição e **eliminar o último
componente C++/httpfs do projeto**, mantendo a capacidade lakehouse.

## Q2 — Dependências (`[x]`)

**M142** (o tier-out — a imagem `theodb-htap` que hospeda o pg_duckdb hoje, que este milestone vai aposentar) +
**M100** (o DataFusion executor own-code que o read/write Parquet reusa — Regra 9). Ambas `[x]`. O spike
(gate de viabilidade, GO) é pré-requisito informal já satisfeito.

## Q3 — Definition of done (medido)

1. `theodb.read_parquet` own-code (DataFusion, **sem DuckDB**) lê Parquet externo e produz o agregado do M62 —
   **paridade byte-a-byte** vs o baseline pg_duckdb (reusa/estende `scripts/spike-parquet-validate.sh`).
2. **Write Parquet own-code** (o `COPY→Parquet` do `htap_refresh_sql` → `DataFrame::write_parquet`) — round-trip
   escreve+lê+agrega correto.
3. `sql/85` reescrito: `olap_sql`/`htap_refresh_sql` usam o caminho próprio (`theodb.*`), **não** `duckdb.query` —
   a superfície M62 funciona **sem** pg_duckdb. Guard M142 removido/ajustado (não há mais o que guardar).
4. `Dockerfile.htap` **dropa** o pg_duckdb (estágio C++, COPY, `shared_preload`, `libcurl4`, `CREATE EXTENSION`) —
   e o lakehouse own-code **dobra no build default** → a imagem `theodb-htap` deixa de existir (decisão do owner).
   Delta de tamanho medido (~118 MB de C++ fora, ~9 MB de Rust dentro) em `docs/benchmarks/`.
5. Feature `spike-parquet` **promovida a permanente** (a superfície liga por default); smoke lê+escreve+agrega
   Parquet own-code sem `pg_duckdb` em `pg_extension`.
6. **Suporte amplo a tipos** (decisão do owner): mapear a maioria dos tipos Parquet/Arrow comuns
   (text, numéricos, timestamp/date, bool, e — na medida do viável — nested/list/struct), com **erro tipado
   fail-closed** para o que não for suportado.
7. ADR emendando 0056/0020 (remoção total) + README (lakehouse own-code no default) + CHANGELOG.

## Q4 — Top 2 riscos NOVOS

- **R1 (escopo/tipos — ampliado pela decisão "amplo"):** o spike provou correção+paridade num arquivo pequeno de
  **shape fixo**; suporte **amplo** a tipos (nested/list/struct) é escopo grande e pode virar projeto maior que um
  milestone. Mitigação: ordenar por measurement-first (cobrir primeiro os tipos que o M62/lakehouse exercita, medir,
  expandir); **declinar** tipos ainda-não-suportados com erro tipado (nunca silencioso); se o "amplo" estourar o
  tamanho de um milestone, **dividir** (v1 = tipos escalares comuns; v2 = nested) em vez de inchar.
- **R2 (SETOF dinâmico no pgrx):** `read_parquet(path)` de schema **arbitrário** retornando SETOF record dinâmico
  no pgrx é mais complexo que o shape fixo do spike. Mitigação: começar pelo shape que o M62 precisa + o padrão de
  composite dinâmico do pgrx só onde o caso de uso exigir.

## Out-of-scope cross-check

Seção `### Explicitly out of scope` do ROADMAP vazia → nenhum overlap a resolver.

## Decisões confirmadas pelo owner (2026-07-22)

- Direção: **remover o pg_duckdb inteiro** (não manter).
- Dependências: **M142 + M100**.
- Topologia final: **dobrar o lakehouse no default** — a imagem `theodb-htap` é aposentada.
- Escopo de tipos: **amplo** (maioria dos tipos comuns) — com o risco de escopo capturado em R1.
