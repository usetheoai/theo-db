# Review — M147 refactor de `am/scan.rs` (version-dispatch)

**Data:** 2026-07-24 · **Slug:** m147-scan-version-dispatch · **Branch:** develop · **Milestone:** M147
**Plano:** `.claude/knowledge-base/plans/m147-scan-version-dispatch-plan.md`
**Implementação:** `.claude/knowledge-base/implementations/m147-scan-version-dispatch-implementation.md`
**Code-quality:** `.claude/knowledge-base/audits/m147-scan-version-dispatch-code-quality-2026-07-24.md` (PASS_WITH_CAVEATS 89)

## Verdict: **READY_TO_MERGE**

Sem BLOCKER, sem HIGH. 13 findings totais — **todas INFO/LOW**. A única acionável (M147-DB-3, INFO) foi
**corrigida neste ciclo**. Os 3 agentes especialistas convergem: o refactor é comportamentalmente idêntico
onde o A/B mede *e* onde ele não mede.

## Domínio detectado

Concorrência + database (Index AM sobre PostgreSQL, pgrx 0.19, Rust unsafe/FFI). Agentes despachados:

| Agente | Lente | Veredito | Findings |
|---|---|---|---|
| domain-database | Index AM API + storage + crash-safety + ADR-2 | PASS | 5 INFO |
| wiring-pgrx | Rust/pgrx: unsafe, FFI, unwind, borrow, closures | SOUND | 6 INFO |
| cross-validation | Plano ↔ implementação ↔ testes ↔ git | READY_TO_MERGE | 2 LOW |

## Matriz de severidade

| Severidade | Qtde | Ação |
|---|---|---|
| BLOCKER | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | documentadas (já tratadas honestamente na impl) |
| INFO | 11 | 1 corrigida (M147-DB-3), 10 confirmações positivas |

## O que cada agente provou (não só o A/B)

### domain-database — ADR-2 intacta, sem regressão v2/VACUUM

- **ADR-2 (o mais crítico) — PASS.** O `stage1_score_blocks` só computa `base = codes_off + b*pairs*32`;
  nunca deriva `codes_off`. As 5 fórmulas on-disk (v4 `8n+entry_f32·n`, v5/v6/v8 `8n`, v7 `8n+n·label_bytes`)
  ficaram nos corpos como linhas de contexto **não-modificadas** no diff. O layout on-disk não vazou para o
  kernel — a fronteira que a ADR-2 do M145 protege está intacta.
- **Closures vs. `for` original — PASS.** `on_candidate` é chamado uma vez por candidato; um `return` na
  closure `FnMut` pula **só aquele candidato** = exatamente o que o `continue` do loop interno fazia. O `break`
  (página mais curta que o diretório) permaneceu no kernel, não na closure. Ordem de filtros do v7 idêntica
  (label → tid → membership → push).
- **`map_ivf_version` estrito — PASS, sem regressão v2.** `2 | 3 => V3` → mesmo `read_ivf_meta` que aceita
  `ver ∈ {2,3}`. Um índice v2 legado segue o caminho idêntico ao pré-refactor.
- **VACUUM `matches!(…, Ok(V4|V5|V6|V7|V8))` — PASS.** Conjunto idêntico ao OR de 5 predicados; MVCC/locks/pending
  intocados.

### wiring-pgrx — unwind sound, zero unsafe novo

- **`ScanError::raise` / unwind — SOUND.** Todos os `.unwrap_or_else(|e| e.raise())` (scan.rs:293,346,409,1300,1325)
  são capturados pelo MESMO `#[pg_guard] extern "C-unwind"` que já capturava o panic pré-M147 (`amrescan:205`,
  `amgettuple:1162`). Provado por `awk` que não há `fn` aninhada nem closure passada a C entre os call-sites.
  **Nenhum frame C interposto entre o panic e o guard.** O refactor moveu *onde* o `err_*` é chamado, nunca o
  *tipo* de unwind nem o frame que o captura.
- **`?` vs. buffer leak — SOUND.** `read_page_item_into` copia (`extend_from_slice`) ANTES de
  `UnlockReleaseBuffer`; o único caminho de Err retorna antes do `ReadBufferExtended`. Nenhum `?` atravessa um
  Buffer pinado.
- **Kernel — SOUND.** `fn` seguro; `bytes[base..base+pairs*32]` guardado por `if bytes.len() < base+pairs*32 { break }`;
  `try_into().unwrap()` de tid nunca panicam (só chegam com o guard passado). Página truncada ⇒ zero candidatos,
  fail-safe.
- **unsafe novo — NENHUM.** grep das linhas adicionadas por `unsafe|transmute|from_raw|set_len|MemoryContext` = vazio.

### cross-validation — 5 bullets do DoD cumpridos, números reconciliam com o git

| Bullet | Status | Evidência |
|---|---|---|
| 1 — if-ladder → enum (OCP) | MET | `fn ivf_is_v` = 0; dispatch `match` exaustivo |
| 2 — 8 gathers → Result+? UM boundary | MET | baseline 55 → HEAD 9 arms C-style |
| 3 — Stage-1 compartilhado (ADR-2) | MET | `for b in 0..nblocks` 5→1; corpos de decode intocados |
| 4 — A/B byte-idêntico | MET | 6 caminhos (v3..v8, superset do ROADMAP); AB_COMPARE_OK |
| 5 — zero SQL surface + CHANGELOG | MET | 0 `pg_extern` no diff; 6 entradas #170 |

Números: scan.rs 1567→1400 (−167), QPS ~381 vs ~377 (−1%), 55→9 arms — todos batem.

## Findings acionáveis

### M147-DB-3 (INFO) — CORRIGIDA neste ciclo
A mensagem para um discriminante TIVS desconhecido trocou de "unsupported… — REINDEX to upgrade" para
"unknown format version N". A **classe** (XX002) sempre foi preservada, mas o pré-refactor dava ao operador a
pista de remediação (REINDEX). **Fix aplicado:** `map_ivf_version` (ivf.rs:49) + a cópia do example voltam a
carregar "unsupported structured format vN — REINDEX to upgrade to a supported generation". Diagnosticabilidade
(dimensão operacional) restaurada; taxonomia inalterada.

### CV-1 (LOW) — já tratada honestamente
O Baseline Context do plano afirmava "grep confirmado" para `ivf_is_v*` mas `build.rs` tinha 12 refs (incl. o
caminho de produção do VACUUM). A implementação **pegou e declarou** o desvio (impl log "Escopo além do Baseline
Context") e migrou todos os sítios — sem scope creep silencioso. Honestidade (Rule 3) intacta. Sem ação de código.

### CV-2 (LOW) — nota de dataset, sem ação
O dataset (2000 vecs dim-8) é pequeno para os quantizadores divergirem *cross-version*, mas o contrato real
(novo==baseline por-caminho) + a não-vacuidade (mutar `codes_off`→FAIL) exercitam o kernel Stage-1 de verdade.

## Gates de merge (BLOCKER-level) — todos verdes

- ✅ Testes verdes na branch (cassert-smoke + A/B in-PG).
- ✅ Sem secrets commitados.
- ✅ Sem commit direto em `main`.
- ✅ Sem trailer `Co-Authored-By`.
- ✅ CHANGELOG `[Unreleased]` atualizado (Rule 6).

## Trilha de auditoria

- Findings por agente: `.claude/agents/review-m147-scan-version-dispatch-2026-07-24/findings/{domain-database,wiring-pgrx,cross-validation}.json`

**Verdict:** READY_TO_MERGE
