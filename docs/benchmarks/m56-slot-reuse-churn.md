# M56 fase 2 — slot-reuse churn benchmark (índice não incha, MAS regride recall)

Caracterização (NÃO comparação competitiva) do efeito do slot-reuse in-place do `aminsert` (GUC
`theodb.hnsw_slot_reuse`) sob churn DELETE+INSERT sustentado. N=5000, dim=8, 10 ciclos de 20% de churn por
ciclo, numa dev box. A/B via o GUC (ON = reusa slot tombstonado; OFF = pending-append legado). Harness:
`benchmarks/run_m56_slot_reuse_churn.py`.

## Resultado — as duas pernas (compaction desligada vs ligada)

| Compaction | Modo | Crescimento do índice | recall@10 |
|---|---|---|---|
| **off** (isola o slot-reuse) | reuse **ON** | **0%** | **0.475** |
| off | reuse OFF | 37.9% | 0.53 |
| **on (20%)** (produção) | reuse **ON** | **0%** | **0.57** |
| on (20%) | reuse OFF | 284% | **0.95** |

## Veredito honesto (Regra 3)

- **Slot-reuse LIMITA o tamanho do índice** — 0% de crescimento vs 38–284% no pending-append. ✅ (a parte estrutural do DoD-1 funciona.)
- **MAS slot-reuse REGRIDE o recall** — 0.57 vs 0.95 (compaction ligada). ❌
- **Causa-raiz:** o slot-reuse **suprime o gatilho de compactação por ratio** — ele consome os tombstones antes de
  atingirem o threshold, então o **fold (que REPARA o grafo, DoD 3) nunca dispara**. Sem o reparo periódico, o
  insert incremental in-place degrada a qualidade do grafo cumulativamente. O caminho OFF deixa os tombstones
  acumularem → a compactação dispara → o fold repara → recall volta a 0.95.
- **Conclusão:** slot-reuse **troca recall por tamanho — uma troca RUIM**. O design **navigate-through + fold-compaction**
  já entregue no M56 (DoD 2 mediu recall estável ≥0.9 sob 20% de tombstones; DoD 3 o fold repara+reclama) é o
  caminho correto. Por isso o GUC `theodb.hnsw_slot_reuse` é **OFF por default** — o slot-reuse fica **opt-in** para
  quem aceitar o trade (ou até landar: melhor qualidade de linking do insert incremental, OU um gatilho de
  compactação dirigido por contagem-de-reuso que force o reparo periódico mesmo com os tombstones reusados).

## Caveats honestos

- Caracterização numa dev box; recall@10 sobre 20 probes gaussianas; tamanho via `pg_relation_size`.
- O crescimento OFF de 284% (compaction on) reflete o pending crescendo dentro de cada ciclo antes do fold reclamar;
  o fold eventualmente reclama, mas menos prontamente que o slot-reuse — daí o trade tamanho×recall.
- Escala pequena (5000×8d) suficiente para expor o efeito de qualidade; o mecanismo (supressão da compactação) é
  independente da escala. Reprodução: `run_m56_slot_reuse_churn.py --compact-pct 0` e `--compact-pct 20`.
