---
type: Measurement
title: m177 fase 1 — o hop local custa 15 ms; a residência do modelo custa 1,7 GB por processo
description: Mede os dois lados da troca que decide a extensão de embeddings local, e o resultado refuta a rota por backend em multilíngue — não por licença nem por peso de pacote, mas por memória.
resource: benchmarks/artifacts/m177/
tags: [benchmark, m177, embedding, multilingual, memoria, hop, rerank, licenca, veredito-parcial]
milestone: M177
generated: { by: claude-code/opus-5, at: 2026-08-07T21:00:00Z }
sources:
  - id: hopfair
    resource: benchmarks/artifacts/m177/hop-cost-fair.json
    title: Custo do hop, coleta com orçamento de thread equalizado (n=40)
  - id: hopbias
    resource: benchmarks/artifacts/m177/hop-cost-biased-run1.json
    title: Coleta anterior, enviesada — preservada como registro do erro de método
  - id: survey
    resource: benchmarks/artifacts/m177/model-survey.json
    title: Custo por modelo multilíngue (RSS, load, latência), subprocesso isolado por modelo
---

Primeira metade do gate do **M177**. Responde **o custo**; **não** responde a qualidade — ver § O que
este artefato NÃO mede.

# A pergunta

O TheoDB [chama um endpoint e não embarca modelo](/guides/sql-embeddings.md). A proposta é entregar o
modelo como **extensão instalável**. A troca é: economizar o hop HTTP local **versus** pagar para ter o
modelo residente. O [prior art](/references/embedding-local-como-extensao-2026-08.md) apontou memória por
conexão como o custo decisivo — este artefato põe número nos dois lados.

# Lado 1 — o hop custa 15 ms, e só importa em chamada unitária

`BAAI/bge-small-en-v1.5`, mesmo modelo nos dois braços, `OMP_NUM_THREADS=1` em ambos, braços
**alternados** A,B,A,B, n=40, bootstrap pareado:

| batch | in-process (ms) | via HTTP (ms) | hop | % do total | p (permutação) |
|---|---|---|---|---|---|
| 1 | 41,56 ± 13,10 | 57,11 ± 13,28 | **15,55 ms** | 27,2% | 0,0000 |
| 8 | 122,90 ± 33,17 | 130,66 ± 49,85 | 7,76 ms | 5,9% | **0,1457 — não significativo** |

**O hop é custo fixo por requisição**, então seu peso cai com o batch: relevante e significativo em
chamada unitária (`theodb.embed` por linha), e **estatisticamente indistinguível de zero em lote de 8** —
que é o regime do [vectorizer](/features/16-vectorizer.md).

## O erro de método que precedeu este número, registrado

A primeira coleta usou `taskset -c 3` no cliente e deixou o servidor sem restrição. Resultado: **hop
negativo** (−14,82 ms, p=0,0000) — fisicamente impossível, porque o braço HTTP faz a mesma inferência
mais transporte. A causa era o orçamento de CPU: o braço in-process estava preso a **um** core; o
servidor usava os **doze**. O artefato enviesado fica preservado em `hop-cost-biased-run1.json`, porque
um resultado impossível que sobreviveu a duas coletas é a evidência mais útil que este experimento
produziu sobre a própria régua.

# Lado 2 — a residência custa entre 0,7 e 1,8 GB por processo

Cada modelo medido em **subprocesso isolado** (RSS medido após outro modelo é contaminado). Só modelos
**D1-limpos**; os non-commercial não foram medidos.

| modelo | dim | RSS | latência b1 | latência b8 | licença |
|---|---|---|---|---|---|
| `paraphrase-multilingual-MiniLM-L12-v2` | 384 | **677 MB** | 16,0 ± 1,4 ms | 47,6 ± 1,4 ms | apache-2.0 |
| `intfloat/multilingual-e5-large` | 1024 | **1 735 MB** | 59,8 ± 2,0 ms | 400,3 ± 8,0 ms | mit |
| `paraphrase-multilingual-mpnet-base-v2` | 768 | **1 788 MB** | 18,0 ± 1,1 ms | 120,0 ± 8,0 ms | apache-2.0 |
| `nomic-embed-text-v1.5` | 768 | 1 384 MB | 42,9 ± 7,1 ms | 261,1 ± 49,1 ms | apache-2.0 |
| *(referência en)* `bge-small-en-v1.5` | 384 | 213 MB | — | — | mit |

