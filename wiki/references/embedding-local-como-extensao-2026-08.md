---
type: Reference
title: Prior art — gerar embeddings localmente como extensão PostgreSQL instalável
description: A arquitetura já existe sob Apache 2.0 e MIT, e duas fontes independentes convergem no mesmo custo decisivo — memória por conexão, não licença nem peso do pacote.
resource: https://pgxn.org/dist/pg_gembed/1.0.0/
tags: [referencia, prior-art, embedding, inferencia-local, extensao, onnx, licenca, nao-e-evidencia]
generated: { by: claude-code/opus-5, at: 2026-08-07T20:15:00Z }
sources:
  - id: pggembed
    resource: https://pgxn.org/dist/pg_gembed/1.0.0/
    title: pg_gembed 1.0.0 — Generate embeddings inside PostgreSQL (PGXN)
  - id: postgresml
    resource: https://github.com/postgresml/postgresml
    title: PostgresML — Postgres with GPUs for ML/AI apps
  - id: neurstore
    resource: https://arxiv.org/pdf/2509.03228
    title: "NeurStore: Efficient In-database Deep Learning Model Management System (arXiv:2509.03228)"
  - id: pginfer
    resource: https://www.postgresql.org/about/news/pg_infer-100-released-transformer-model-knowledge-as-sql-relations-3307/
    title: pg_infer 1.0.0 released — transformer model knowledge as SQL relations
  - id: pgonnx
    resource: https://github.com/microsoft/onnxruntime/discussions/17574
    title: "pg_onnx: ONNX Runtime integrated with PostgreSQL"
  - id: oracleonnx
    resource: https://blogs.oracle.com/developers/how-to-load-embedding-models-into-oracle-ai-database-in-2026
    title: How to Load Embedding Models into Oracle AI Database in 2026
  - id: pgai
    resource: https://dev.to/tigerdata/we-listened-pgai-vectorizer-now-works-with-any-postgres-database-1e57
    title: We Listened — Pgai Vectorizer Now Works With Any Postgres Database
---

Levantamento feito em 2026-08-07 para responder a **uma pergunta de decisão, não de medição**: a proposta
do owner de gerar embeddings localmente, com o modelo entregue **como uma extensão instalável** em vez de
embarcado no binário default — hoje o banco chama um endpoint e
[não embarca modelo algum](/guides/sql-embeddings.md).

# Isto é prior art, e prior art não é evidência

**Nada aqui justifica trabalho por si só.** O que outro projeto faz não mede o nosso sistema, e este acervo
recusa "o projeto X faz assim" como razão para agir. O valor deste documento é outro e é real: ele responde
*"isso já foi feito, e como?"* antes de gastarmos um milestone descobrindo sozinhos — e mostra **onde os
outros pagaram a conta**, que é a parte que um benchmark nosso não revelaria de graça.

Qualquer decisão que saia daqui precisa de um ADR e de medição no nosso sistema.

# A arquitetura proposta já existe — e uma implementação é quase idêntica

| Projeto | Licença | Desenho | Por que importa aqui |
|---|---|---|---|
| **pg_gembed 1.0.0** | **Apache 2.0** | Extensão fina que marshalla tipos PostgreSQL para o **C ABI de um core Rust portátil** (`libgembed`); backends plugáveis — embed_anything, FastEmbed, **ORT** (ONNX Runtime), gRPC e **HTTP** | É a proposta, implementada e permissiva. Mantém HTTP **ao lado** dos backends locais, que é exatamente a coexistência que preservaria nossa independência de modelo |
| **PostgresML** | **MIT** | Inferência in-process no backend, com GPU | Mostra que o formato escala em ambição; alega 8–40× sobre model serving por HTTP |
| **pg_infer** (mai/2026, PG18+) | não verificada | Expõe internals de transformers como **relações SQL** + access method próprio | Vizinho conceitual do nosso AM |
| **pg_onnx** | não verificada | ONNX Runtime dentro do PostgreSQL | Discussão hospedada no próprio repositório do ONNX Runtime |
| **Oracle AI Database** | proprietária | Modelo ONNX importado como **objeto schema-native** | O SOTA comercial tomou o mesmo caminho |

