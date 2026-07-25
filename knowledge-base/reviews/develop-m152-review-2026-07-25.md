# Review — M152 routing-map spike (develop)

**Data:** 2026-07-25 · **Verdict:** READY_TO_MERGE

## Método
Spike measurement-first. Código = só instrumentação `admit_trace` (19 pontos, atrás de `THEODB_ADMIT_TRACE=1`).
Validação empírica no droplet.

## Evidência
- **Behavior-neutral (o gate crítico):** com o trace off, `run_m128 --agg` → `columnar_customscan_count = 14`,
  `result_ab.diverged = 0` — IDÊNTICO ao M151. O trace não muda o roteamento (só reporta a razão).
- **Mapa completo:** as 29 não-roteadas têm razão de declínio medida — **zero gaps** (cross-check: as 14 roteadas
  emitem 0 razões). Consistência 100%.
- **Bug pego e corrigido no spike:** `THEODB_ADMIT_TRACE` é lido pelo backend, precisava estar no ambiente do
  postmaster (não do cliente) — corrigido, re-medido.

## Achado (o valor do spike, estilo M148)
O mapa CORRIGE a hipótese do blueprint: GROUP BY texto NÃO é o lever (group-key texto já aceito); o real blocker é
o AGG_SORTED-texto-por-collation + text-`<>`-WHERE. Bloqueios são compostos → cobertura marginal por fatia = 2-4.
Reordena M153-M155 (COUNT DISTINCT > text-`<>` > GROUP BY texto), documentado em `m152-routing-map.md`.

## Decisão
Nenhum finding de correção (instrumentação behavior-neutral provada). O deliverable (routing-map + reorder medido)
está completo e consistente. DoD do M152 atendido (cada query → razão file:line; cobertura marginal medida; veredito
de reordenação honesto). **READY_TO_MERGE.**
