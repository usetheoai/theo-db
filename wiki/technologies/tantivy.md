---
type: Technology
title: Tantivy
description: O motor de busca full-text em Rust que é a base do BM25 próprio; sua abstração de storage permitiu persistir o índice no heap do PostgreSQL em vez de escrever páginas.
resource: https://github.com/quickwit-oss/tantivy
tags: [tecnologia, rust, lexical, bm25, busca, biblioteca]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: tantivy-repo
    resource: https://github.com/quickwit-oss/tantivy
    title: Tantivy, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O Tantivy é um motor de busca full-text escrito em Rust, com índice invertido segmentado e ranqueamento
[BM25](/technologies/bm25.md), sob licença permissiva. Uma característica de desenho o torna
especialmente adequado a este projeto: ele abstrai o armazenamento atrás de um trait, em vez de assumir
sistema de arquivos.[^recalled]

# Papel neste acervo

**É a base do [motor lexical BM25 próprio](/features/18-motor-lexical-bm25.md)** — com o projeto
implementando o storage e a superfície, e adotando o motor de indexação e ranqueamento.

A abstração de storage é o que viabilizou persistir o índice **no heap do PostgreSQL**, herdando MVCC,
WAL, TOAST e crash-safety **de graça** — a decisão do
[ADR 0052](/decisions/0052-m140-1-lexical-storage-decision.md), tomada porque o índice medido é **menor**
que a alternativa nativa, derrubando o argumento a favor de um access method dedicado.

# O achado que mudou a arquitetura

Um experimento mediu que **o Tantivy chama o storage de 4 threads distintas, mesmo configurado com um
único thread de escrita** — ele usa threads de merge e de background internamente.

Como SPI e o buffer manager do PostgreSQL são **exclusivos da thread do backend**, escrever direto
**derrubaria o backend**. Daí o desenho obrigatório de **bufferizar e só depois descarregar**, registrado
no [ADR 0051](/decisions/0051-m139-tantivy-pg-page-directory-design.md).

**Medir de onde uma biblioteca chama o seu código**, em vez de assumir pela configuração, é o que
transformou uma classe de crash em restrição de desenho — e a garantia virou **estrutural** ao mover o
núcleo para [um crate que não linka o framework](/decisions/0053-m140-2-lexical-core-crate.md).

# Situação

O motor existe, é robusto e é medido — mas **não está no binário default**, e a perna lexical embarcada
continua a nativa, porque **na fusão a troca não ganha** ([m138](/benchmarks/m138-bm25-fusion.md)).

[^tantivy-repo]: Tantivy, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
