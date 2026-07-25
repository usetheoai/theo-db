# Blueprint — M150: Chunk-group filtering no scan geral (theodb_columnar)

**Data:** 2026-07-25 · **Fonte:** discover (council-index-storage) + evidência primária Citus + código real verificado.

## Veredito: ~70% da máquina JÁ EXISTE. O M150 é wiring, não greenfield.

O teste de exclusão puro (`chunk_can_match`), a extração `Var op Const` (`extract_zone_predicate`), o min/max
por-chunk (`ChunkDirEntry`) e o side-channel keyed-por-scandesc (M149) **já existem e estão testados** — mas o
skip está ligado só ao caminho de **agregação** (`decode_columns`). O M150 leva a mesma poda ao caminho de
**scan geral** (`decode_stripe`/`load_next_batch`), usando o CustomScan de projeção do M149 (que já vê `plan.qual`).

## Coverage Corner 1 — Integration Tests

Como testar o skip sem perder linha: **twin heap idêntico** (padrão `seed()` de `columnar_project.rs:503`) com
WHERE variados → resultado colunar `==` heap (A/B byte-idêntico). Cobrir: Eq dentro/fora do range do chunk, range
`<`/`>`/BETWEEN, negativos, temporais (timestamp/date), float-NaN, coluna sem min/max (fallback), predicado
não-empurrável (OR/função → sem skip, ExecScan re-checa), self-join com WHERE por-lado (herança M149). Métrica
`chunks_skipped/scanned` sob `THEODB_SCAN_PROFILE=1` prova que o diretório é consumido (0 com preds vazio).

## Coverage Corner 2 — Dependencies

**Nenhuma nova.** Reusa `zonemap::chunk_can_match` (zonemap.rs:36), `ZonePredicate`/`ZoneOp` (zonemap.rs:16,26 —
já pub(crate)), `ChunkDirEntry.{has_minmax,min_bits,max_bits}` (columnar_codec.rs:108), `compute_minmax`,
`minmax_kind_of`. Rule 9 puro: consumidor do que M105/M149 já escreveram.

## Coverage Corner 3 — Tools

`THEODB_SCAN_PROFILE=1` (env já usado em `decode_columns:855-859`) — espelhar o contador `chunks_skipped/total`
no scan geral. `EXPLAIN (ANALYZE)` A/B OFF-vs-ON (GUC novo `theodb.enable_chunk_skip`, default ON) como oráculo
do ganho. ClickBench `run_m128` (diverged=0) como oráculo de correção.

## Coverage Corner 4 — Techniques

**Prior art Citus** (`.claude/knowledge-base/references/citus/src/backend/columnar/`): `ExtractPushdownClause`
(`columnar_customscan.c:760`) — best-effort recursivo sobre AND/OR: de um AND pega os args empurráveis e **ignora
os não-empurráveis** (`:808`); `SelectedChunkMask` (`columnar_reader.c:1132`) — por chunk pula se `!hasMinMax`
(`:1167`) e testa refutação, contando `chunkGroupsFiltered`. **Diferença de design (KISS):** o Citus usa o
theorem-prover do PG (`predicate_refuted_by`); o TheoDB faz o teste direto das 5 estratégias btree em
`chunk_can_match` — mais simples e suficiente para `col op const`. NÃO copiar `predicate_refuted_by`.

## Mecanismo (espelha 1:1 o M149, trocando "wanted columns" por "pushable predicates")

1. **Extração best-effort** (não all-or-nothing como o agg): em `begin_custom_scan`, sobre `(*plan).qual`,
   `filter_map(|c| extract_zone_predicate(c, scanrelid))` — descarta os não-empurráveis (o ExecScan re-checa o
   WHERE completo). Promover `extract_zone_predicate`/`flip_op`/`encode_const_bits` de columnar_agg.rs a
   `pub(crate)` (DRY — hoje duplicáveis).
2. **Side-channel** — `SCAN_PREDICATES` paralelo ao `SCAN_PROJECTION` (mesma keying por scandesc, mesmo
   `ActiveGuard`/registry, mesma limpeza xact/subxact). **Toda a disciplina de correção do M149 (nested-scan,
   ABA-após-subxact-abort) é herdada** — os preds vivem no mesmo idioma.
3. **Skip no loop de chunks** — `decode_stripe` ganha `predicates: &[ZonePredicate]`; no `for cg`
   (columnar.rs:723), antes de descomprimir (`:732`), o guard idêntico ao de `decode_columns:825-841`: se algum
   pred prova exclusão (`!chunk_can_match(...)`), `chunks_skipped += 1; continue` (pula read_chunked+zstd de
   TODAS as colunas do chunk-group → alinhamento preservado). `load_next_batch` busca via `scan_predicates(st)`.
