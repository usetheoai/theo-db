# M152 — Routing-map: por que cada query do ClickBench não vetoriza (medido)

**Data:** 2026-07-25 · **Milestone:** M152 (spike measurement-first) · **Box:** droplet DO `c-8`, efêmero, destruído.
**Método:** instrumentação `admit_trace` (19 pontos, `THEODB_ADMIT_TRACE=1`, behavior-neutral) → EXPLAIN de cada uma
das 43 queries do ClickBench, capturando a razão real de declínio do CustomScan de agregação.
**Artefatos:** `docs/benchmarks/m152-artifacts/{m152_trace.json, m152_agg.json}`.

## TL;DR — o spike corrigiu a hipótese (como o M148)

> **Behavior-neutral confirmado:** com o trace off, `columnar_customscan_count = 14`, `diverged = 0` (idêntico ao
> M151). **Todas as 29 não-roteadas têm razão medida (zero gaps).** E o achado reordena o roadmap: **GROUP BY texto
> NÃO é o lever** (o group-key texto já é aceito — o `arrow_supported_group_type` inclui 25/1042/1043); o que declina
> as queries GROUP-BY-texto é o **`ORDER BY count(*)` sobre o agregado no path AGG_SORTED** (collation) + o **text
> `<>` no WHERE**. E os bloqueios são **compostos**: a cobertura marginal de cada fatia isolada é pequena (2-4).

## Distribuição das razões (first-blocker medido, das 29 não-roteadas)

| Razão (trace) | Queries | # | Fatia planejada? |
|---|---|---|---|
| `unpushable_where_qual` | q12,14,20,30,31,36,37 (+q27 compound) | **8** | text `<>`/LIKE no WHERE — era o follow-up ADR-4 do M151, NÃO uma fatia M153-M155 |
| `agg_distinct_filter_order` | q4,5,8,9,10,11,13 | **7** | COUNT(DISTINCT) — M154 |
| `target_grouping_expression_or_other` | q18,34,35,39,42 | **5** | expr no target/GROUP BY (`date_trunc`, `CASE`, `ClientIP-1`, `GROUP BY 1`) — não planejado |
| `swap_sorted_text_group_collation` | q16,17,33,38 | **4** | GROUP BY texto + ORDER BY → AGG_SORTED → texto declina por collation — o REAL blocker do "M153" |
| `minmax_over_unordered_text` | q21,22 | **2** | `MIN(URL)`/`MAX(texto)` — não planejado |
| `grouping_sets_having_distinct_window` | q27,28 | **2** | HAVING — não planejado |
| `agg_over_expression` | q29 | **1** | `SUM(x+1)` — não planejado |

Total = 8+7+5+4+2+2+1 = 29 ✓ (consistente: as 14 roteadas emitem 0 razões de declínio).

## Cobertura marginal por fatia (o número que reordena o roadmap)

**Crítico:** o first-blocker é o que dispara PRIMEIRO; a maioria das queries tem bloqueios COMPOSTOS. Uma query só
roteia quando TODOS os seus bloqueios caem. A cobertura marginal de uma fatia = queries cujo conjunto INTEIRO de
bloqueios é fechado por ela (medido pelo first-blocker + análise de compostos no SQL):

| Fatia | first-blocker | Cobertura marginal (queries que realmente roteiam) | Compostos que sobram |
|---|---|---|---|
| **COUNT(DISTINCT)** (M154) | 7 | **~2** (q4, q5 — puros, sem GROUP BY/WHERE) | q8/10/11/13 têm GROUP BY texto/int + ORDER-BY (deparse); q22 tem MIN(URL); q9 tem ORDER-BY-agg |
| **GROUP BY texto sorted** (M153) | 4 | **~3** (q16, q17, q33 — se o deparse/ORDER-BY for tratado junto) | q38 tem text `<>` no WHERE |
| **text `<>` no WHERE** (follow-up M151) | 8 | **~4** (q30, q31, q36, q37 — GROUP BY int + text `<>`) | q12/q14 têm GROUP BY texto também; q20/q28 são LIKE/regex (não roteáveis) |

**Veredito honesto (o spike fez seu trabalho):**
1. **Nenhuma fatia isolada roteia muitas queries** — o modelo "cada fatia adiciona K" do blueprint era **otimista**.
   Os bloqueios compõem: q13 (`SearchPhrase, COUNT(DISTINCT UserID) WHERE SearchPhrase<>'' GROUP BY SearchPhrase`)
   tem TRÊS bloqueios (distinct + text-`<>`-WHERE + GROUP-BY-texto) — nenhuma fatia sozinha a roteia.
2. **GROUP BY texto (M153) NÃO é "aceitar o group-key texto"** (já aceito) — é tratar o **AGG_SORTED texto por
   collation** (o planner escolhe GroupAgg sorted p/ `ORDER BY count LIMIT`) + o **deparse ORDER-BY-sobre-agregado**.
   É mais complexo do que o milestone assumia; e o risco é a collation não-determinística (o blueprint acertou).
3. **O `ORDER BY count(*) DESC LIMIT` é um bloqueio TRANSVERSAL** — atinge GROUP-BY-texto (via AGG_SORTED),
   COUNT(DISTINCT) (q8) e text-`<>` (q30). Tratar o ORDER-BY-sobre-agregado destrava fração de VÁRIAS classes.
4. **A maior fatia limpa de ganho é `text <> no WHERE` (~4) + COUNT(DISTINCT) (~2)** — não o GROUP BY texto (~3, e
   caro). Isso **reordena**: o follow-up text-`<>` do M151 (ADR-4) e o COUNT(DISTINCT) sobem; o GROUP BY texto puro
   desce e absorve o tratamento de collation + deparse.

## Reordenação recomendada de M153-M155 (a anotar no ROADMAP)

Dado o medido, a ordem de maior ganho×menor risco muda:

1. **M153 → COUNT(DISTINCT) exato** (era M154): ~2 puras limpas + destrava parte dos compostos; parity-clean; o
   menor risco A/B. Melhor primeira fatia medida.
2. **M154 → text `<>`/`=` no WHERE** (era o follow-up ADR-4 do M151): ~4 (GROUP BY int + text-`<>`); precisa da
   serialização de const-texto no `custom_private` (a fatia que o M151 marcou honest-negative). Segundo maior ganho.
3. **M155 → GROUP BY texto + ORDER-BY/deparse** (era M153): ~3, mas exige o tratamento de collation determinística
   (guard) + o ORDER-BY-sobre-agregado (deparse). Maior risco (collation) e mais estrutural → por último.

LIKE/regex (q20,q28), `MIN(URL)` (q21,q22), HAVING (q27,q28), expr-group/target (q18,34,35,39,42) permanecem
honest-negative (RE2≠POSIX, min/max-texto por collation, complexidade) — fora do escopo M153-M155.

## Metodologia / reprodução

```bash
# behavior-neutral (flag off): run_m128 --agg → columnar_customscan_count=14, diverged=0
# trace (flag no ambiente do POSTMASTER, não do cliente): THEODB_ADMIT_TRACE=1 pg_ctl start; então EXPLAIN cada query
THEODB_ADMIT_TRACE=1 pg_ctl -D $PGDATA start
python3 m152_trace.py queries.sql   # captura 'theodb_admit_decline: <razão>' por query (WARNING no client)
```

Nota (bug pego e corrigido durante o spike): `THEODB_ADMIT_TRACE` é lido pelo BACKEND (`admit` roda lá), então o
env var precisa estar no ambiente do **postmaster** (os backends herdam), não do cliente psql/python.
