---
type: Technology
title: ScaNN
description: A biblioteca de busca vetorial aproximada do Google, cujo algoritmo está sob o índice vetorial do AlloyDB — e cujo gap de QPS o projeto perseguiu e mediu como intransponível.
resource: https://github.com/google-research/google-research/tree/master/scann
tags: [tecnologia, ann, quantizacao, biblioteca, sota]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: scann-repo
    resource: https://github.com/google-research/google-research/tree/master/scann
    title: ScaNN, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O ScaNN — *Scalable Nearest Neighbors* — é a biblioteca de busca vetorial aproximada do Google, publicada
sob licença permissiva. A técnica que a distingue é a **quantização anisotrópica**: uma função de perda
que pesa o erro de quantização **conforme ele afeta o produto interno**, em vez de tratar todas as
direções igualmente.[^recalled] Combinada com **Asymmetric Hashing** e tabelas de lookup vetorizadas, ela
permite pontuar candidatos comprimidos muito rapidamente.

# Papel neste acervo — o número mais citado e mais mal-entendido

O ScaNN aparece aqui como **proxy sancionado do [AlloyDB](/technologies/alloydb.md)** — o algoritmo por
trás do índice vetorial dele —, medido em [m33](/benchmarks/m33-scann-headtohead.md) com um gap de ~25×
de QPS a recall alto.

**Esse número precisa de três qualificações**, todas registradas:

1. **O ScaNN é uma biblioteca in-memory** — sem persistência, sem transações, sem SQL. A comparação é do
   **eixo algorítmico**, e não torna o ScaNN um banco de dados.
2. **Boa parte do gap é imposto de sistema**, não algoritmo: literatura de sistemas mede que o overhead
   de página e MVCC consome a maior parte dos ciclos quando o mesmo algoritmo roda dentro do
   PostgreSQL — conforme o [dossiê de pesquisa](/references/scann-storage-separation-2026-07.md).
3. **A comparação correta e alcançável não é a biblioteca** — é o teto publicado do próprio AlloyDB.

# O que o projeto tentou, e o que mediu

O algoritmo é permissivo, então **construí-lo era permitido**. E foi construído: o
[ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md) registra o IVF com quantização e Asymmetric
Hashing shipado **como access method do PostgreSQL** — funcionalmente correto, lossless, **e sem ganho de
QPS**, porque o gargalo era I/O e não compute.

O veredito final é o [ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md): **superar o ScaNN em
QPS é não-alcançável por extensão permissiva**, por gap de paradigma — e a
[feature correspondente](/features/05-indice-scann.md) diz isso ao usuário em vez de prometer o
contrário.

[^scann-repo]: ScaNN, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
