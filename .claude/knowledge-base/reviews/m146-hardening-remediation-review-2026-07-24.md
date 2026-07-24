# Review: m146-hardening-remediation

**Data:** 2026-07-24
**Base:** `b948ea7` → `HEAD` (23 commits)
**Agentes:** 6 na primeira passada (architecture, tests, wiring, cross-validation, domain-database, domain-security) + 2 na segunda (security, database/tests) sobre o estado corrigido
**Findings:** 66 (BLOCKER: 1 · HIGH: 9 · MEDIUM: 22 · LOW: 18 · INFO: 15)
**Veredito:** `READY_TO_MERGE`

---

## Por que este review foi diferente

Dois dos nove HIGH estavam em código que **eu havia declarado correto e fechado**. O review pagou-se
inteiramente nesses dois — e a segunda passada achou ainda outros dois comentários meus factualmente errados.
Isso é o registro do que aconteceu, não uma nota de rodapé: um pipeline que só confirma o autor não tem valor.

| O que eu afirmei | O que era verdade |
|---|---|
| "o fix do #172 fechou a injeção; a única superfície SQL é `recommend_ef`" | `api.rs:72` expõe uma segunda superfície que eu nunca sondei, e o terceiro eixo (`tbl`) seguia aberto |
| "`amcanreturn` nunca era atribuído e o campo não vinha zerado" | `PgBox::alloc_node` chama `alloc0()` — o campo já era NULL; a linha é no-op |
| "o literal `query dim` é asseverado por `ah_tests.rs`" | `ah_tests.rs:94,104` asseguram apenas `is_err()` |
| "panic de FFI alcançável por índice corrompido" (no #168) | 44 ensaios × 2 configurações → `deaths=0` nas duas |

---

## BLOCKER

### B1 — `SELECT count(*)` devolvia 0 num índice vetorial (#177)

Encontrado por mim ao validar os fixes, não por um agente. **Pré-existente**, sobreviveu ~120 milestones.

| Índice | `count(*)` sob `enable_seqscan=off` | Plano |
|---|---|---|
| B-tree (controle) | 500 ✅ | Bitmap Heap Scan |
| `theodb_hnsw` | **0** ❌ | Index Only Scan |

Sem erro, sem WARNING, nada no log. **Resposta errada silenciosa** — pior que crash: um `count(*)` de auditoria
devolve zero e ninguém percebe. O pgvector tem o guard há anos (`hnswscan.c:214`).

**Fechado.** Guard no `amgettuple` (não no `amrescan` — o upstream põe lá, e o re-review confirmou por
enumeração exaustiva dos callers de `index_rescan` no fonte do PG que nenhum caminho legítimo quebra).
Teste de regressão no `cassert-sql-safety`, não-vacuidade provada nos dois sentidos.

---

## HIGH — estado de cada um

| ID | Achado | Estado |
|---|---|---|
| F-sec-1 / F-test-1 | 3º eixo (`tbl`) de injeção seguia aberto | **fechado** — `resolve_relation()` via `regclass`, provado nas 2 superfícies |
| F-sec-2 | injeção de 2ª ordem em `arrow_cache` (payload executável) | **aberto** → **#176** (pré-existente, fora do escopo) |
| F-db-1 | dim errada reportada como `XX002 index_corrupted` | **fechado** — discriminador estrutural (`codebook_dim == 0`), não substring |
| F-test-2 | nenhum dos 3 fixes de injeção tinha teste | **fechado** — 5 sondas no CI; removendo o gate → exit 1 |
| F-test-3 | harness de corrupção com 3 falso-verdes | **fechado** — exit 2 INCONCLUSIVE + alvo `check-corrupt` |
| F-xval-1 | métrica do Goal não atingida | **declarada** no ROADMAP (2/3, não 3/3) |
| F-xval-2 | prova de crash do T1.3 nunca executada | **fechado** — `crash_parquet.sh` roda, com limite honesto declarado |

---

## Evidência medida (droplet PG 18.4 + pgrx 0.19)

| Prova | Resultado |
|---|---|
| Injeção, 3 eixos × 2 superfícies | `division by zero` → `42602`/`22023`; caminho honesto `ef=5` |
| BLOCKER #177 | 0 silencioso → erro tipado; B-tree 500 inalterado |
| Caminhos de manutenção sob o guard | VACUUM, VACUUM FULL, ANALYZE, REINDEX ×3, ambulkdelete, aminsert, UPDATE — todos passam |
| Taxonomia IVF-AQ | dim errada → `22023` (era `XX002`); top-k honesto = 5 |
| Corrupção de índice | HNSW: `deaths=0`, `ours=8`; IVF: `deaths=0`, 209 offsets |
| SQLSTATE de corrupção | `XX002` em `pg.rs:15`, backend `ALIVE 400` |
| Durabilidade sob crash | 90920 B, `PAR1`/`PAR1`, 0 temp órfão, 5000 linhas relidas |
| Não-regressão do `read_record_at` | 75.344.086 casos, **0** divergências |
| Mutação `out.clear()` | sobrevivia → agora morre |
| Mutantes `>`→`>=` (2) | sobreviviam → agora morrem |
| Gate de injeção removido | exit 1 nos dois eixos `tbl` |
| Arquivo Parquet truncado | exit 1 |
| `/code-quality` autoritativo | `PASS_WITH_CAVEATS` (89), D1 e D2 comprovadamente executando |

---

## Aberto, com dono

| Issue | Severidade | Por que não entrou neste milestone |
|---|---|---|
| #176 | HIGH | injeção pré-existente em `arrow_cache`; corrigi-la aqui seria escopo não planejado sobre código que este milestone não tocou |
| #173 | MEDIUM | `write_parquet` sem gate de privilégio in-function — é **decisão de design** (gate por role vs GUC de diretório), não minha para tomar sozinho |
| #174 | LOW→MEDIUM | least-privilege: 14 candidatas a `REVOKE`, que precisam ser classificadas antes de qualquer ação em bloco |
| #175 | HIGH | os falso-verdes do `/code-quality` — corrigidos, mas o issue registra o impacto retroativo: todo `PASS` anterior deste repo é vacuoso |
| #170 | — | é o M147 |

---

## Desvios do DoD, declarados

Listados no `ROADMAP.md § M146`. Os dois que importam:

1. **A métrica do Goal é 2/3, não 3/3.** T1.2 e T1.3 têm RED→GREEN discriminante; T1.1 não tem, porque a
   alcançabilidade foi **medida como inexistente** — o RED nunca falhou. O fix entrou como defense-in-depth.
2. **Bullet 4 pedia `mod tests` in-file**, entregue como `examples/` — porque um `mod tests` ali nunca
   executaria neste substrato (`cargo test` e `cargo pgrx test` não linkam). Teste que executa > teste que existe.

---

## Veredito

`READY_TO_MERGE`. Sem BLOCKER aberto; o único HIGH aberto (#176) é pré-existente, está filado com repro
executável, e corrigi-lo dentro deste milestone seria escopo não planejado sobre um arquivo que ele não toca.
