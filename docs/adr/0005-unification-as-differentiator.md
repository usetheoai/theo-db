# ADR 0005 — Diferencial do produto: Unificação tudo-em-um (performance competitiva, não líder)

**Status:** Accepted (LOCKED — refina o ADR 0002; sign-off CTO 2026-06-29) · **Data:** 2026-06-29 · **Owner:** CTO (paulohenriquevn)
**Atualiza (refina, não supersede):** ADR `0002-north-star-equal-or-superior-to-alloydb` (item 3 da Decisão) ·
**Relacionado:** ADR `0001-no-engine-fork`, ADR `0004-scann-fork-decision` (NO-FORK), PRD §15 (D1–D7), `ROADMAP.md`

> Esta ADR define **qual é o moat do TheoDB como produto**. Por tocar a estratégia LOCKED do ADR 0002,
> exige sign-off explícito do CTO + entrada no `CHANGELOG.md` (mesmo padrão de lock). Enquanto `Proposed`,
> nenhum roadmap/código novo deriva dela.

## Contexto

Pergunta do CTO (2026-06-29): *"é instalável, mas não somos um produto de fato — qual nosso diferencial?"*
Diagnóstico honesto (Regra 3): o que entregamos (M0–M15) é uma **composição de peças commodity** —
pgvector (está em Supabase/Neon/RDS), `ai.*` via plpython3u (casca fina sobre a API da OpenAI), DiskANN
(o **M14** mediu **paridade** com ScaNN-quality, não superioridade). "PostgreSQL + IA empacotado" já existe
e é mais bem financiado (AlloyDB Omni, proprietário). Ser "a versão OSS do AlloyDB" é **table-stakes, não
moat**.

Decisão do CTO sobre o diferencial: **NÃO** perseguir liderança de performance; ser **competitivo** em
performance e diferenciar por **uma feature** — a **unificação tudo-em-um**. Posicionamento: a **alternativa
OSS** a AlloyDB e Pinecone.

## Decisão

**O diferencial do TheoDB é a UNIFICAÇÃO tudo-em-um:** vetor + relacional + IA + colunar numa **única
instância PostgreSQL**, com `JOIN` entre embeddings e dados operacionais, em **SQL único e transacional**,
**sem ETL e sem um segundo sistema**.

1. **Moat = unificação (feature), não performance.** O valor é "um banco só": busca vetorial filtrada por
   dados relacionais + geração/rerank de IA + analytics colunar, na mesma query/transação, com
   consistência (sem staleness entre o vetor e o dado de negócio). Pinecone não tem relacional ("apague seu
   vector DB separado"); AlloyDB tem a capacidade mas é fechado/GCP ("AlloyDB-Omni aberto").
2. **Performance = competitiva, não líder.** Paridade "boa o suficiente" — já comprovada no M14 (DiskANN
   atinge a barra ScaNN-quality). NÃO perseguimos um número de recall/QPS líder; não há claim de "mais
   rápido" (Regra 5).
3. **Superioridade estrutural mantida (do ADR 0002):** abertura (Apache-2.0), custo (sem licença por vCPU),
   portabilidade (mesma imagem laptop→bare-metal), **independência de modelo**. Continuam válidas e
   reforçam o moat de unificação.

### O que isto ATUALIZA no ADR 0002 (LOCKED)

| ADR 0002 (antes) | Após esta ADR |
|---|---|
| Item 3 da Decisão: *"Superioridade de performance no pilar vetorial (killer), perseguida e comprovada por benchmark"* | **Performance vetorial competitiva (paridade) é suficiente.** O killer **não** é o número — é a unificação. O harness de benchmark continua, mas para **provar competitividade**, não liderança. |
| ScaNN-as-PG-AM / fork como "rota de superioridade no índice" | **Não é prioridade.** Coerente com o ADR 0004 (NO-FORK). D3 (fork-gate) permanece, mas o gatilho passa a ser **perda de competitividade** (paridade), não busca de liderança. |
| Paridade estrutural + abertura/custo/portabilidade/model-agnostic superiores hoje | **Inalterado.** Continuam válidos e são a base do moat de unificação. |

Tudo o mais no ADR 0002 (no-fork do engine, teto de licença D1, Opção β fora de escopo, honestidade) **permanece em vigor**.

## North-star metric (confirmada)

**Casos de migração saindo de Pinecone / AlloyDB pago → TheoDB** (já é a north-star metric do PRD §). É a
prova de que a unificação OSS ocupa o vácuo. Métricas de apoio: pulls da imagem, contribuidores externos.

## Consequências

- O próximo trabalho NÃO é um fork de índice nem um número de benchmark — é **provar e empacotar a
  unificação** que as peças já permitem: a *query unificada canônica* (vetor `JOIN` relacional + filtro +
  IA), **filtered vector search** eficiente (o ponto onde vetor+relacional juntos ganham), **migração do
  Pinecone**, e uma **demo honesta 1-sistema vs 2-sistemas** (consistência/simplicidade, não velocidade).
- Performance só vira trabalho se a **competitividade** for perdida (gatilho D3), não para buscar liderança.
- Evita o "amontoado de features": daqui em diante, cada milestone é avaliado por *"reforça a unificação /
  o caminho de migração?"* — não por adicionar mais uma capacidade isolada.
- A divergência com o ADR 0002 fica registrada e auditável (não é um drift silencioso).

## Alternatives considered

- **Manter o ADR 0002 as-is (perseguir superioridade de performance — Aposta B).** Rejeitado: o próprio
  M14 mediu paridade, não superioridade; perseguir liderança contra Qdrant/Milvus/AlloyDB-ScaNN é caro e
  incerto (sunk-cost que o D3 existe para evitar). O CTO decidiu competitivo, não líder.
- **Soberania / air-gapped como moat (Aposta A).** Válida e defensável, mas é um eixo de *deployment*; o CTO
  escolheu unificação como a feature central. (A soberania continua sendo uma superioridade estrutural — ADR
  0002 item 2 — e pode virar um sub-tema de um milestone futuro, não o moat principal.)
- **RAG end-to-end no SQL como moat (Aposta C).** Forte como DX, mas compete de frente com Supabase e é mais
  superfície; fica como complemento da unificação (a IA é uma das pernas do "tudo-em-um"), não o eixo.

## Honestidade (LOCKED)

- "Unificação" / "alternativa OSS ao AlloyDB/Pinecone" aparece como **posicionamento**, nunca como claim de
  performance. Nenhuma afirmação de "mais rápido" sem benchmark (`public-copy.md`, Regra 5). A demo
  comparativa mede **simplicidade/consistência** (1 vs 2 sistemas), não velocidade.

## Quando esta ADR pode mudar

Só com sign-off do CTO + nota de supersede + entrada no `CHANGELOG.md` (mesmo lock do ADR 0002).
