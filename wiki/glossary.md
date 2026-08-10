---
type: Glossary
title: Glossário — os termos recorrentes deste acervo
description: Os termos cuja história inteira cabe em uma ou duas frases; os que têm fatos e relações próprios ganharam conceito.
tags: [glossario, terminologia]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
---

Termos que aparecem repetidamente nos conceitos e cuja explicação cabe aqui. O que tem história própria
está em [technologies](/technologies/alloydb.md) ou nas decisões.

# Vetorial e índices

recall@k
: A fração dos `k` vizinhos verdadeiros que uma busca aproximada recuperou. **A métrica de qualidade** de
  todo índice vetorial deste acervo, sempre medida contra um oráculo exato.

ef / ef_search
: O tamanho da lista de candidatos que a busca em grafo mantém. **Maior é mais recall e mais latência.**
  Como `recall(ef)` é monotônico não-decrescente, existe um menor `ef` que atinge um alvo — o que o
  [recomendador](/decisions/0026-m67-autotune-recommender.md) acha por bisecção.

probes / lists
: No índice por listas invertidas, `lists` é quantos centroides existem — definido no build — e `probes`
  quantos são sondados por query. **O scan lê aproximadamente `probes/lists` do índice.**

iso-recall
: Comparar dois sistemas **no mesmo nível de recall**, ajustando os parâmetros de cada um. Sem isso, basta
  baixar o recall para "ganhar" em velocidade — a razão de tantos vereditos do acervo insistirem em
  "recall casado".

quantização
: Comprimir vetores para representação menor. **Compra memória; neste acervo, repetidamente medida como
  NÃO comprando QPS** — ver [quantização vetorial](/features/19-quantizacao-vetorial.md).

over_fetch
: Quantos candidatos a mais o refinamento exato examina além de `k`. Pool pequeno demais **derruba o
  recall** mesmo com quantização correta.

# Storage e execução

zone-map
: Mínimo e máximo por bloco, guardados no metadado. Permite **pular blocos sem descomprimir**, e
  responder extremos sem ler dado — ver [zone-map](/benchmarks/columnar-zonemap-verdict.md).

pushdown
: Empurrar trabalho — projeção, filtro, agregação — para a camada mais baixa possível, para que o dado
  desnecessário nunca seja materializado.

byte-idêntico
: O gate de correção do pilar colunar: o resultado acelerado tem de ser **igual**, não equivalente, ao do
  caminho nativo. **Toda ampliação de cobertura custa uma prova disso.**

fail-closed
: Diante de dúvida, recusar em vez de tentar. A postura do filtro estruturado, das guardas de rede e da
  escrita em tipo não suportado.

fail-open
: O oposto — seguir por um caminho alternativo em vez de falhar. Usado com parcimônia e sempre
  documentado, como no [ADR 0059](/decisions/0059-m169-fail-open-cobre-falha-de-spill.md).

# Método

honest-negative
: Um resultado que **refuta a hipótese do próprio milestone**, publicado como entrega. O tipo mais comum
  de veredito deste acervo, e sinal de que o gate funciona.

measurement-first
: Medir antes de construir. Vários milestones foram **re-escopados ou cancelados** por uma medição barata
  feita antes do investimento.

gate
: Uma condição verificável que autoriza ou barra o passo seguinte. Um gate que **nunca barra** não é
  gate.

controle positivo
: Semear deliberadamente um erro para provar que o oráculo o detecta. **Sem ver o vermelho, o verde não
  informa** — ver [m167](/benchmarks/m167-type-coverage.md).

falso-verde
: Um teste que passa sem exercitar o que deveria. O caso canônico aqui: divergência zero entre dois braços
  **porque a otimização não foi aplicada** — ver [m161](/benchmarks/m161-expr-routing-verdict.md).

same-data A/B
: Comparar duas configurações **sobre a mesma tabela, na mesma corrida**. Remove a deriva de máquina que
  invalidou medições entre execuções.

# Licença

D1 / portão permissivo
: A regra de que só entram na distribuição licenças permissivas. **AGPL é barrada** — restrição que
  moldou o pilar colunar, o lexical e o vetorial, e que **esforço não dissolve**.

clean-room
: Implementar a partir do paper, tendo **estudado** uma referência de licença incompatível **sem
  copiá-la**. Algoritmos e layouts não são protegidos por copyright; código é.

study-only
: Marca aplicada a uma referência que pode ser lida e **nunca copiada**.
