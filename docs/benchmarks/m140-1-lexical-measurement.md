# M140.1 — BM25 own-engine vs `ts_rank_cd` vs `pg_textsearch` (medido)

> Medido 2026-07-22, localmente. PostgreSQL **18** em docker (`postgres:18`, porta 55432),
> Tantivy **0.26** (tantivy-py, MIT — o mesmo motor do spike M139). Runner:
> `benchmarks/run_m140_1_lexical.py`. Dados: `docs/benchmarks/m140-1-data/{beir,logproxy}.json`.
> Gate offline: `benchmarks/theodb_bench/test_m140_1_decision.py` (verde sobre os JSON reais).
> Plano: `.claude/knowledge-base/plans/m140-1-lexical-measurement-plan.md`. ADR de storage: `docs/adr/0052`.

## Headline (honest-positive, com magnitude honesta)

**O gate do M140 PASSA: a BM25 own-engine (Tantivy) bate o baseline `ts_rank_cd` em retrieval
lexical puro — o caso de uso do theo-lens — em dois eixos independentes, reproduzindo o M138.**
Mas a **magnitude é contexto-dependente e honesta**, não um "5× universal":

- **BEIR (qualidade graded, qrels humanos):** BM25 vence `ts_rank_cd` com significância em ambos
  os corpora (scifact **0,661 vs 0,072**; nfcorpus **0,308 vs 0,206**), reproduzindo o M138.
- **Logs HDFS reais (known-item):** BM25 vence em **todos** os comprimentos de query (direção
  robusta, p<1e-5), mas **modestamente** no regime justo (m=1: +13%; m=2: +9%); o gap enorme em
  m≥3 é **artefato de semântica de query**, declarado abaixo — **não** é o headline.
- **Storage:** índice Tantivy **2,5× menor** que o heap+GIN do PostgreSQL (confirma o 2,8× do M139).

**Consequência:** o M140 segue (M140.2→M140.4). O valor real é **ranking lexical puro melhor +
índice 2,5× menor + tokenização de logs/IDs + features (phrase/fuzzy/facet) + o moat de
consolidação in-PG** — não um ganho dramático de ranking em query natural curta. Honestidade
(TheoDB rule 7): o ganho de qualidade em query curta e realista é **modesto**; o ganho graded
(BEIR) é grande porque `ts_rank_cd` é um ranker fraco onde o lexical importa.

## Eixo 1 — BEIR (nDCG@10, qrels humanos, teste pareado de permutação)

| Dataset | docs | queries | **BM25 own** | `ts_rank_cd` | mean_diff | p (perm) | W/L/T | M138 (ref) |
|---|---|---|---|---|---|---|---|---|
| scifact | 5 183 | 300 | **0,6611** | 0,0724 | +0,589 | <1e-5 | 223/1/76 | BM25 0,688 / ts 0,070 |
| nfcorpus | 3 633 | 323 | **0,3077** | 0,2060 | +0,102 | <1e-5 | 143/35/145 | BM25 0,325 / ts **0,206** |

**Validação cruzada (não-fabricação):** o `ts_rank_cd` de nfcorpus (**0,206**) bate **exatamente**
o número do M138; o BM25 (0,661/0,308) reproduz o leg BM25 do M138 (0,688/0,325) dentro de ~0,03
(a diferença é Tantivy-own-BM25 vs pg_textsearch-BM25 — motores BM25 distintos, mesma ordem).

## Eixo 2 — Logs HDFS reais, known-item (MRR@10, 3 seeds, LogHub HDFS_2k)

Metodologia: query = os `m` termos mais distintivos de um doc; o relevante é aquele doc (TREC
named-page finding — sem qrels humanos, nada fabricado). **m=1 é o teste de ranking mais justo**
(um único termo neutraliza a diferença de parser Tantivy-OR vs `websearch_to_tsquery`-AND).

