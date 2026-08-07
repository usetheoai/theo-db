---
type: Measurement
title: m82 — o algoritmo do ScaNN como access method: honest-negative final
description: Lossless e correto, sem ganho de QPS medível — e o rigor de construir os dois índices sobre a MESMA tabela, que foi a lição de um benchmark anterior.
resource: git:f7c7b93:docs/benchmarks/m82-pgscann-headtohead.md
tags: [benchmark, access-method, ivf-aq, same-data, honest-negative, m82]
dataset: SIFT1M
milestone: M82
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m82
    resource: git:f7c7b93:docs/benchmarks/m82-pgscann-headtohead.md
    title: M82 — pg_scann v4 IVF-AQ+AH Access Method
    last_modified: 2026-07-11
---

**Veredito: honest-negative final.** A medição terminal da linhagem: o algoritmo do adversário, shipado
como access method de primeira classe, medido **dentro do PostgreSQL** em escala real.

# O rigor de same-data

O método é explícito: carregar o dataset completo numa **única** tabela e construir **ambos** os índices
**sobre os mesmos dados** — **não** em containers separados.

**Essa disciplina é a lição de um benchmark anterior** ([m46](/benchmarks/m46-highrecall-qps.md)), onde a
comparação entre execuções distintas se mostrou invalidada por deriva da máquina. Comparar sobre a mesma
tabela, na mesma corrida, remove essa classe de erro por construção.

# O resultado

O índice é **funcionalmente correto**: o recall é **byte-idêntico** ao exato em todos os níveis de probe
— o pruning por hashing mais rerank exato é **lossless**.

E **não entrega ganho de QPS medível**: as diferenças ficam dentro do ruído. A recall alta ele fica na
classe da referência, cerca de **24× abaixo** do adversário.

# A causa-raiz

Os códigos estavam **interleaved** com os vetores nas mesmas páginas, então **ler os códigos paginava
também os vetores** — o scan pagava o I/O completo de qualquer forma. **O ganho de compute não importa
quando o gargalo é I/O.**

O detalhamento está no [ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md), e a decomposição do
gap em recuperável e irrecuperável, em
[pesquisa de separação de storage](/references/scann-storage-separation-2026-07.md).

# O que sobra de valor

Compressão de memória de **32× nos códigos, sem custo de recall** — benefício de **footprint**, não de
velocidade. E a semente da alavanca seguinte, testada em [m83](/benchmarks/m83-split-storage-spike.md).
