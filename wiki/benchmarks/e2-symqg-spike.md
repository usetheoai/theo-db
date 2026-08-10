---
type: Measurement
title: e2 — spike fora do banco: gate atingido, e a autocorreção do primeiro rascunho
description: O primeiro rascunho concluía o oposto; a causa era um estimador degenerado na configuração usada, e o documento corrige a si mesmo explicando o mecanismo.
resource: git:f7c7b93:docs/benchmarks/e2-symqg-spike.md
tags: [benchmark, spike, autocorrecao, degenerescencia, symqg, sift1m]
dataset: SIFT1M
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: e2spike
    resource: git:f7c7b93:docs/benchmarks/e2-symqg-spike.md
    title: E2 — SymphonyQG spike
    last_modified: 2026-07-17
---

**Veredito atualizado: o gate fora do banco foi ATINGIDO** — paridade de recall com ~2,2× de vantagem a
1M.

# A autocorreção, e o que a causou

> O primeiro rascunho concluía que o ganho estava **travado num kernel**. **Isso era o estimador
> multi-bit** — que faz um produto escalar completo, custando aproximadamente o mesmo que a distância
> exata.

A correção: usar o código de **sinal de 1 bit**, em que o produto vira uma soma de termos com sinal —
**livre de multiplicação**, e 2 a 3× mais barato por elemento.

E a razão pela qual o caminho errado foi tomado é o achado mais útil: **o quantizador existente é
DEGENERADO com 1 bit** — a fórmula de níveis colapsa para zero e **produz códigos todos nulos**. Foi
preciso um codec de sinal dedicado.

**Uma configuração que produz saída degenerada sem erro** é a mesma classe de armadilha do
[ADR 0012](/decisions/0012-benchmark-data-degeneracy.md): o sistema aceita, roda, e devolve algo sem
sentido — e o número resultante parece plausível.

# A honestidade da revisão

O documento **mantém a conclusão anterior visível** e explica **por que ela estava errada**, em vez de
reescrever a história. É o mesmo padrão de [m41](/benchmarks/m41-hnsw-qps.md), que corrigiu o próprio
multiplicador para baixo e explicou a causa.

# O epílogo

**O gate fora do banco foi atingido, e o ganho não transferiu para dentro dele** — o veredito medido é o
[e2 in-PG](/benchmarks/e2-symqg-inpg-verdict.md).
