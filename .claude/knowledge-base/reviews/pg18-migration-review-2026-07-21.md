---
slug: pg18-migration
milestone_id: M135
date: 2026-07-21
reviewers: council-index-storage
---

# Review — M135 / migração PostgreSQL 17 → 18

**Verdict:** READY_TO_MERGE

## Severidade

| Sev | Achado | Estado |
|---|---|---|
| — | `materialize_bitmap`: iterador privado, `tbm_end_iterate` uma vez, clamp, `MAX_TUPLES_PER_PAGE=291`, tipo `u16`, aliasing, ramo lossy | **Correto em todos os pontos**, verificado contra `tidbitmap.{h,c}`, `heapam_handler.c` e um `_Static_assert` compilado |
| — | Fronteira de unwind via `pgrx_extern_c_guard` | **Sólida** — corresponde ao contrato documentado do pgrx (`panic.rs:415-424`); nenhum valor com `Drop` vivo no frame |
| — | `tupdesc_attr` | **Sólido** — pgrx devolve referência ao array real de 104 B, não a um temporário; `Drop` no-op confirmado |
| — | `CompareType::COMPARE_LT` | **Constante correta**; fail-closed em falha de lookup |
| LOW | Comentário obsoleto em `columnar.rs` afirmando que os stubs **não** deviam ser guardados — a crença exata que causou o #143 | **CORRIGIDO** — substituído por nota de histórico |
| LOW | Minha justificativa em `columnar_agg.rs` era **factualmente falsa**: `COMPARE_LT == 1 == BTLessStrategyNumber`, os valores coincidem por design | **CORRIGIDO** — reescrita como port de tipo, não de valor |
| INFO | Pré-inicialização com o valor de ACEITE em vez do de rejeição | **CORRIGIDO** — agora `COMPARE_INVALID` (fail-closed) |

Bônus verificado pelo revisor: `plancat.c:331` deriva a capacidade de bitmap de `scan_bitmap_next_tuple != NULL`,
confirmando que deixar NULL faz o planner rotear ao redor (ADR-2). Ambos os stubs foram deletados por inteiro —
sem `dead_code` órfão para o `/code-quality`.

## Gates

| Gate | Estado |
|---|---|
| Compila no PG18 | OK — 0 erros |
| Suítes de crash + isolamento no 18 | OK — 3/3 isolamento; 3 provas de crash |
| Sem segredos commitados | OK |
| Trailer de co-autoria ausente nos commits | OK (política do projeto) |
| CHANGELOG atualizado | OK — 2 entradas BREAKING |
| Trabalho em `develop` | OK |

## Lacuna carregada para o release (não fechada)

`cargo pgrx test` não linka nesta droplet, então `m92_v1b_materialize_bitmap_exact` — o oráculo unitário direto do
porte do bitmap — **existe no código e nunca executou**. A cobertura vem do A/B in-PG, consistente com execução
correta mas sem prova por instrumentação de que o caminho foi tomado. Registrado no artefato de evidência.

O revisor também não conseguiu provar exaustivamente que o planner nunca remove o `Filter` escalar do filho
vetorial — a premissa que torna seguro ignorar `res.recheck`. É design pré-existente (M92/M93), não delta do M135.
