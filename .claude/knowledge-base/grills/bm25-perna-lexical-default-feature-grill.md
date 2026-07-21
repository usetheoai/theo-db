---
slug: bm25-perna-lexical-default
milestone_id: M138
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Grill — M138 BM25 como perna lexical default (executa o gate de adoção do ADR-0013)

> **Nota de método (honestidade):** as 4 perguntas do grill não foram feitas numa entrevista separada — as
> respostas vêm da análise de fundação conduzida com o owner em 2026-07-21 (peers neon/paradedb/pg_durable, inventário
> medido do repo, e as decisões que ele tomou ao longo dela). Registro a diferença em vez de simular uma
> entrevista que não houve.

**Q1 — O que é e por que AGORA?** Medição decision-grade **nossa**, M53 (2026-07-07, BEIR scifact, 5.183
docs, 300 queries): a perna lexical **shipada** (`ts_rank_cd`) mede nDCG@10 **0,0703**; `pg_textsearch` BM25
mede **0,6881**; o vetor mede 0,7296. O próprio artefato declara *"o gate de medição está executado"* — e a
adoção nunca aconteceu. Caveat honesto herdado do M53: o gap de ~9,8× conflaciona qualidade de ranker com
tamanho do candidate-set (o `@@` do `ts_rank_cd` derruba ~93% dos relevantes); o sinal limpo é BM25 0,688
ombro-a-ombro com o vetor 0,730 sobre o próprio top-k. **Por que agora:** é defeito de produto conhecido há
duas semanas, e vira a **linha de base que qualquer engine própria terá de bater**.

**Q2 — Dependências.** M137 `[ ]` — mudar o default da superfície SQL sem cadeia de upgrade entrega a melhoria
só para instalações novas.

**Q3 — Decisões do owner.** "Vamos usar o Tantivy assim como o ParadeDB utiliza" + priorização do BM25 como
passo 2 (2026-07-21).

**Q4 — Riscos NOVOS.** (a) `pg_textsearch` passa de exceção *gated* a **dependência embarcada** na distribuição
— o roadmap § "Fora de escopo" a mantinha explicitamente "não embarcada ainda"; embarcar e depois substituir
pelo motor próprio (M140) é churn real de packaging e docs, aceito conscientemente. (b) Trocar o default muda
resultados de queries existentes — exige nota de migração e provavelmente um período com os dois engines
selecionáveis.