**A licença não é o gate aqui.** Apache 2.0 e MIT passam no D1 sem discussão — o que contraria a intuição
inicial de que este seria o obstáculo.

# O custo que decide, e ele não é o que parecia

Duas fontes independentes — uma de produto, uma acadêmica — convergem no mesmo ponto: **o problema central
é memória por conexão.**

- O `pg_gembed` documenta **"backend-level model caching"**: o modelo é cacheado **por backend**. Com um
  encoder de algumas centenas de MB, N conexões concorrentes custam N cópias.
- O **NeurStore** ataca precisamente isso, e a lista do que ele precisou construir mede o tamanho do
  problema: storage engine dedicado, **deduplicação de modelo**, compressão (zlib / zstandard / ZFP) e
  **buffers de modelo compartilhados entre conexões**, explicitamente para conter o *memory overhead per
  connection*.

**Isto é a informação mais cara deste levantamento.** Peso do pacote e licença — as duas objeções
intuitivas — não são o obstáculo. O obstáculo é onde o modelo fica residente, e ele só aparece sob
concorrência, que é o regime em que um banco vive.

Há um caminho que este projeto já tem construído para contorná-lo: o
[vectorizer](/features/16-vectorizer.md) roda um **BackgroundWorker in-process**
([ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md)) cuja razão de existir é justamente tirar a
latência do modelo da transação de quem escreve. Um modelo carregado **uma vez, no worker** paga uma cópia
em vez de N, e nunca segura um backend — o que também neutraliza o footgun de escala que o
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md) registra.

# Uma restrição nossa que parece bloquear e não bloqueia

O [ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md) registra que distribuir os `#[pg_extern]`
**por feature** foi abandonado: no [pgrx](/technologies/pgrx.md), todos os externs compartilham um único
`#[pg_schema] mod theodb_rs`, porque o schema SQL vem do *ident* do módulo.

Isso **não** veta uma extensão separada. Aquela restrição é sobre fatiar a superfície de **uma** extensão em
N módulos de schema dentro do mesmo crate; uma extensão distinta tem outro `.so`, outro control file e outro
schema. A distinção fica registrada aqui porque alguém relerá o 0009 e concluirá o contrário — e porque o
projeto **já distribui três extensões** com `requires` entre elas (`theodb`, `vector`, `theodb_rs`), de modo
que o padrão está provado em casa.

# O contra-argumento, que só apareceu na pesquisa

O **pgai saiu do formato de extensão**: funcionava bem em Postgres self-hosted, mas usuários de RDS, Supabase
e outros managed services não conseguiam instalá-lo, porque não é possível instalar extensão arbitrária lá.

Para o TheoDB isso pesa **menos** — a edição é downloadable e self-hosted —, mas é um dado real sobre
alcance, e pertence a qualquer ADR sobre o assunto em vez de ser descoberto depois.

# O que este levantamento NÃO estabelece

- **Se compensa.** O claim de 8–40× do PostgresML é de vendor, sem artefato reproduzível, e compara contra
  model serving por **HTTP provavelmente remoto**. Nosso baseline é outro: um servidor ONNX local no mesmo
  host, que os testes de integração já exercitam. O número deles não responde à nossa pergunta.
- **O custo de empacotar os pesos.** Centenas de MB num pacote de extensão, ou download no primeiro uso —
  com a questão de ambiente offline em aberto. Nenhuma das fontes documenta isso, e pode ser o custo
  dominante sem aparecer em benchmark de latência.
- **Qual modelo.** Ranking público (MTEB e afins) seleciona candidatos; o veredito tem de sair do nosso
  corpus, com a bancada BEIR que já existe.

# Relacionados

- O desenho atual, que este levantamento questiona: [embeddings em SQL](/guides/sql-embeddings.md)
- O footgun de escala da chamada síncrona: [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)
- O worker que já tira o modelo da transação: [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md)
- A restrição de schema do pgrx que **não** se aplica: [ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)
- O isolamento de biblioteca com threads, já praticado: [ADR 0053](/decisions/0053-m140-2-lexical-core-crate.md)
- A independência de modelo como superioridade estrutural: [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)
