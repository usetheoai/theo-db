---
type: Measurement
title: teto de escala do arnês — 10M medido, 1B quantificado e não alcançado
description: Carga de 10 000 000 vetores em 155 s com 1,16 GB de RSS contra 5,1 GB de corpus; a 64 332 linhas/s um bilhão são 4,3 h de carga e 780 GB de disco, contra 284 GB livres no host medido.
resource: .claude/knowledge-base/reviews/b061-analytical-suite-review-2026-08-17.md
tags: [benchmark, arnes, escala, copy-binario, streaming, honest-negative]
generated: { by: claude-code/opus-5, at: 2026-08-17 }
sources:
  - id: scale
    resource: theodb-bench @ workspace 2026-08-17
    title: theodb-bench — carga em streaming e COPY binário
    last_modified: 2026-08-17
---

> **Correção por acréscimo (2026-08-17).** As contas de teto abaixo são de **disco**, e continuam certas
> sobre disco. Elas são **silenciosas sobre RAM**, e é a RAM que decide a construção de índice: medido em
> [o teto de escala do build de HNSW é RAM](/benchmarks/build-hnsw-teto-de-ram.md), o build mantém
> **~1,7 GB por milhão** em memória do backend, o que põe o teto de construção deste host em **~4,7M** —
> não nos ~200M que o disco comportaria. Carga e consulta a 20M funcionam; construção de grafo não.

**10 000 000 de vetores carregados em 155 s, com o corpus nunca residente.** Medido no droplet efêmero
`138.197.22.192` (s-8vcpu-16gb, nyc3), SIFT-128 lido de HDF5 em chunks de 200 000.

Contexto: o arnês é o `theodb-bench`, e a régua que ele aplica está em
[o instrumento reporta o pedido](/guides/instrumento-reporta-o-pedido.md). A escala aqui é a do pilar vetorial,
cujo veredito medido está em [scann AM × theodb_hnsw × pg_scann](/benchmarks/b057-scann-am-headtohead.md).

# O que foi medido

| | |
|---|---|
| linhas carregadas | **10 000 000** (`complete = True`) |
| tempo | **155 s** → **64 332 linhas/s** |
| pico de RSS do cliente | **1,16 GB** |
| corpus, se residente | 5,1 GB |
| tabela resultante | 5796 MB |

O RSS é limitado pelo **chunk**, não pelo corpus — é isso que torna a escala uma questão de disco em vez de
memória.

# A escada que levou aqui

Três implementações da mesma carga, 1 000 000 de vetores SIFT-128, mesmo host:

| implementação | tempo | razão |
|---|---|---|
| `executemany` em lotes de 1000 | **122 s** | — |
| `COPY` texto | **75 s** | 1,63× |
| `COPY` binário | **16,8 s** | **7,3×** |

O degrau do meio é o que justificou o terceiro: dos 75 s do COPY texto, **72 eram a codificação em Python** —
um `repr()` por float, 128 milhões deles. Reduzir round-trips já tinha dado tudo que dava.

# O bilhão, quantificado e não alcançado

| | |
|---|---|
| vetores brutos float32 | **512 GB** |
| na tabela `vector(128)` | **520 GB** |
| com índice HNSW (~1,5×) | **~780 GB** |
| carga a 64 332 linhas/s | **4,3 h** |
| **disco livre no host medido** | **284 GB** |

**Um bilhão não cabe.** E o disco não é o único limite: construir HNSW sobre 1B vetores é trabalho de dias, não
de horas. A capacidade do arnês está entregue e verificada a 10M; a corrida exige máquina diferente.

Registrar isto é o ponto. Um benchmark cujas alegações de escala ultrapassam suas medições é pior que um que
declara seus limites — e este vai ser publicado.

# O oráculo, que era o segundo bloqueio

Força bruta é um produto Q × N: a 1B linhas e 10 000 consultas, **10¹³** cálculos de distância por corrida.
`neighbour_vectors` busca apenas os **k × Q** vetores que as ids de vizinhos publicadas nomeiam, lê cada linha
distinta uma vez (consultas compartilham vizinhos), coalesce corridas contíguas num único read, e **reporta
quantas linhas leu** — para a corrida poder dizer isso em vez de insinuar que leu o corpus.

Distâncias publicadas continuam **nunca** usadas: carregam a precisão e a convenção de métrica de outra pessoa.

E uma id de vizinho fora do corpus é **recusada**, não descartada. Acontece quando um dataset é subamostrado sem
remapear as listas de vizinhos, e descartar em silêncio **aumentaria** o recall — removendo exactamente os
vizinhos que o sistema não achou.

# Ressalva do dado medido

O corpus de 10M foi o SIFT1M lido **dez vezes**. Isso exercita o caminho de streaming na escala real de linhas e
bytes, e **não** produz recall significativo — vetores duplicados o tornariam sem sentido. O que esta medição
afirma é a **carga**, que é o que o trabalho mudou. Nenhum número de qualidade sai daqui.
