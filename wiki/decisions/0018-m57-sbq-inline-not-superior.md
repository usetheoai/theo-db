---
type: Decision
title: ADR 0018 — SBQ-inline NÃO é superior: veredito D3 do M57 (honest-negative)
description: A tese "SBQ ≥2× QPS sob pressão de RAM" foi falsificada por medição — o SBQ é recall-neutro e consistentemente 0,35–0,77× do QPS do f32 em todos os regimes.
resource: git:f7c7b93:docs/adr/0018-m57-sbq-inline-not-superior.md
tags: [adr, sbq, quantizacao, honest-negative, m57, anti-sunk-cost]
adr_id: "0018"
adr_status: Accepted
decision_date: 2026-07-08
owner: human:paulohenriquevn
milestone: M57
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0018
    resource: git:f7c7b93:docs/adr/0018-m57-sbq-inline-not-superior.md
    title: ADR 0018 — SBQ-inline NÃO é superior
    last_modified: 2026-07-08
---

Fecha o gate que o [ADR 0015](/decisions/0015-sbq-inline-keep-kill.md) deixara pendente — e o fecha
**contra** a hipótese.

# A tese sob teste

O ADR 0015 manteve o SBQ-inline condicionalmente, com o gate anti-sunk-cost dependente de uma
medição **a escala com pressão de RAM**: reter a tese do AM próprio apenas se o SBQ entregasse
**≥2× QPS a recall ≥0,99 sob pressão de memória**. A premissa era que os códigos SBQ, pequenos,
ficariam em cache enquanto os vetores f32, grandes, iriam para disco.

# Decisão

**Rejeitar a tese de superioridade do SBQ-inline.** O SBQ é recall-neutro contra f32, mas
**consistentemente mais lento** em todos os regimes medidos. A tese ≥2× está **falsificada por
medição**, e o AM próprio **não se justifica pela superioridade do SBQ**.

# Evidência

500k × 768d cosine, máquina limpa, recall idêntico de 0,956 em ambos ([m57](/benchmarks/m57-sbq-superiority.md)):

| Regime | SBQ QPS | f32 QPS | SBQ/f32 |
|---|---|---|---|
| in-RAM (16 GB) | 90 | 256 | **0,35×** |
| pressão 1,8 GB | 194 | 266 | 0,73× |
| pressão 1,3 GB (apertada) | 218 | 284 | 0,77× |

# O mecanismo — por que isso generaliza além do dataset

1. **O HNSW tem localidade de acesso.** Uma query toca ~`ef·log N` nós, então o índice f32 **não
   thrasha** sob pressão mesmo excedendo a RAM: as páginas quentes permanecem em cache. A premissa
   "o índice não cabe, logo há I/O por query" simplesmente não vale.
2. **O read-path do SBQ é mais caro por query** — walk por Hamming mais rerank exato em f32 — e
   piora relativamente com a escala (0,90× a 100k; 0,35× a 500k in-RAM), o oposto do previsto.

# Consequências

- **Finaliza o ADR 0015:** o AM próprio não é retido pela justificativa SBQ, e o eixo de
  superioridade vetorial segue não cumprido por essa via.
- **O AM próprio tem valor geral medido à parte** — o HNSW do TheoDB dá ~1,2× o QPS do pgvector a
  100k com recall equivalente. É tese **diferente** (qualidade do grafo e do scan), não coberta por
  este veredito; se o AM for retido, é por ela, com benchmark próprio.
- **Reenquadra o próximo passo:** o gap para o [ScaNN](/technologies/scann.md) é **quantização
  anisotrópica com Asymmetric Hashing SIMD**, não bit-quantization escalar. O trabalho migra para
  esse eixo — ver [ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md).
- O SBQ-inline **permanece no código** como formato versionado, sem custo de manutenção ativo e
  como base de experimentos futuros — mas **não é o caminho de performance** e não embasa claims.[^adr0018]

# Opções consideradas

**Reter o SBQ como default de performance** — seria claim falso. **Remover o SBQ-inline** —
rejeitada por YAGNI reverso: o formato versionado não custa manutenção e é base do próximo
milestone. **Honest-negative e reenquadramento** (escolhida) — medir, registrar a falsificação e
mover o esforço para o eixo certo.

# Ressalvas

Dados gaussian-mixture sintéticos, não SIFT1M — a direção é mecânica (localidade do HNSW mais custo
do rerank), mas os absolutos podem mover. O pgvector não buildou a 500k por limite de `/dev/shm` do
Docker, então o baseline dele é só a 100k. E o recall de 0,956 abaixo de 0,99 a 500k é qualidade do
grafo, item à parte deste veredito.

[^adr0018]: ADR 0018 — SBQ-inline NÃO é superior: veredito D3 do M57
