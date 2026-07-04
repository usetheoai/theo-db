# ADR 0013 — Escopo dos pilares v1-legacy: columnar (M6) + BM25 (M7) — MANTER como exceções permissivas

**Status:** Accepted · **Data:** 2026-07-03 · **Deciders:** CTO (paulohenriquevn) · **Milestone:** M30
**Relacionado:** ADR `0002` (North Star; columnar fora-de-escopo → reabrir exige ADR — **este ADR**),
ADR `0003` (BM25 permissivo via `pg_textsearch`), `ROADMAP.md § M30` / `§ Fora de escopo do v2`
**Evidência:** `docs/benchmarks/m30-columnar-scale.md` (columnar-at-scale, NOVO), `docs/benchmarks/m6-columnar-vs-row.md`,
`docs/benchmarks/m7-bm25-vs-tsrank.md`

## Contexto e problema

Dois pilares foram explorados sob a tese **v1 (composição de extensões de terceiros)** e ficaram como
**medição throwaway, NÃO embarcada** no produto:

- **Columnar / HTAP (M6)** — `pg_mooncake` (+ `pg_duckdb`), um columnstore DuckDB+Iceberg (lakehouse em disco).
- **BM25 (M7)** — `pg_textsearch`, ranking lexical BM25.

O mandato **v2 (ADR 0006)** é código próprio, dependências mínimas. `ADR 0002` colocou columnar "fora de escopo
do v2 — reabrir exige ADR". **M30 é esse ADR:** decidir se columnar e BM25 são **mantidos, deprecados, ou
reescritos-próprios**. A superfície SHIPADA de texto do hybrid search é FTS **nativo do Postgres**
(`ts_rank_cd` + GIN) — composição própria sobre feature nativa (Regra 9), **intocada** por esta decisão.

## Drivers da decisão

1. **North Star = igualar/superar o AlloyDB** — que tem columnar/HTAP. Analytics sobre dados transacionais
   vivos é uma capacidade de banco geral (dashboards, rollups, e.g. observabilidade — **um** workload entre
   vários), não um nicho.
2. **Measurement-first (ADR 0002) + honestidade (Regra 3)** — nenhum pilar fica/some por opinião; a decisão é
   ancorada em benchmark reproduzível.
3. **Licença (D1)** — só permissivo. `pg_mooncake`/`pg_duckdb` = **MIT**; `pg_textsearch` = permissivo (ADR 0003).
   Os columnar in-memory (Citus/Hydra) são **AGPL → barrados** por D1 — DuckDB é a única rota permissiva.
4. **Regra 9 (não reinventar)** — reescrever columnar/BM25 do zero é inviável (DuckDB é battle-tested).

## Opções consideradas

- **(A) Deprecar-e-remover** ambos (foco puro vetor+IA, own-code mínimo).
- **(B) Reescrever-próprio** columnar/BM25 em Rust.
- **(C) MANTER** ambos como **exceções permissivas** (Regra 9), gated para adoção futura. ← **escolhida**

## Decisão

**MANTER os dois** como exceções permissivas explícitas ao mandato own-code, gated para uma milestone de
adoção futura. Por pilar:

### Columnar (M6, `pg_mooncake`) — MANTER
A decisão-crítica era: columnar ganha em escala? O M6 mediu só 100k (onde o row-store ganhava) e marcou o win
de escala **UNBENCHMARKED**. **M30 fechou esse gap** (`docs/benchmarks/m30-columnar-scale.md`, substrato
canônico `mooncakelabs/pg_mooncake` PG18):

| linhas (n) | row-store (Seq Scan) | columnstore (DuckDBScan) | speedup | correto? |
|---|---|---|---|---|
| 100.000 | 9.2 ms | 4.0 ms | **2.33×** | sim |
| 1.000.000 | 62.3 ms | 7.2 ms | **8.65×** | sim |
| 5.000.000 | 397.4 ms | 26.6 ms | **14.94×** | sim |

O speedup **cresce com a escala** (2.3× → 15×) numa agregação `GROUP BY` analítica, com resultado **byte-correto**
(count exato + avg dentro de 1e-3) e plano `DuckDBScan` vetorizado. É a assinatura clássica do columnar. ⇒ Um
pilar de analytics/HTAP real, permissivo (MIT), medido-vencedor a escala. Deprecá-lo jogaria fora paridade com
o AlloyDB.

### BM25 (M7, `pg_textsearch`) — MANTER
`docs/benchmarks/m7-bm25-vs-tsrank.md`: **nDCG@10 BM25 0.9546 vs `ts_rank_cd` nativo 0.5143** — um ganho grande
de qualidade lexical, permissivo (ADR 0003). Deprecá-lo descartaria um win medido (Regra 3). ⇒ Mantido; o leg
lexical SHIPADO continua o `ts_rank_cd` nativo; BM25 é candidato de adoção.

## Consequências

- **Positivo:** o produto mantém o caminho para analytics/HTAP (paridade AlloyDB) e para BM25 (qualidade
  lexical superior), ambos com evidência medida, ambos permissivos.
- **Custo (honesto):** duas dependências de terceiros permanecem como **exceções à regra own-code** — declaradas
  explicitamente como tal no `ROADMAP.md` (justificativa Regra 9 + evidência).
- **Feasibility / caminho de adoção (gated, milestone futura — NÃO neste M30):** columnar hoje NÃO é embarcado
  (a imagem shipada é PG17; `pg_mooncake` tem prebuilt só em PG18). Duas rotas: **(a)** consertar o build PG17
  from-source (Rust+cargo-pgrx+DuckDB — travou num pin rustc/MSRV, "resolvable toolchain issue"), ou **(b)**
  bump PG17→PG18 (prebuilt). A decisão de qual rota + o shipping são uma milestone de adoção separada.
- **Escopo desta ADR:** decisão + evidência. Zero mudança de código de produto (`theodb_rs`/`sql`/Dockerfile
  shipado intocados); o benchmark roda sobre o substrato throwaway.

## Prós/contras das rejeitadas

- **(A) Deprecar** — mais enxuto, mas: joga fora paridade AlloyDB (columnar) + um win lexical medido (BM25);
  contradiz a evidência (columnar 15× a 5M). Rejeitada.
- **(B) Reescrever-próprio** — YAGNI + esforço enorme; DuckDB é maduro (Regra 9). Rejeitada.
