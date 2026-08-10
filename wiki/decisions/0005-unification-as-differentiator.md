---
type: Decision
title: ADR 0005 — O diferencial é a unificação tudo-em-um, não a performance
description: O moat do TheoDB é vetor + relacional + IA + colunar numa única instância PostgreSQL, sem ETL; performance é competitiva, não líder.
resource: git:f7c7b93:docs/adr/0005-unification-as-differentiator.md
tags: [adr, estrategia, moat, unificacao, posicionamento]
adr_id: "0005"
adr_status: Accepted (LOCKED, ampliado pelo ADR 0006)
decision_date: 2026-06-29
owner: human:paulohenriquevn
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0005
    resource: git:f7c7b93:docs/adr/0005-unification-as-differentiator.md
    title: ADR 0005 — Diferencial do produto
    last_modified: 2026-06-29
---

Nasceu de uma pergunta desconfortável do CTO — *"é instalável, mas não somos um produto de
fato; qual nosso diferencial?"* — e da resposta honesta que ela forçou.

# Contexto

O diagnóstico registrado é duro: o que fora entregue até então era uma **composição de peças
commodity**. [pgvector](/technologies/pgvector.md) está no Supabase, no Neon e no RDS; a
superfície `ai.*` era casca fina sobre a API da OpenAI; e o [DiskANN](/technologies/diskann.md)
medira **paridade** com qualidade-ScaNN, não superioridade
([m14](/benchmarks/m14-scann-fork-decision.md)). "PostgreSQL + IA empacotado" já existia, mais
bem financiado, no [AlloyDB](/technologies/alloydb.md) Omni. Ser "a versão OSS do AlloyDB" é
table-stakes, não moat.

# Decisão

**O diferencial é a UNIFICAÇÃO tudo-em-um:** vetor, relacional, IA e colunar numa **única
instância PostgreSQL**, com `JOIN` entre embeddings e dados operacionais, em SQL único e
transacional, **sem ETL e sem um segundo sistema**.

1. **Moat = unificação (feature), não performance.** O valor é "um banco só": busca vetorial
   filtrada por dados relacionais, geração e rerank de IA, e analytics colunar — na mesma query
   e na mesma transação, sem staleness entre o vetor e o dado de negócio. O Pinecone não tem
   relacional; o AlloyDB tem a capacidade mas é fechado e preso ao GCP.
2. **Performance = competitiva, não líder.** Paridade "boa o suficiente", já comprovada. Não se
   persegue número líder de recall/QPS e não há claim de "mais rápido".
3. **Superioridade estrutural mantida** do [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md):
   abertura, custo, portabilidade, independência de modelo.

## O que isto atualiza no ADR 0002

| ADR 0002 (antes) | Depois deste ADR |
|---|---|
| Superioridade de performance vetorial perseguida e comprovada | **Competitividade (paridade) é suficiente.** O killer não é o número — é a unificação. O harness continua, para provar competitividade |
| ScaNN-as-PG-AM / fork como rota de superioridade | **Não é prioridade.** O fork-gate permanece, mas o gatilho passa a ser **perda de competitividade**, não busca de liderança |
| Paridade estrutural + abertura/custo/portabilidade | **Inalterado** — é a base do moat |

# North-star metric

**Casos de migração saindo de Pinecone ou de AlloyDB pago para o TheoDB.** É a prova de que a
unificação OSS ocupa o vácuo. Métricas de apoio: pulls da imagem, contribuidores externos. O
caminho de migração correspondente está em [migrar do Pinecone](/guides/migrate-from-pinecone.md).

# Consequências

O próximo trabalho deixou de ser fork de índice ou número de benchmark e passou a ser **provar e
empacotar a unificação**: a query unificada canônica, a
[busca vetorial filtrada](/features/08-acelerar-consultas.md) eficiente — o ponto exato onde
vetor e relacional juntos ganham —, a migração do Pinecone, e uma demonstração honesta de
[1 sistema contra 2 sistemas](/guides/unification-1-vs-2-systems.md) medindo simplicidade e
consistência, **não** velocidade.

Cada milestone passou a ser avaliado por *"reforça a unificação ou o caminho de migração?"* em
vez de por adicionar mais uma capacidade isolada — a defesa explícita contra o amontoado de
features.

# Alternativas consideradas

**Manter o ADR 0002 as-is e perseguir superioridade de performance.** Rejeitada: o próprio M14
medira paridade; perseguir liderança contra Qdrant, Milvus e o ScaNN do AlloyDB é caro e
incerto. **Soberania / air-gapped como moat.** Válida e defensável, mas é eixo de *deployment*;
continua sendo superioridade estrutural, não o eixo central. **RAG end-to-end no SQL.** Forte
como DX, mas compete de frente com o Supabase e é mais superfície; fica como complemento.[^adr0005]

# Como evoluiu

O [ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md) **ampliou** este ADR: o moat
passou a incluir código próprio defensável em Rust/Go, e a performance voltou a ser perseguível
sob benchmark. A unificação segue sendo um pilar do produto.

[^adr0005]: ADR 0005 — Diferencial do produto: unificação tudo-em-um
