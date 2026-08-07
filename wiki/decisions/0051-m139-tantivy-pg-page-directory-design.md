---
type: Decision
title: ADR 0051 — Backend de páginas PG para o Directory do Tantivy: buffer-then-flush obrigatório
description: Um experimento mediu que o Tantivy chama o Directory de 4 threads distintas mesmo com um worker — e SPI e buffer manager do Postgres são backend-thread-only, o que redesenhou a arquitetura.
resource: git:f7c7b93:docs/adr/0051-m139-tantivy-pg-page-directory-design.md
tags: [adr, lexical, tantivy, mvcc, threads, crash-safety, m139]
adr_id: "0051"
adr_status: Accepted
decision_date: 2026-07-21
milestone: M139
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0051
    resource: git:f7c7b93:docs/adr/0051-m139-tantivy-pg-page-directory-design.md
    title: ADR 0051 — M139 design do backend de páginas PG
    last_modified: 2026-07-22
---

O ADR cujo maior valor é um **achado empírico que corrigiu o próprio desenho** antes de qualquer
código de produção — e que teria custado um crash de backend se não tivesse sido medido.

# O desenho proposto

O backend de páginas do `Directory` do [Tantivy](/technologies/tantivy.md) **reusa** o primitivo de
blob-sobre-páginas-WAL já provado nos access methods, e a visibilidade MVCC segue o **padrão de MVCC
por catálogo** do [colunar próprio](/decisions/0042-m99-own-code-columnar-tam.md). Não se reinventa
storage, WAL nem MVCC — compõe-se o que existe.

- **Storage:** o trait `Directory` é write-once, então cada "arquivo" — segmento ou metadados — vira
  um blob persistido em páginas com WAL. A substituição do metadado é atômica porque um único
  registro de WAL publica a troca: **a atomicidade é a do WAL, não a de rename de filesystem**.
- **MVCC:** a visibilidade transacional vem de um **catálogo de segmentos** com `xmin`/`xmax`, de
  modo que um leitor com snapshot anterior lê a lista de segmentos visível ao **seu** snapshot.
- **Crash real:** reusa o harness de SIGABRT mais replay de WAL que já provara durabilidade em
  incidentes anteriores.

# O achado empírico que corrigiu o desenho

Um experimento mediu **de quais threads** o Tantivy chama o `Directory`. Resultado: **mesmo
configurando um único thread de escrita, os `write` vêm de 4 threads distintas.** O Tantivy usa
threads de merge e de background que chamam os métodos do `Directory` diretamente.

**Consequência dura:** SPI e o buffer manager do PostgreSQL são **exclusivos da thread do backend** —
chamá-los de uma thread do Tantivy **derrubaria o backend**. É exatamente a classe de bug que um
spike existe para pegar.

Logo, o storage **não pode tocar o Postgres no `write`**. A arquitetura correta é **buffer-then-flush**:

1. durante a indexação, bufferizar em memória, thread-safe, com **zero** chamadas ao PG — de qualquer
   thread;
2. depois que o commit do writer **retorna** (na thread principal), persistir os arquivos
   bufferizados via SPI — operação de thread principal, dentro da transação corrente;
3. ao abrir, carregar do heap para o buffer, também na thread principal.

Para o spike, bufferizar sobre uma tabela heap `bytea` reusa **toda** a máquina do Postgres — TOAST,
MVCC e WAL — sem código próprio de página e WAL, o que é **mais parsimonioso que a proposta
original**; o caminho de páginas fica como otimização posterior, se o TOAST se mostrar lento.[^adr0051]

# Alternativas rejeitadas

**Copiar o directory MVCC do ParadeDB** (~105 mil linhas, AGPL) — barrado por licença; estuda-se, não
se copia. **Registrar um resource manager de WAL próprio**, como o ParadeDB faz — o mecanismo genérico
de WAL já resolve a crash-safety; só subir para resource manager se um gate provar que não basta.
**Reimplementar storage e WAL** — já existem e já são provados sob crash.

# Consequências

**GO condicional:** os gates de MVCC e de crash são implementáveis compondo o que existe. O risco
residual real é **custo** — merge e paralelismo —, não viabilidade de storage.

**Esforço:** semanas, por causa da integração transacional, e não uma sessão — declarado
honestamente.

O núcleo ficou **livre de dependência do pgrx** e testável isoladamente, o que virou a semente do
crate de núcleo lexical decidido no [ADR 0053](/decisions/0053-m140-2-lexical-core-crate.md).

[^adr0051]: ADR 0051 — M139 gate 2/3: design do backend de páginas PG para o Directory do Tantivy
