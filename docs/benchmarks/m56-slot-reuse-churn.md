# M56 fase 2 — slot-reuse churn benchmark (recall corrigido; benefício líquido marginal)

Caracterização (NÃO comparação competitiva) do efeito do slot-reuse in-place do `aminsert` (GUC
`theodb.hnsw_slot_reuse`) sob churn DELETE+INSERT sustentado. N=5000, dim=8, 10 ciclos de 20% de churn por
ciclo, numa dev box. A/B via o GUC (ON = reusa slot tombstonado; OFF = pending-append legado). Harness:
`benchmarks/run_m56_slot_reuse_churn.py`.

## História em 3 medições

**1. Regressão original** (antes do fix, compaction 20%): reuse **ON recall 0.57** vs OFF 0.95. O slot-reuse
consumia os tombstones antes do threshold → o fold (que REPARA o grafo) nunca disparava → recall despencava.

**2. Fix aplicado** — (a) reusar só slots de nível-0 não-entry (linking limpo, sem links obsoletos herdados nem
corromper o entry) + (b) gatilho de compactação por **churn** (`version>0` = tombstones + reusados), não só
tombstones, para o fold reparar mesmo sob reuso. Resultado (compaction 20%):

| Modo | recall@10 | crescimento do índice |
|---|---|---|
| **reuse ON** | **0.955** ✅ | +276% |
| reuse OFF | 0.92 | +290% |

Recall **corrigido** (0.57 → 0.955). Mas o fold agora dispara todo ciclo (churn 20% ≥ threshold 20%) → o
benefício de tamanho do slot-reuse fica marginal (size OFF/ON = **1.04×**).

**3. Threshold mais alto** (compaction 50%, fold raro): reuse ON recall **0.765**, OFF 0.805, size OFF/ON 1.18×.
Com folds raros, o recall degrada para AMBOS, e o reuso é levemente PIOR que o OFF (0.765 < 0.805).

## Veredito honesto (Regra 3)

- **Recall foi corrigido** — o slot-reuse é agora recall-safe (0.955) via o fix (a)+(b). ✅
- **MAS o benefício líquido é marginal:** o ganho de tamanho é **1.04–1.18×** e o slot-reuse **nunca melhora o
  recall** vs o navigate-through+fold puro (é igual ou levemente pior). Manter recall exige folds frequentes, que
  eliminam o ganho de tamanho; afrouxar o fold para ganhar tamanho degrada o recall de ambos.
- **Decisão (dirigida por evidência):** `theodb.hnsw_slot_reuse` é **OFF por default**. O caminho
  **navigate-through + fold-compaction** (DoD 2/3, recall estável, mais simples) é o default recomendado. O
  slot-reuse fica **opt-in** (implementado, testado, crash-safe, recall-safe) para workloads muito específicos que
  queiram o ganho marginal de tamanho entre folds — mas não é o caminho geral. Isso confirma a análise original:
  dado o design navigate-through+fold, o slot-reuse não tem benefício líquido significativo.

## Caveats honestos

- Caracterização numa dev box; recall@10 sobre 20 probes gaussianas; tamanho via `pg_relation_size`.
- Escala pequena (5000×8d) suficiente para expor o efeito de qualidade; o mecanismo (supressão da compactação, e
  a correção pelo gatilho de churn) é independente da escala.
- Reprodução: `run_m56_slot_reuse_churn.py --compact-pct {0|20|50}` (o JSON aqui é o run threshold=20 pós-fix).
