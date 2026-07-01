# ADR 0010 — M26 vector index AM: scope decisions (l2-first, blob persistence, fixed params)

**Status:** Accepted · **Data:** 2026-07-01 · **Owner:** paulohenriquevn
**Relacionado:** `.claude/knowledge-base/plans/m26-vector-index-am-plan.md` (ADR-1/2/3), `docs/benchmarks/m26-index-am.md`,
`.claude/knowledge-base/discoveries/blueprints/m21-own-ann-index-blueprint.md`

## Contexto

M26 promoveu o ANN in-memory (rebuild-por-query) a **index AMs Postgres persistidos** (`theodb_ivfflat` +
`theodb_hnsw`): registro do `IndexAmRoutine`, persistência em páginas WAL-logged (GenericXLog), pushdown do planner
(`ORDER BY <-> LIMIT k` → Index Scan), e manutenção incremental (aminsert pending-buffer + VACUUM fold). Tudo
validado end-to-end (`test_index_am.py` 6/6; coexistência M20–M22 61 verdes; benchmark 16× vs rebuild-per-query).

Três decisões de escopo emergiram na implementação (measurement-first, honestidade — Regra 3). Este ADR as registra.

## Decisão

### D1 — Operator class **l2 primeiro**; cosine/ip são follow-up

Ambos os AMs registram o opclass `theodb_{ivfflat,hnsw}_l2_ops` (DEFAULT). A métrica é **gravada no blob
persistido** (o scan a lê de volta), mas resolver a métrica a partir do **opclass no momento do build** exige ler o
nome da opfamily via catálogo — e o **pgrx 0.16 não expõe `get_opfamily_name`** (nem um equivalente limpo). Em vez
de um lookup de syscache frágil (GETSTRUCT manual) sob o callback FFI de build, `ambuild` fixa `Metric::L2`.

**Alternativas rejeitadas:** (a) syscache `SearchSysCache1(CLAOID/OPFAMILYOID)` + GETSTRUCT manual — FFI frágil,
risco alto sob o build callback, ganho pequeno agora; (b) SPI em `ambuild` — evitar SPI em callbacks low-level.
**Follow-up:** quando a resolução opclass→métrica for wired (helper de catálogo testado), adicionar
`theodb_{ivfflat,hnsw}_{cosine,ip}_ops` — o `Persisted`/scan já são métrica-agnósticos (leem do blob).

### D2 — Persistência **blob-por-scan** (MVP measurement-first); scan O(N) é limitação conhecida

`ambuild` serializa o índice inteiro num blob e o grava em páginas; `amrescan` **deserializa o blob inteiro por
scan** (plan ADR-1). Isso é correto e **16× mais rápido que o rebuild-per-query** (a baseline do DoD — 86ms vs
1372ms), mas é **O(N) por scan** — mais lento que um seq scan não-rebuilding em N pequeno/médio (benchmark § 3.1).

**Aceito para o MVP** (measurement-first: correção + persistência + pushdown + manutenção primeiro, com número
honesto). **Otimização (follow-up):** ler só as páginas necessárias por scan (centroide + listas probed) e/ou
cache do índice deserializado por relação — transforma o scan em O(probes·list_size), ponto em que o AM também
vence o seq scan. Documentado, não mascarado (Regra 3).

### D3 — Parâmetros de build **fixos** (sem reloptions ainda)

`ambuild` usa `lists=100` (IVFFlat) / `m=16, ef_construction=64` (HNSW), espelhando os defaults da função
SQL-callable. Reloptions (`WITH (lists=…)`) são um follow-up (`amoptions=None` hoje). Não bloqueia nenhum DoD (o
scan usa `probes`/`ef_search` fixos sensatos); é tuning.

## Consequências

- **Positivas:** DoD do M26 cumprido e validado — AMs persistidos (ivf+hnsw), pushdown, manutenção incremental,
  benchmark reproduzível (16× vs rebuild), coexistência. Camada de página/scan/vacuum compartilhada e
  métrica-agnóstica (pronta para D1/D3 follow-ups sem re-arquitetura).
- **Negativas (aceitas, documentadas):** l2-only por ora; scan O(N) até a otimização de leitura parcial; params
  fixos. Nenhuma é um workaround silencioso — cada uma tem número/caminho honesto no benchmark doc + aqui.

## Quando muda

Follow-ups (cosine/ip, leitura parcial de páginas, reloptions) entram como slices próprios com benchmark, cada um
via CHANGELOG (e este ADR referenciado). Nenhum re-arquiteta a camada — são extensões.
