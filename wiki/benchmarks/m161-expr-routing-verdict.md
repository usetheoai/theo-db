---
type: Measurement
title: m161 — roteamento por expressão, e a lição do falso-verde
description: Divergência zero sozinha não prova roteamento — um braço recusado iguala trivialmente o outro; a prova exige ver o plano.
resource: git:f7c7b93:docs/benchmarks/m161-expr-routing-verdict.md
tags: [benchmark, columnar, falso-verde, oraculo, planner, m161]
milestone: M161
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m161
    resource: git:f7c7b93:docs/benchmarks/m161-expr-routing-verdict.md
    title: M161 — expr routing verdict
---

# A lição do falso-verde

> O roteamento é provado pelo **plano**, mostrando o nó acelerado no braço ligado — **nunca inferido de
> uma divergência zero trivial**: um braço que **recusou** iguala o outro trivialmente.

Este é um dos achados metodológicos mais importantes da série. **Divergência zero é condição
necessária e não suficiente**: se a otimização **não foi aplicada**, os dois braços executam o mesmo
plano e o resultado é obviamente idêntico — um verde que não significa nada.

**A prova precisa de duas partes: o resultado é igual, E o caminho foi diferente.** Verificar o plano é o
que fornece a segunda.

# A separação de estados do planner

O documento mantém **dois estados distintos, explicitamente não confundidos**:

- **a medição de cobertura e a prova de roteamento** rodam as queries **como escritas**, sob
  configuração **padrão** do planner — e esse é **o número de produção**, porque é o que acontece com o
  usuário;
- o outro estado serve a outro propósito, e é mantido apartado.

**Misturar os dois produziria um número que nenhum usuário observa.** É a mesma preocupação do
[m95](/benchmarks/m95-cost-model.md): uma capacidade que só aparece quando forçada não é capacidade.

# O contrato

O contrato fail-closed estabelecido aqui — **ou roteia e é byte-idêntico, ou recusa corretamente** — é o
que o [m163](/benchmarks/m163-type-coverage-verdict.md) depois exercitou exaustivamente sobre o espaço de
tipos.
