---
type: Decision
title: M2 — Escolha do índice ANN default, dirigida por evidência
description: HNSW é o índice default porque venceu em recall, QPS, tempo de build e — em baixa dimensão — tamanho; a vantagem de compressão do DiskANN mostrou-se artefato de alta dimensionalidade.
resource: git:f7c7b93:docs/decisions/m2-index-decision.md
tags: [decisao, ann, hnsw, diskann, benchmark, glove, licenca, m2]
adr_status: Decided
decision_date: 2026-06-27
milestone: M2
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m2dec
    resource: git:f7c7b93:docs/decisions/m2-index-decision.md
    title: M2 — Index decision (evidence-driven)
    last_modified: 2026-06-27
---

Registro de decisão fora da série numerada de ADRs, e a **primeira** decisão do projeto tomada
inteiramente por evidência de benchmark — o precedente que a doutrina measurement-first depois
formalizou.

# O que foi entregue

O StreamingDiskANN do [pgvectorscale](/technologies/pgvectorscale.md) ficou disponível na imagem
oficial, via build multi-stage, custando +2 MB e **sem embarcar toolchain Rust**. E o harness passou a
medi-lo, com curva completa de recall × QPS, ao lado do [HNSW](/technologies/hnsw.md).

# Primeira medição — gaussiano sintético (n=5000, dim=128)

| índice | params | recall@10 | QPS | p95 | tamanho |
|---|---|---|---|---|---|
| HNSW | ef=40 | 0,715 | 2174 | 1,05 ms | 4,17 MB |
| HNSW | ef=100 | **0,940** | 1088 | 1,48 ms | 4,17 MB |
| DiskANN | sls=500 | 0,915 | 300 | 5,63 ms | **2,43 MB** |
| DiskANN | sls=1000 | **0,971** | 168 | 8,74 ms | 2,43 MB |

**Nota de método, e ela importa.** Rascunhos anteriores **fixavam o rescore em 500** enquanto varriam o
outro parâmetro para cima — assimetria que **congelava o recall do DiskANN num platô falso de 0,916**
enquanto o QPS continuava caindo. O harness passou a escalar o rescore junto, até o teto real da
engine, de modo que **todo ponto é um par (recall, QPS) verdadeiro**.

**QPS é relativo, não claim de throughput.** O número absoluto depende da carga da máquina — uma
corrida sob build concorrente leu ~2,5× menos em todas as linhas. O que é **estável** é a *forma*:
HNSW entrega 3 a 4× o QPS do DiskANN a recall igual, e o índice do DiskANN é 42% menor.

**Leitura honesta:** isso é **artefato do dataset**, em dois eixos. Vetores uniformes de alta dimensão
são quase equidistantes, e a quantização perde justamente as distinções finas de que esses dados
dependem. E o DiskANN é algoritmo de escala de bilhão, residente em disco — a 5000 vetores tudo cabe em
memória, e a vantagem de streaming é irrelevante.

**O sintético sozinho não podia decidir.** Escolher um índice a partir dele violaria a exigência de
benchmark representativo. Ele provou que o harness funciona e **sinalizou o próprio dataset como
inadequado**.

# Segunda medição — dataset real (glove-25-angular, n=50k, dim=25)

| índice | params | recall@10 | QPS | build | tamanho |
|---|---|---|---|---|---|
| HNSW | ef=40 | 0,984 | 2778 | **11 s** | **20,55 MB** |
| HNSW | ef=100 | **0,996** | 1495 | 11 s | 20,55 MB |
| DiskANN | sls=1000 | 0,933 | 75 | 123 s | 22,77 MB |

**Num dataset real, o HNSW domina em todos os eixos:** recall (0,996 contra 0,933 — o DiskANN nunca o
alcança), QPS (~20× mais rápido, e a recall **maior**), build (~11× mais rápido) e **tamanho** — e aqui
está o achado que inverte a leitura ingênua da primeira medição:

**A vantagem de 42% em tamanho DESAPARECE em dim=25** — ela era artefato de alta dimensionalidade. A
quantização comprime os vetores armazenados, mas em baixa dimensão o grafo e os vetores de precisão
plena para rescore dominam, então comprimir quase não economiza.

O sinal honesto: **a proposta de valor do DiskANN exige AMBOS — alta dimensionalidade (768–1536) E
grande escala (milhões)**. O glove tem nenhum dos dois; o sintético tinha só a dimensão. **Nenhum dos
dois benchmarks está no envelope de projeto do DiskANN.**[^m2dec]

# Decisão final

**HNSW é o índice ANN default do TheoDB.** A evidência é inequívoca em toda dimensionalidade e escala
medidas.

**O StreamingDiskANN permanece disponível** e documentado como opção para o regime para o qual foi
projetado — embeddings de alta dimensão em escala de milhões. **Nenhuma alegação de superioridade é
feita para esse regime: ele está NÃO MEDIDO para nós**, e verificá-lo exigiria um dataset de 768
dimensões com milhões de vetores.

# Política de fork honrada

O pgvectorscale é usado **como está**, com commit fixado, **sem fork**. A política é upstream-first, e
um fork exigiria benchmark de gatilho reproduzível — que não existe.

# Dívida de licença declarada

A licença de topo do pgvectorscale é permissiva e verificada. Mas o `.so` **linka estaticamente** a
árvore transitiva de crates Rust, e *esse* código embarca na imagem. Como o lockfile não carrega campos
de licença, **uma varredura de licenças sobre o conjunto fixado de crates é gate obrigatório de
pré-release**. Isto é imagem de desenvolvimento, não release — então a obrigação fica **rastreada
explicitamente, não assumida como limpa**.

# Reprodutibilidade do build

Fixado em todos os eixos: imagem base por digest, pgvector por SHA, pgvectorscale por commit, e as
versões de ferramental e toolchain declaradas.

[^m2dec]: M2 — Index decision (evidence-driven)
