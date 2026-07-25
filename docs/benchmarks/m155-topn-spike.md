# M155 — Spike Top-N: hipótese REFUTADA por medição (PG já usa top-N heapsort)

**Data:** 2026-07-25 · **Milestone:** M155 (spike measurement-first) · **Box:** droplet DO `c-8`, efêmero, destruído.
**Método:** EXPLAIN ANALYZE (TIMING ON) das queries `ORDER BY … LIMIT` do ClickBench sobre o binário v0.146.0,
isolando o custo do nó `Sort` acima do CustomScan colunar. **Artefatos:** `docs/benchmarks/m155-artifacts/{m155_spike_explain.txt, m155_base_coverage.json}`.

## TL;DR — o spike corrige a hipótese (como o M152/M148)

> **A premissa do M155 ("rotear ao TopK para evitar o sort completo") está ERRADA: o PostgreSQL já usa `Sort Method:
> top-N heapsort`** — um heap O(n log k), exatamente o algoritmo do `TopK` do DataFusion. Não há "sort completo a
> evitar". O nó `Sort` custa **~2-4ms** (não é gargalo); o custo dominante é a **materialização row-by-row do
> CustomScan (o gargalo do M148, ~150ms para 13k linhas)**, que o TopK NÃO elimina (a chave de ordenação precisa ser
> decodificada para TODAS as linhas para decidir o top-k). E a cobertura marginal é **ZERO** (as Top-N já roteiam).

## Medido (EXPLAIN ANALYZE, head 100k)

Sort node ms = delta incremental (fim-do-Sort − fim-do-CustomScan), o custo do Sort SOBRE o scan. Rótulo `Sort Method`
= **verbatim do artefato** `m155_spike_explain.txt` (nada arrumado — inclui o q25 cru `still in progress`):

| Query | Forma | Sort ms (delta) | Sort Method (verbatim) | CustomScan (actual) | Total |
|---|---|---|---|---|---|
| q24 | `SearchPhrase WHERE <> '' ORDER BY EventTime LIMIT 10` | ~1,9ms (153,4→155,4) | **top-N heapsort** | 104,8→153,4ms (13005 rows) | 156,2ms |
| q25 | `… ORDER BY SearchPhrase LIMIT 10` | ~1,6ms (137,9→139,5) | `still in progress  Memory: 0kB` (artefato l.22)† | 92,7→137,9ms (13005 rows) | 140,3ms |
| q26 | `… ORDER BY EventTime, SearchPhrase LIMIT 10` | ~1,9ms (145,3→147,1) | **top-N heapsort** | 98,1→145,3ms (13005 rows) | 147,8ms |
| q33 | `URL, COUNT(*) GROUP BY URL ORDER BY c DESC LIMIT 10` | ~2,6ms (1,4→4,0) | **top-N heapsort** | 0,003→1,4ms (18180 groups) | 65,3ms |
| q23 | `SELECT * WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10` | (0 rows casaram) | `quicksort` (0 rows) | 1376,6ms (scan/LIKE, 0 rows) | 1377,4ms |

† **q25 é fiel ao artefato, não ao argumento:** o artefato reporta `still in progress  Memory: 0kB` (não `top-N
heapsort`). A conclusão "o Sort não é gargalo" NÃO depende do rótulo — o custo incremental do Sort é ~1,6ms de fato
(139,546−137,926), e um sort COMPLETO de 13005 linhas de texto reportaria `quicksort  Memory: NNMB` (como o q23) e
custaria muito mais que 1,6ms. Os outros três (q24/q26/q33) confirmam `top-N heapsort` literalmente. Nenhum número foi
arrumado para caber na história.

`columnar_customscan_count = 21` (as Top-N já roteiam o CustomScan de projeção/agg; o Sort+Limit fica no PG acima).

## Por que rotear ao TopK NÃO vale (3 razões medidas/estruturais)

1. **PG já é O(n log k).** `Sort Method: top-N heapsort` é o MESMO algoritmo do `TopK` do DataFusion. O DoD do M155
   ("evitando o sort completo") mira um custo que não existe — não há sort completo, há um heap bounded de ~2-4ms.
2. **O gargalo real é a materialização do scan (M148), que o TopK não fecha.** O TopK ordena por uma chave que precisa
   ser decodificada para todas as linhas; ele não pode pular linhas que não viu. Empurrar o TopK para dentro do scan
   economizaria no máximo a materialização das COLUNAS DE SAÍDA das linhas descartadas — o que é a otimização
   **"materialização preguiçosa de colunas de saída"**, diferente de "rotear ao TopK", e o único regime onde ganharia
   (SELECT * largo, q23) teve 0 linhas casadas aqui (não-medível).
3. **Cobertura marginal = 0.** Confirmado (M152 + este spike): nenhuma query do ClickBench declina por Sort/Limit; as
   Top-N já roteiam. Rotear ao TopK não move `columnar_customscan_count`.

Bônus (correção): **byte-identidade do top-k com empates é mal-definida** — `ORDER BY EventTime LIMIT 10` com empates na
fronteira: o PG (heapsort instável) escolhe arbitrariamente quais 10 sobrevivem; um tie-breaker total tornaria o NOSSO
resultado determinístico mas não necessariamente igual à escolha arbitrária do PG. O oráculo do `run_m128` (remove LIMIT
+ canonicaliza) já prova a igualdade do CONJUNTO — a única invariante bem-definida.

## Veredito honesto (esforço ≠ complexidade)

Rotear ao TopK seria **complexidade sem valor**: um operador novo de alto risco de correção para economizar ~2-4ms em
queries que já roteiam, mirando um "sort completo" que o PG não faz. Por CLAUDE.md (measurement-first, anti-sunk-cost,
esforço≠complexidade) e o padrão do North Star ("medir → reconhecer o limite → reposicionar"), o veredito é
**HONEST-NEGATIVE: não implementar o roteamento-ao-TopK**.

**O lever real que o spike aponta** (para um milestone futuro, se priorizado): **materialização preguiçosa de colunas de
saída** no CustomScan de projeção — decodificar só a chave de ordenação para todas as linhas, materializar as demais
colunas apenas para o top-k. Ataca diretamente o gargalo M148 no regime `SELECT * … ORDER BY key LIMIT k` (wide top-N).
É uma otimização distinta e maior, com sua própria superfície de correção — candidata a M156/M157, não ao escopo atual do M155.

## Reprodução

```bash
# v0.146.0; SET theodb.enable_columnar_agg=on
EXPLAIN (ANALYZE, TIMING ON, COSTS OFF) SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
# → Sort Method: top-N heapsort; Sort ~2ms; Custom Scan (theodb_columnar_project) ~150ms (o gargalo é o scan, não o sort)
```