| m (termos) | BM25 own MRR | `ts_rank_cd` MRR | flip (p<1e-5) | interpretação |
|---|---|---|---|---|
| **1** (justo) | 0,413 ± 0,029 | 0,366 | ✅ | ranking puro: **+13%**, modesto e honesto |
| **2** (realista) | 0,692 ± 0,008 | 0,635 | ✅ | +9%, query realista |
| 3 | 0,933 ± 0,005 | 0,458 | ✅ | semântica começa a dominar |
| 5 | 0,991 ± 0,004 | 0,202 | ✅ | **ARTEFATO** (ver abaixo) — não reportar como ranking |

### Por que m≥3 infla o gap (o artefato, declarado)

`websearch_to_tsquery('english', ...)` usa **AND** entre termos e gera `<->` (frase) para tokens
compostos (ex.: `blk_38865049064139660` → `'blk' <-> '38865049064139660'`); o parser default do
Tantivy usa **OR**. Com 5 termos raros ANDados + divergência de tokenização (o PG lexa IP
`10.251.73.220` como um token; nosso extrator de termos o separa), o alvo do `ts_rank` **deixa de
casar** → MRR despenca. Isso é **semântica de query + tokenização, não qualidade de ranking**.
Reportar o m=5 como "BM25 5× melhor" seria spin (lente council-benchmark: *você mediu ou está
supondo?*). A direção (BM25 ≥ ts_rank) é robusta em todo m; a **magnitude honesta** é a do m=1–2.

## Storage (T3.1 — candidatos de storage no corpus de logs)

| Métrica | Tantivy (heap buffer-then-flush) | PostgreSQL heap+GIN |
|---|---|---|
| Índice | **626 KB** | 1 564 KB |
| Fator | **2,5× menor** | baseline |
| Ingest (2000 docs) | ~41 ms | — |

Decisão consolidada no **ADR 0052**: heap buffer-then-flush (MVCC/WAL/crash de graça — M139),
index AM custom **rejeitado** por over-engineering sem benefício medido.

## Caveat de fidelidade (declarado — ADR D1 do plano)

O eixo 2 usa um **corpus público de logs (LogHub HDFS_2k)** como proxy lexical trace-like — **não**
é tráfego de produção do theo-lens (que não existe no repo; scout exaustivo confirmou). A validação
em **traces reais** é o boundary explícito do **M140.4** (consumidor theo-lens) e do **M141**
(dogfood ≥30 dias). Este milestone mede um **sinal**, não uma prova de produção.

## Consequência para o roadmap

- **Gate M140.1 PASSA** — BM25 own-engine bate `ts_rank_cd` no retrieval lexical puro (o caso do
  theo-lens); o M140 é justificado a seguir (M140.2 crate núcleo → M140.3 engine de produção →
  M140.4 MVCC/crash provados + consumidor).
- **Framing honesto travado:** "ranking lexical puro melhor + índice 2,5× menor + tokenização de
  logs + features + moat in-PG aberto/permissivo" — **jamais** "5× melhor em busca de traces".
- **Storage decidido** (ADR 0052): heap, não AM custom.

## Reprodução

```bash
docker run -d --name m140pg -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgres:18
export M140_DSN="host=127.0.0.1 port=55432 dbname=postgres user=postgres password=postgres"
cd benchmarks
# corpus público de logs (uma vez): baixe HDFS_2k para .cache140/hdfs.log
curl -fsSL https://raw.githubusercontent.com/logpai/loghub/master/HDFS/HDFS_2k.log -o .cache140/hdfs.log
python3 run_m140_1_lexical.py --dataset hdfs --seeds 3 --n 2000 --out ../docs/benchmarks/m140-1-data/logproxy.json
python3 run_m140_1_lexical.py --beir scifact,nfcorpus       --out ../docs/benchmarks/m140-1-data/beir.json
python3 -m pytest theodb_bench/test_m140_1_decision.py -q   # gate offline sobre os JSON reais
```
