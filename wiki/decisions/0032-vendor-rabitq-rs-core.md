---
type: Decision
title: ADR 0032 — Vendorizar o core do rabitq-rs (Apache-2.0) para o índice IVF-RaBitQ
description: Copiar para dentro do projeto apenas o núcleo algorítmico do RaBitQ — quantizador, rotação, FastScan, SIMD — descartando a camada de storage do upstream, que é incompatível com um AM do Postgres.
resource: git:f7c7b93:docs/adr/0032-vendor-rabitq-rs-core.md
tags: [adr, rabitq, quantizacao, ivf, vendoring, licenca, supply-chain]
adr_id: "0032"
adr_status: Accepted
decision_date: 2026-07-10
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0032
    resource: git:f7c7b93:docs/adr/0032-vendor-rabitq-rs-core.md
    title: ADR-0032 — Vendorizar o core do rabitq-rs
    last_modified: 2026-07-10
---

# Contexto

O head-to-head mediu ~25× de gap de QPS contra o [ScaNN](/technologies/scann.md), cuja vantagem é de
**paradigma**: IVF com quantização e Asymmetric Hashing (FastScan LUT SIMD), não grafo em precisão
plena. Duas tentativas de quantizar sobre o carrier [HNSW](/technologies/hnsw.md) —
[SBQ](/decisions/0018-m57-sbq-inline-not-superior.md) e
[anisotrópico+AH](/decisions/0019-m59-ah-needs-code-vector-separation.md) — foram **refutadas por
medição**.

A pesquisa apontou três fatos que reposicionaram a aposta:

1. O SOTA permissivo é o **[RaBitQ](/technologies/rabitq.md)** — quantização 1-bit, **sem treino de
   codebook**, com **bound de erro provado** que pode dispensar rerank a recall alto. Adotado por
   Milvus, Faiss e Elasticsearch.
2. Existe implementação **pura em Rust, Apache-2.0**, com IVF, RaBitQ, FHT, FastScan e SIMD.
3. O TheoDB **já tem a metade cara**: IVFFlat próprio com k-means++ e listas invertidas, o access
   method, storage page-native e WAL. **O carrier certo já é nosso.**

# Decisão

1. **Vendorizar apenas o CORE algorítmico** de um commit auditado: quantizador, rotação, FastScan,
   kernel e math. **Não** vendorizar a camada de storage do upstream, baseada em arquivo e mmap —
   ela é substituída pela infraestrutura page-native do access method.
2. **Wiring:** o core quantiza os vetores; a **nossa** IVF particiona, pagina e faz WAL dos códigos;
   o scan de cada lista invertida usa a FastScan vendorizada sobre os códigos comprimidos. Isso é
   IVF-RaBitQ.
3. **Vendoring, não dependência**, por quatro razões: o storage do upstream é incompatível com um AM
   do Postgres, então a integração do core seria necessária de qualquer forma; controle para evoluir
   sem esperar upstream; supply-chain (o crate é 0.9.0, de repositório individual, com bug conhecido
   em ARM64 — congela-se um commit auditado); e Apache-2.0 permite copiar com atribuição.

# Alternativas rejeitadas

**Depender do crate** — não daria para usar a camada de índice, e acoplar a stack a um crate 0.9.0
individual com bug de arquitetura é risco. **Reimplementar do zero** — violaria a regra de não
reinventar: o algoritmo é sutil (bound de erro, rotação FHT, packing de códigos) e há implementação
permissiva pronta mais a canônica como oráculo. **Adotar o AQ anisotrópico do ScaNN** — possível
patente sobre a loss anisotrópica, mais treino complexo de codebook; o RaBitQ é permissivo,
training-free e com bound. **Quantizar de novo sobre HNSW** — já refutado duas vezes; o carrier
precisa ser IVF, para permitir batch-scan.

# Consequências

**Positivas:** transforma o ataque ao gap de "aposta de meses do zero" em "integrar um core
permissivo na nossa IVF", e é a alavanca ainda **não refutada**.

**Obrigações de licença:** preservar `LICENSE` e um arquivo de proveniência no diretório vendorizado,
com atribuição; toda modificação rastreada em git; e a auditoria de licença precisa passar. A
manutenção do core passa a ser nossa — aceitável, já que a modificação era inevitável.

**Gate antes da integração completa:** um spike de recall e velocidade deve mostrar caminho viável de
fechar fração significativa do gap antes do investimento total. **Honest-negative aceito** — e foi
exatamente o que veio, no [ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md): o
ganho do RaBitQ é **memória, não QPS**.[^adr0032]

O destino do diretório vendorizado está registrado no
[ADR 0046](/decisions/0046-rabitq-vendor-tree-deleted.md).

[^adr0032]: ADR-0032 — Vendorizar o core do rabitq-rs
