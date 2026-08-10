---
type: Measurement
title: m187 — o vector-join não tinha defeito: o teste exigia do HNSW uma garantia que ele não dá
description: Dois testes que nunca haviam executado pareciam revelar perda de recall; medindo, a causa era a premissa do próprio teste — ef_search limita o beam, e a semeadura gera apenas 55 vetores distintos em 60 linhas.
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


# Veredito final — o produto não tinha o defeito; o teste tinha a premissa errada

A varredura decisiva, com os **dados exatos da semeadura do teste** reproduzidos em SQL:

```
vetores DISTINTOS: 55   (em 60 linhas — 5 duplicatas exatas)

ef_search= 40 → 59      ef_search=100 → 60
ef_search= 60 → 59      ef_search=200 → 60
ef_search= 61 → 59      ef_search=500 → 60

id ausente com ef=60: 54
```

E com **dados aleatórios** de mesma dimensão e cardinalidade, `ef_search=60` devolve **60**. A diferença não
está no limite — está nos dados.

**A causa é a própria semeadura do teste.** `(i*7 + j*3) % 11` tem período 11 em `i`, `i % 5` tem período 5,
logo o padrão se repete a cada **55** — e 60 linhas contêm só 55 vetores distintos. Os empates de distância
nas duplicatas fazem o heap de resultado evictar quando o beam está apertado.

**`ef_search` limita o beam, não o resultado.** "Pedir `k ≥ |b|` devolve tudo trivialmente" é falso para HNSW,
e era a premissa das duas asserções. Elevar o beam **dá à asserção a condição que ela sempre pressupôs** — o
alvo (igualdade exata com o oráculo seqscan) permanece intacto, e é isso que separa esta correção de um
afrouxamento.

**Verificado:** `4 passed; 0 failed` nos quatro testes de vector-join.

## O que continua verdadeiro do registro anterior

A hipótese de que a correção do planner (m175) causara as falhas **foi testada e refutada** — revertendo
apenas a correção TOAST, os mesmos dois falhavam. Aquela medição segue válida; ela apenas apontava para a
causa errada, porque eu supunha defeito de produto quando era defeito de teste.

## O que este episódio ensina, e não é sobre HNSW

Eu registrei isto como "defeito de recall que 109 benchmarks não pegaram". **Era um teste errado que nenhuma
execução havia desmentido** — o que é uma frase diferente e menos alarmante sobre o produto, e igualmente
grave sobre o processo: um teste que nunca roda não protege nada, e ainda por cima acumula premissas falsas
que ninguém revisa.