4. **Fallback** — preds vazio reproduz o caminho pré-M150 byte-a-byte; pending rows nunca pulados.

## Invariantes (evolution-gate)

- **Nunca perder linha (A/B byte-idêntico obrigatório).** O skip é filtro de ADMISSÃO: o `ExecScan` re-checa o
  `qual` completo (`columnar_project.rs:471`). **Sobre-admitir** (não pular um chunk podável) = só perda de perf;
  **pular um chunk que PODE ter match = BUG.** A corretude está em `chunk_can_match` retornar `false` **apenas**
  quando PROVA exclusão — fail-safe por construção (`has_minmax==false`/`kind==None`/NaN → `true`).
- **Formato de página NÃO muda** — puro consumer do `min_bits/max_bits` do M105. Sem magic bump, sem REINDEX.
- **MVCC/pending intactos** — stripes visíveis resolvidos no scan_begin sob snapshot; pending nunca pulado.

## Riscos (ordem)

1. **Corretude do skip (único BLOCKER real)** — `encode_const_bits` divergir de `compute_minmax` num tipo. Mitig.:
   reusar o extrator existente (não reimplementar) + A/B heap-twin obrigatório com Eq/range dentro-e-fora.
2. **Perf-theater** — sem clustering pela coluna do predicado, os bounds por-chunk sobrepõem e nada é pulado.
   Honestidade: medir `chunks_skipped/scanned` num dataset clusterizado antes de qualquer claim (Regra 5).
3. **Interação M149 no mesmo nó** — preds + wanted no mesmo side-channel; keying por scandesc já resolve (prova
   do `test_nested_self_join_projection`); estender o teste para self-join com WHERE por-lado.

## ADRs

- **ADR-1: teste direto (chunk_can_match) vs theorem-prover (Citus).** Escolhido o teste direto das 5 estratégias
  btree — cobre `col op const` (o alvo), KISS, sem dependência do `predicate_refuted_by`. Alternativa rejeitada:
  portar `predicate_refuted_by` (mais geral p/ OR composto, mas complexidade acidental — YAGNI hoje).
- **ADR-2: best-effort (Citus) vs all-or-nothing (agg).** No scan geral o ExecScan re-checa → empurrar o subset
  empurrável e ignorar o resto. Diferente do `extract_all_predicates` do agg (que substitui o WHERE, logo exige
  tudo-ou-nada). Alternativa rejeitada: exigir todos empurráveis (perderia o skip em `a=X AND lower(b)=Y`).
- **ADR-3: side-channel paralelo (SCAN_PREDICATES) vs estender a tupla do SCAN_PROJECTION.** Paralelo mantém o
  M149 intocado (menor blast radius); mesma keying/limpeza. Alternativa rejeitada: mudar a tupla existente (toca
  o caminho de projeção já released/estável).

## Delta (existe vs escrever)

| Peça | Estado |
|---|---|
| `chunk_can_match` + `ZonePredicate`/`ZoneOp` (zonemap.rs) | **Existe, testado, reusar as-is** |
| `ChunkDirEntry.{min,max,has_minmax}` + `compute_minmax` + `minmax_kind_of` | **Existe** |
| Skip loop pattern (`decode_columns:825-841`) | **Existe** (só no caminho agg) |
| `extract_zone_predicate`/`flip_op`/`encode_const_bits` (columnar_agg.rs) | **Existe** — promover a pub(crate) |
| Side-channel scandesc-keyed + ActiveGuard + xact cleanup (columnar_project.rs) | **Existe (M149)** — irmão SCAN_PREDICATES |
| `decode_stripe` receber `predicates` + guard de skip | **Escrever** (~15 linhas, portar de decode_columns) |
| `load_next_batch` buscar preds + repassar | **Escrever** (~5 linhas) |
| `begin/exec_custom_scan` extrair preds best-effort + instalar | **Escrever** (~30 linhas) |
| Métrico `chunks_skipped/scanned` no scan geral | **Escrever** (espelhar decode_columns:855) |

## Referências
- `.claude/knowledge-base/references/citus/src/backend/columnar/columnar_reader.c` (`SelectedChunkMask:1132`)
- `.claude/knowledge-base/references/citus/src/backend/columnar/columnar_customscan.c` (`ExtractPushdownClause:760`)
- theodb: `theodb_rs/src/am/zonemap.rs:36`, `columnar_agg.rs:146`, `columnar.rs:698,775,1045`, `columnar_codec.rs:108`
- [[m149-projection-pushdown-released]] (o side-channel + CustomScan reusados), M105 (zone-map `directory_minmax`)