Um `python3` vazio ocupa 10,5 MB — o custo acima é do modelo, não do runtime.

**Nota contraintuitiva medida:** o `mpnet` multilíngue é **3,3× mais rápido** que o `e5-large` em batch 1
(18,0 contra 59,8 ms) com RSS praticamente igual (1 788 contra 1 735 MB). Maior não é mais lento por
definição, e a escolha por reputação teria errado aqui.

# Lado 3 — rerank: o único multilíngue está barrado por licença

| modelo | RSS | latência (8 docs) | licença | D1 |
|---|---|---|---|---|
| `Xenova/ms-marco-MiniLM-L-6-v2` | 194 MB | 35,7 ± 5,4 ms | apache-2.0 | OK — **mas inglês** |
| `Xenova/ms-marco-MiniLM-L-12-v2` | 279 MB | 65,2 ± 4,0 ms | apache-2.0 | OK — **mas inglês** |
| `BAAI/bge-reranker-base` | **1 787 MB** | 147,1 ± 3,9 ms | mit | OK |
| `jinaai/jina-reranker-v2-base-multilingual` | não medido | não medido | **cc-by-nc-4.0** | **BARRADO** |

O único rerank explicitamente multilíngue do catálogo é **non-commercial** — barrado por D1, e por isso
não medido: medir o que não pode ser distribuído produz um número que seduz e não pode ser usado. O mesmo
vale para `jinaai/jina-embeddings-v3` no lado do embedding.

**A cobertura multilíngue do `bge-reranker-base` NÃO foi verificada** por este artefato. Ele é MIT e foi
medido em custo; se ele atende português é pergunta em aberto.

# O veredito

**Uma cópia do modelo por backend está refutada para o regime multilíngue.** O stack multilíngue mais
forte que passa no D1 — `multilingual-e5-large` (1 735 MB) mais `bge-reranker-base` (1 787 MB) — custa
**~3,5 GB por processo**. A máquina desta medição tem 15 GB totais e 7 disponíveis: **dois backends
esgotam a memória livre**. Economizar 15,55 ms por chamada a esse preço não se sustenta em nenhuma
aritmética de concorrência.

Isto **não** refuta a extensão local. Refuta uma das duas rotas de residência, e é exatamente a rota que
o `pg_gembed` adota (cache por backend). A rota do **BackgroundWorker**
([ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md)) — uma cópia, fora do caminho da query —
permanece viável e agora tem o número que a justifica.

E há uma ironia útil no lado 1: o hop **só** é significativo em batch 1, e é precisamente no lote que o
worker opera, onde ele deixa de ser mensurável. **A rota que resolve a memória também é a que menos
ganha com a eliminação do hop.**

# O que este artefato NÃO mede

- **Qualidade.** Nenhum nDCG, nenhum recall. O primeiro item do DoD da Fase 1 — comparar modelos no nosso
  corpus — **não foi executado**, porque exige corpus com qrels. Toda a tabela acima é **custo**, e um
  modelo barato que recupera mal não serve.
- **Português especificamente.** Os modelos são multilíngues por catálogo; nenhum foi medido em pt-BR.
- **Empacotamento dos pesos.** O terceiro item do DoD segue aberto.
- **`load_ms` não é limpo.** Os tempos de carga observados (21 s a 128 s) **incluem download** na primeira
  execução e não devem ser lidos como tempo de inicialização.

# Ambiente

Linux, 12 cores, 15 GB RAM (7 disponíveis), **com dez containers Docker ativos** — máquina não dedicada.
`OMP_NUM_THREADS=1` e `ORT_NUM_THREADS=1` nos dois braços do lado 1. Instrumentos:
`benchmarks/m177_hop_cost.py`, `benchmarks/m177_model_survey.py`. Significância por
`benchmarks/theodb_bench/significance.py` (bootstrap pareado). A não-dedicação da máquina é a maior
ameaça aos desvios reportados, e por isso o veredito se apoia na **ordem de grandeza da memória**
(gigabytes contra milissegundos), não em diferenças finas de latência.

# Relacionados

- O prior art que motivou a medição: [embeddings locais como extensão](/references/embedding-local-como-extensao-2026-08.md)
- O desenho atual: [embeddings em SQL](/guides/sql-embeddings.md)
- A rota de residência que sobrevive: [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md)
- O footgun da chamada síncrona por linha: [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)
- O rerank já existente na superfície: [ranquear resultados](/features/09-ranquear-resultados.md)
