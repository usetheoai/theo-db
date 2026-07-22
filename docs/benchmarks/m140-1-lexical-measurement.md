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
- **Storage:** índice Tantivy **~3,5× menor** no footprint enxuto justo (até ~5× vs o baseline fiel; 1,7× vs o GIN sozinho) — direção robusta, consistente com o M139.

**Consequência:** o M140 segue (M140.2→M140.4). O valor real é **ranking lexical puro melhor +
índice ~3,5× menor + tokenização de logs/IDs + features (phrase/fuzzy/facet) + o moat de
consolidação in-PG** — não um ganho dramático de ranking em query natural curta. Honestidade
(TheoDB rule 7): o ganho de qualidade em query curta e realista é **modesto**; o ganho graded
(BEIR) é grande porque `ts_rank_cd` é um ranker fraco onde o lexical importa.

## Eixo 1 — BEIR (nDCG@10, qrels humanos, teste pareado de permutação)

| Dataset | docs | queries | **BM25 own** | `ts_rank_cd` | mean_diff | p (perm) | W/L/T | M138 (ref) |
|---|---|---|---|---|---|---|---|---|
| scifact | 5 183 | 300 | **0,6611** | 0,0724 | +0,589 | <1e-5 | 223/1/76 | BM25 0,688 / ts 0,070 |
| nfcorpus | 3 633 | 323 | **0,3077** | 0,2060 | +0,102 | <1e-5 | 143/35/145 | BM25 0,325 / ts **0,206** |

**Validação cruzada (não-fabricação, travada pelo gate):** o `ts_rank_cd` de nfcorpus (**0,206**)
bate o número do M138 (0,206117) dentro de ±0,03 — agora **assertado** por
`test_beir_ts_rank_reproduces_m138_within_tolerance` (não só prosa, review M3); o BM25 (0,661/0,308)
reproduz o leg BM25 do M138 (0,688/0,325) dentro de ~0,03.

**Enquadramento honesto (review M2):** este é um head-to-head **pipeline-vs-pipeline** — a pipeline
Tantivy inteira (tokenizer `default`, **sem stemming/stopword**) vs a pipeline PostgreSQL inteira
(`to_tsvector('english')`, **com stemming + remoção de stopword**). Não é uma comparação controlada
só da *função de score* BM25 vs ts_rank_cd. É a pergunta de **produto** certa (o baseline que o
theo-lens *ships* vs a engine own-code proposta), mas atribuir o ganho puramente ao "BM25 scoring"
seria impreciso: parte vem do tokenizer/pré-processamento. O que a medição autoriza é "a engine
own-code proposta bate o baseline shipado", não "a fórmula BM25 é N× melhor que ts_rank_cd".

## Eixo 2 — Logs HDFS reais, known-item (MRR@10, 3 seeds, LogHub HDFS_2k)

Metodologia: query = os `m` termos **mais raros no doc** (por frequência local; desempate
alfabético — é raridade-no-doc, não IDF-de-corpus, review L1); o relevante é aquele doc (TREC
named-page finding — sem qrels humanos, nada fabricado). **m=1 é o teste de ranking mais justo**
(um único termo neutraliza a diferença de parser Tantivy-OR vs `websearch_to_tsquery`-AND).

**Significância (review M1):** o teste pareado de permutação é aplicado **por seed** (300 alvos
cada), **não** sobre os 3 seeds agrupados — o corpus HDFS_2k tem exatamente 2000 linhas, então os
3 seeds compartilham o **mesmo corpus** (variam só a amostra de 300 alvos); agrupar as 900
observações seria pseudo-replicação. O `flip` reportado exige `p<0,05` em **todos** os 3 seeds; o
`p` mostrado é o **pior caso** entre seeds. Em m=2 os p por-seed são [1e-5, 5e-5, 7e-5] — cada seed
significativo independentemente.

| m (termos) | BM25 own MRR | `ts_rank_cd` MRR | flip (3/3 seeds, p<1e-4) | interpretação |
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

## Storage (T3.1 — apples-to-apples, HDFS_2k, seed 0)

Índice Tantivy limpo de 1 segmento (**313 KB** — reproduzível; o bug de dupla-indexação do
draft anterior foi corrigido, review H1). Comparação em três framings honestos (review H2):

| Framing (o que compara) | Tantivy | PostgreSQL | Fator |
|---|---|---|---|
| **índice-vs-índice** (e o índice Tantivy ainda guarda o body; o GIN não) | 313 KB | GIN 532 KB | **1,7× menor** |
| **footprint enxuto** (heap+GIN+pkey+toast, sem coluna tsv materializada) | 313 KB | 1 097 KB | **3,5× menor** |
| **footprint fiel** (o baseline que o theo-lens *ships*: coluna `search_tsv` materializada, `schema.ts:84`) | 313 KB | 1 565 KB | **5,0× menor** |

Ingest Tantivy (2000 docs): ~41 ms. **A direção (Tantivy menor) é robusta em todos os
framings**; o número honesto é **~3,5× no footprint enxuto justo**, até ~5× vs o baseline fiel.
O "2,5×" do draft anterior era um artefato (net de dois erros opostos: índice Tantivy duplicado
+ PG inflado pela coluna tsv redundante) — corrigido. Decisão consolidada no **ADR 0052**: heap
buffer-then-flush (MVCC/WAL/crash de graça — M139), index AM custom **rejeitado** por
over-engineering sem benefício medido.

## Caveat de fidelidade (declarado — ADR D1 do plano)

O eixo 2 usa um **corpus público de logs (LogHub HDFS_2k)** como proxy lexical trace-like — **não**
é tráfego de produção do theo-lens (que não existe no repo; scout exaustivo confirmou). A validação
em **traces reais** é o boundary explícito do **M140.4** (consumidor theo-lens) e do **M141**
(dogfood ≥30 dias). Este milestone mede um **sinal**, não uma prova de produção.

## Consequência para o roadmap

- **Gate M140.1 PASSA** — BM25 own-engine bate `ts_rank_cd` no retrieval lexical puro (o caso do
  theo-lens); o M140 é justificado a seguir (M140.2 crate núcleo → M140.3 engine de produção →
  M140.4 MVCC/crash provados + consumidor).
- **Framing honesto travado:** "ranking lexical puro melhor + índice ~3,5× menor + tokenização de
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
