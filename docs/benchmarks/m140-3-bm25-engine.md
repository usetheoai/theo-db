# M140.3 — engine BM25 de produção own-code: cache vs reload + nDCG in-PG (medido)

> Medido 2026-07-22 no droplet e2e-runner (165.227.121.20, 32GB), PostgreSQL **18.4** (pgrx 0.19.0), extensão
> `theodb_rs` (feature `spike-lexical`) instalada via `cargo pgrx install`. Runner: `benchmarks/run_m140_3_engine.py`.
> Dados: `docs/benchmarks/m140-3-data/{latency-sweep,ndcg-and-latency-n2000}.json`. Smoke SQL (funcional+MVCC):
> `scripts/m140-3-bm25-smoke.sh`. Plano: `.claude/knowledge-base/plans/m140-3-bm25-production-engine-plan.md`. ADRs: `0052` (heap), `0054` (supersede pg_textsearch).

## Headline (honest-positive)

**A engine BM25 own-code de produção (`bm25_build`/`bm25_search`, sobre heap, com cache do Directory
MVCC-correto) mata o reload-por-query do spike M139 — e o ganho ESCALA com o tamanho do índice.** O nDCG@10
in-PG reproduz o M140.1 byte-a-byte (0,6611 scifact), confirmando a qualidade na forma final.

- **Latência (o Goal):** o cache elimina o custo de reload (load do heap + rebuild do índice); nos 3 pontos o
  **reload cresce mais rápido** que a busca cacheada (a razão cache/reload cai 0,62→0,36→0,22). O gate
  `cache < 50% reload` é **atingido em N ≥ ~5k** (estimado; a fronteira em 2k é ruidosa): em N=50k o cache é
  **4,5× mais rápido** (ratio 0,22). Atribuição mecanística (o reload é a única diferença de custo estrutural),
  não A/B controlado — ver caveats.
- **nDCG@10 (DoD-2):** a engine de produção in-PG reproduz o M140.1 (0,6611 scifact) — **~4% abaixo do**
  `pg_textsearch` (0,688; não significance-tested, atribuído a impl/tokenização BM25) e vitória dramática sobre o
  `ts_rank` shipado (0,072, o baseline real do theo-lens).
- **MVCC:** provado no smoke — uma sessão com snapshot antigo NÃO vê o build de outra sessão (o cache é
  invalidado pela geração lida sob o snapshot).

## Latência — cache vs reload-por-query (sweep de N, mean de 30 buscas, `latency-sweep.json`)

| N docs | `bm25_search` (cache) | `lexical_spike_search` (reload) | ratio cache/reload | ganho |
|---|---|---|---|---|
| 2 000 | 2,65 ms | 4,25 ms | 0,62 | 1,6× (reload ainda barato) |
| 10 000 | 5,16 ms | 14,22 ms | **0,36** | 2,8× |
| 50 000 | 11,76 ms | 54,00 ms | **0,22** | **4,5×** |

**Interpretação honesta (com os caveats de rigor do council-benchmark):**

- **O que o cache elimina:** o **reload** (o `load` do heap + rebuild do `MemStore`/índice que o spike fazia a
  CADA query). Nos 3 pontos, o **reload cresce mais rápido** que a busca cacheada (não "linear"/"flat" — é
  sub-linear em ambos: reload 4,25→14,22→54,0 = 12,7× para 25×N; cache 2,65→5,16→11,76 = 4,4×), então a **razão
  cache/reload cai** (0,62→0,36→0,22). O gate `<50%` é atingido em N≥~5k (interpolado; a fronteira em 2k é
  ruidosa — ver variância abaixo).
- **A comparação é confundida (atribuição mecanística, não A/B limpo):** o braço "reload" usa o motor do spike
  (`lexical_spike_search`, schema body-only, toca 1 doc) e o braço "cache" usa a engine de produção
  (`bm25_search`, schema id+body, materializa 10 docs). São motores/schemas distintos. **Os confounders são
  conservadores** — o motor com cache faz MAIS trabalho por query e ainda é mais rápido; a variável realmente
  isolada é o `load`+`Index::open` a cada query vs o `cache.get_or_build`. A atribuição do ganho ao cache é
  **mecanística** (o reload é a única diferença de custo estrutural), não um isolamento controlado.
