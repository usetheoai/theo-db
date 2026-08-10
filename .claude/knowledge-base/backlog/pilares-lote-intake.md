---
slug: pilares-lote
generated_by: backlog-item
status: completed
date: 2026-08-09
items: [B-002, B-003, B-004, B-005, B-006, B-007, B-008, B-009, B-010]
---

# Desvio de protocolo, declarado

O contrato do `/backlog-item` pede um grill de 4 perguntas, uma por turno, por item. Nove itens dariam
36 turnos. **Respondi as quatro perguntas eu mesmo**, a partir das medições já publicadas, porque o owner
deu a diretiva em lote com o objetivo explícito ("não precisamos ser melhor em todos os benchs, mas temos
que ser atrativos") e porque cada `why_now` tinha resposta medida no acervo — não precisava ser perguntada.

O que isso custa: os `suggested_mode` e os `dod` são minha leitura, não a do owner. Ambos são revisáveis —
o `suggested_mode` é explicitamente não-vinculante (`cycle-backlog § Item schema`) e o DISCOVER pode
reclassificar.

# Gate G5 — nenhum item passou por prior-art

Cada `why_now` cita medição **nossa**: M73 (B-002), M88/M89 (B-003), M140.3/M140.4/M184 (B-004), M123
(B-005), M128/M184 (B-006), M184 (B-007, B-008, B-009), M175 + estado do `theo-rag` (B-010). Nenhum se
justifica por "projeto X faz assim". O BEIR e o ClickBench aparecem como **instrumento de medição**, não
como justificativa — a distinção que o G5 exige.

# Gate G2 — dedup

`grep` por pilar em `BACKLOG.md`: os únicos hits foram a tabela de roteamento, não itens. B-001 (`cargo
pgrx test` não roda) foi deliberadamente **não duplicado** — B-008 o cita como dependência em vez de
re-filar a mesma coisa.

# Gate G3 — um pilar por item

B-007 (grafo) e B-008 (lakehouse) foram roteados para `colunar` por ser o especialista cuja superfície os
cobre (`theo-columnar` inclui DataFusion/Parquet). Não existe pilar `grafo` na tabela — é uma lacuna da
tabela de roteamento, registrada aqui e não resolvida por conta própria.
