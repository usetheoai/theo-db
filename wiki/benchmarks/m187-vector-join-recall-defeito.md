---
type: Measurement
title: m187 — o vector-join do HNSW perde exatamente um elemento, e o defeito é anterior à correção do planner
description: Dois testes que nunca haviam executado revelam 199 de 200 e 59 de 60 onde o contrato exige recall 1.0; a hipótese de que a correção do planner os causara foi testada e refutada.
resource: wiki/runbooks/rodar-a-suite-de-testes.md
tags: [benchmark, m187, recall, hnsw, vector-join, defeito, honest-negative, b-001]
milestone: M187
generated: { by: claude-code/opus-5, at: 2026-08-10T13:00:00Z }
---

Achado ao executar, pela primeira vez, a suíte que o [B-001] destravou. **Nenhum benchmark encontrou isto em
109 artefatos** — os dois testes existiam e nunca haviam rodado.

# O que falha

```
τ=2 (all cosine pairs): index idiom must match the exact count within the LIMIT window
  left: 199   right: 200

k ≥ |b| must return all of b (recall 1.0)
  left:  59   right:  60
```

Ambos em `am::hnsw_page` vector-join. **Errado por exatamente um elemento nos dois casos** — o índice devolve
199 de 200 e 59 de 60 onde o contrato do teste exige igualdade com a busca exata.

O segundo é o mais grave: `k ≥ |b|` significa que o `k` pedido é maior ou igual ao conjunto inteiro. Nesse
regime **não há trade-off de recall a fazer** — pedir tudo tem de devolver tudo.

# A hipótese que eu tinha, e a medição que a derrubou

Eu havia alterado o `amcostestimate` no [m175](/benchmarks/m175-planner-cost-inversion-verdict.md) horas
antes. A cadeia parecia óbvia: antes o planner nunca escolhia o índice, então o teste comparava seq scan
contra seq scan e passava **trivialmente**; ao fazer o índice ser usado, eu teria exposto — ou causado — a
perda.

**Testado revertendo apenas a correção TOAST em `am/mod.rs`, mantendo todo o resto:**

```
test result: FAILED. 2 passed; 2 failed
    pg_vector_join_recall_matches_exact_within_tol
    pg_vector_join_threshold_correct
```

**Os mesmos dois falham.** A correção do planner não é a causa, e a metade "tornei os testes significativos"
da minha hipótese também não se sustenta — eles já eram significativos e já falhavam.

# Por que isto importa mais que o número

O pilar vetorial tem **paridade de recall medida** contra o pgvector (M45/M60/M69/M70) e é o pilar mais
auditado do projeto. Este defeito não contradiz aqueles artefatos — eles mediram busca ANN top-k, e isto é o
caminho de **vector-join**, uma superfície diferente e menos exercitada.

O que ele contradiz é a suposição de que benchmark substitui suíte de teste. **109 artefatos de benchmark não
pegaram um off-by-one que dois testes unitários pegam em 8 minutos** — porque benchmark mede o caminho que
você escolhe medir, e teste cobre o caminho que você esqueceu.

# O que NÃO foi medido

- **A causa.** Só o sintoma. Nada em `hnsw_page` foi lido ainda.
- **Se é um só bug ou dois.** Os dois testes falham por um elemento, o que sugere causa comum — sugere, não prova.
- **As outras 18 falhas.** Duas causas foram capturadas de 20; as demais seguem sem mensagem.
- **Desde quando.** O teste nunca rodou, então o defeito pode ter qualquer idade.