- **Selectividade máxima (pior caso):** a query `'lazy'` aparece em TODOS os docs → casa 100% do corpus (custo de
  scan máximo). Uma query seletiva (1 hit) teria reload menor e o ganho do cache seria menos pronunciado. A
  latência medida é o **pior caso de selectividade**.
- **Variância (rigor):** k=30 buscas, `mean` reportado (o `p50` nos JSON ≈ mean). Execuções independentes de N=2k
  deram ratio **0,55–0,66** — variância real bem na fronteira do gate 0,5, por isso o `~5k` é uma estimativa, não
  um crossover cravado.

**Para o consumidor real (theo-lens — corpora de traces de milhares a milhões de spans), o reload-por-query
seria proibitivo e o cache é uma vitória clara** — a conclusão (o ganho do cache **cresce** com N) é medida e
reproduzida (council-benchmark re-rodou a direção). Mostrar o scaling honesto, não cherry-pickar um N.

## Qualidade — nDCG@10 in-PG (forma final, `ndcg-and-latency-n2000.json`)

| Dataset | docs | queries | `bm25_search` nDCG@10 | M140.1 (mesmo motor, off-PG) | pg_textsearch (M138) | ts_rank (baseline shipado) |
|---|---|---|---|---|---|---|
| BEIR scifact | 5 183 | 300 | **0,6611** | 0,6611 | 0,688 | 0,072 |

A engine de produção in-PG reproduz o M140.1 **byte-a-byte** (0,6611) — o cache não muda o ranking (D3: ranking é
independente do storage), e a superfície SQL entrega a mesma qualidade que o harness off-PG. **~4% ABAIXO do
`pg_textsearch`** (0,6611 vs 0,688; não significance-tested, impls BM25 distintas — NÃO é superioridade) e
**vitória dramática sobre `ts_rank`** (o baseline que
o theo-lens ships). Honestidade (Regra 7): o argumento do M140 **não** é bater `pg_textsearch` (a engine mede
~4% ABAIXO dele, não significance-tested — não superioridade); é
own-code permissivo + cache + índice ~3,5× menor (M140.1) + o moat de consolidação in-PG.

## Correção MVCC (smoke, `scripts/m140-3-bm25-smoke.sh` — 9/9 OK)

Prova de duas sessões: A abre `REPEATABLE READ`, estabelece o snapshot na geração 1 (só 'alpha'); B (backend
separado) adiciona 'betamax' e reconstrói (geração 2, commitada); **A ainda NÃO vê 'betamax'** (`A_sees_betamax=0`)
— o cache de A é keyed pela geração que o snapshot de A enxerga (gen 1), então reconstrói do estado heap visível a
A, nunca serve o build mais novo. Após A commitar, um novo snapshot vê a geração 2. Isto é o cache **MVCC-correto**.

## Consequência para o roadmap

- **Gate M140.3 PASSA** — a superfície BM25 de produção own-code opera, com cache que mata o reload-por-query (o
  Goal, atingido no regime realista), nDCG in-PG confirmado, MVCC provado.
- **ADR-0054** supersede a exceção `pg_textsearch` do ADR-0013 (own-code é a superfície BM25).
- **M140.4** prova MVCC/VACUUM/crash a fundo (suítes de isolamento contra o binário shipado) e liga o primeiro
  consumidor real (theo-lens).

## Reprodução

```bash
# no e2e-runner (pgrx 0.19 + PG18):
cd theodb_rs && cargo pgrx install --features spike-lexical --pg-config ~/.pgrx/18.4/pgrx-install/bin/pg_config
bash scripts/m140-3-bm25-smoke.sh          # funcional + MVCC (9/9 OK)
# inicia um PG com a extensão, então:
cd benchmarks && python3 run_m140_3_engine.py --dsn "host=127.0.0.1 port=PORT dbname=postgres user=postgres" \
    --n-sweep "2000,10000,50000" --beir scifact --out ../docs/benchmarks/m140-3-data/result.json
```
