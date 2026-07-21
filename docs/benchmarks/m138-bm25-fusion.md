# M138 — fusão híbrida com BM25 vs com `ts_rank_cd` (medido; HONEST-NEGATIVE)

> Medido 2026-07-21 na droplet (165.227.121.20), PostgreSQL **18.4** (`/tmp/pg18data`, porta 28918),
> `pg_textsearch` 1.3.1, `shared_preload_libraries=theodb_rs,pg_textsearch`.
> Plano: `.claude/knowledge-base/plans/bm25-lexical-default-plan.md`.
> Blueprint: `.claude/knowledge-base/discoveries/blueprints/bm25-lexical-default-blueprint.md`.
> Harness: `benchmarks/run_m138_bm25_fusion.py` (fusão via o twin RRF byte-idêntico ao in-DB, ADR D2).

## Headline

**A medição em DOIS corpora NÃO autoriza trocar o default lexical para BM25 — e no corpus lexical-heavy a
troca mede como significativamente PIOR.** Em BEIR scifact a fusão com BM25 (0,7418 nDCG@10) **não vence** com
significância a fusão com `ts_rank_cd` (0,7337): **p = 0,51**, 54W/49L/197 empates. Em BEIR NFCorpus (lexical-
heavy) a fusão com BM25 (0,3795) é **significativamente pior** que com `ts_rank_cd` (0,3951): **p = 0,0115**,
67W/105L. A perna BM25 é **9,8× mais forte isolada** em scifact (0,688 vs 0,070), mas o
**RRF funde por rank e lava essa diferença** — o vetor (0,730) domina a fusão, e as duas pernas lexicais
contribuem marginalmente por cima. Por ADR-1 do plano, isto é um **honest-negative**, não um fracasso: a
medição preveniu uma troca de default que quebraria resultados de query existentes, exigiria
`shared_preload_libraries` + reinício, e embarcaria uma dependência hoje **quebrada** (issue #146) — em troca de
**zero ganho mensurável**.

A premissa do milestone — "a perna lexical shipada mede 0,0703, é defeito de produto" — é **falsificada pela
medição no eixo que importa**: o produto que ships é a **fusão** (vetor + ts_rank_cd = 0,7337), não a perna
lexical isolada. A fusão não é defeituosa; está empatada com o melhor híbrido possível (p = 0,51).

## Resultado (BEIR scifact — 5.183 docs, 300 queries, qrels binário)

| Retriever | nDCG@10 | Recall@100 |
|---|---|---|
| vetor puro | 0,729644 | 0,973333 |
| leg `ts_rank_cd` (isolado) | 0,070275 | 0,069444 |
| leg BM25 (isolado) | 0,688075 | 0,918222 |
| **fusão vetor + `ts_rank_cd`** | **0,733724** | 0,973333 |
| **fusão vetor + BM25** | **0,741846** | 0,980000 |

**Decisão (`decide_flip`, α = 0,05):** `flip = false`. mean_diff (BM25 − ts_rank_cd) = **+0,00812**;
**p_permutation = 0,5106**; wins/losses/ties = **54/49/197**; Cohen's dz = 0,038 (efeito desprezível).

### Validação cruzada do harness (não-fabricação)

Três âncoras independentes provam que os números são reais, não inventados:

1. **`hybrid_tsrank` (twin) = 0,733724 = o número in-DB do M123** (`/root/m123-scifact.json`, medido 2026-07-20,
   caminho `ai.hybrid_search_rrf` real). O twin RRF em Python reproduz a fusão in-DB **byte-a-byte** — é o que
   autoriza medir a fusão BM25 pelo twin (ADR D2), contornando o bug in-DB #146 sem perder fidelidade.
2. **vetor = 0,729644** e **leg `ts_rank_cd` = 0,070275** batem o M123/M53 exatamente.
3. **leg BM25 = 0,688075** reproduz o 0,6881 registrado no blueprint (a perna isolada do pg_textsearch).

## Por que a perna 9,8× mais forte não move a fusão

O RRF funde por **rank recíproco** (`score = Σ 1/(k+rank)`, k=60), não por score bruto. Com o vetor entregando
recall@100 de 0,973 e nDCG de 0,730, ele já coloca o documento relevante no topo na maioria das queries; a perna
lexical só altera o resultado quando **discorda** do vetor perto do topo. Trocar uma perna lexical fraca (0,070)
por uma forte (0,688) muda o rank lexical de muitos docs, mas na **fusão** isso quase sempre cai em empate
(197/300) porque o vetor já os havia ranqueado. Foi exatamente o padrão que o M123 já observara para
`ts_rank_cd` vs vetor (p=0,25) — aqui ele se repete para BM25 vs `ts_rank_cd` (p=0,51).

## Caveat de generalização (medido, não presumido)

scifact é um corpus **favorável ao vetor** (claims científicos, paráfrase — onde o lexical sofre). O M125 já
usara **NFCorpus** por ser lexical-heavy (lá o híbrido vence o vetor). Para o honest-negative ser robusto, a
mesma medição foi rodada em NFCorpus:

**NFCorpus (3.633 docs, 323 queries) — e o resultado é ainda mais forte contra a troca:**

| Retriever | nDCG@10 | Recall@100 |
|---|---|---|
| vetor puro | 0,384440 | 0,361938 |
| leg `ts_rank_cd` (isolado) | 0,207636 | 0,101940 |
| leg BM25 (isolado) | 0,325365 | 0,245781 |
| **fusão vetor + `ts_rank_cd`** | **0,395148** | 0,367412 |
| **fusão vetor + BM25** | **0,379509** | 0,362432 |

**Decisão:** `flip = false`. mean_diff (BM25 − ts_rank_cd) = **−0,01564** (BM25 **pior**); **p = 0,0115**
(significativo); wins/losses/ties = **67/105/151**; Cohen's dz = −0,14.

Aqui a perna BM25 isolada (0,325) **vence** a `ts_rank_cd` isolada (0,208) — como se esperava num corpus
lexical-heavy. Mas na **fusão** o resultado **inverte com significância**: a fusão com `ts_rank_cd` (0,395)
vence a fusão com BM25 (0,380), p = 0,0115, perdendo 105 vs ganhando 67. O RRF premia **complementaridade com
o vetor**, não força bruta da perna: a `ts_rank_cd`, mais esparsa (recall 0,102 vs 0,246), aporta um sinal mais
**diverso** ao vetor; a BM25, mais forte porém mais correlacionada com o ranqueamento do vetor, adiciona menos
diversidade à fusão. **No corpus onde o lexical mais importa, trocar para BM25 mede como significativamente
pior.** O honest-negative não é artefato do scifact ser vetor-favorável — ele se sustenta (e endurece) no
corpus lexical-heavy.

## Achado colateral (issue #146)

A fusão in-DB `ai.hybrid_search_rrf(lexical_engine => 'bm25')` está **quebrada** no `pg_textsearch` 1.3.1: o
template usa `col <@> $bind` (texto cru), que exige `to_bm25query($bind, 'índice')` de 2 args quando o operando
é bind. Nunca fora exercido (a perna BM25 sempre foi imagem throwaway; M53/M123 só mediram vetor/fts/hybrid-
ts_rank_cd). Filado como **#146**. A medição do M138 contornou o bug medindo a fusão pelo twin byte-idêntico.

## Consequência para o roadmap

- **Default lexical permanece `ts_rank_cd`** — a medição não autoriza a troca (ADR-1).
- **NÃO embarcar `pg_textsearch` na distribuição** neste milestone: sem ganho mensurável na fusão e com o leg
  in-DB quebrado (#146), embarcá-lo agora shipa complexidade (preload + reinício) e uma dependência inoperante
  por zero benefício medido — o oposto de "Esforço ≠ Complexidade".
- **Valor entregue:** o follow-up que o M53 registrou e nunca rodou — a fusão com BM25 **medida**, com
  significância, com validação cruzada — mais o bug #146 que só apareceu porque a perna foi finalmente
  exercida ponta-a-ponta.

## Reprodução

```bash
# na droplet, PG18.4 na 28918 com shared_preload_libraries=theodb_rs,pg_textsearch
set -a; source /root/m138_key.env; set +a   # OPENAI_API_KEY
export PGHOST=127.0.0.1 PGPORT=28918 PGUSER=postgres PGDATABASE=postgres
cd /root/benchmarks
python3 run_m138_bm25_fusion.py --dataset scifact  --cache-dir .cache138 --out m138-scifact.json
python3 run_m138_bm25_fusion.py --dataset nfcorpus --cache-dir .cache138 --out m138-nfcorpus.json
# gate offline: python3 -m pytest theodb_bench/test_m138_decision.py
```
