---
slug: columnar-gap-closing
generated_by: roadmap-feature
date: 2026-07-25
status: completed
source: derived-from-research (NOT a live grill)
---

# Grill log — columnar-gap-closing (M152-M155)

## Método (honestidade)

Os 4 milestones NÃO vieram de uma entrevista de 4 perguntas. Vieram do **blueprint de deep research**
`.claude/knowledge-base/discoveries/blueprints/columnar-gap-closing-strategy-blueprint.md` (2 agentes, R0 web +
R0.1 acervo, com fonte primária citada: PG source, papers X100/Kersten/Neumann, código DataFusion). O usuário
pediu explicitamente "crie os milestones baseados nos dados que temos" — a pesquisa já resolveu as 4 questões do
grill com evidência citada, então derivar dela é mais rigoroso que entrevistar.

## As 4 respostas do grill (extraídas do blueprint)

**Q1 — o que e por que agora:** fechar o gap colunar vs ClickBench. Medido: 29/43 queries são row-based (geomean
2,14s @100k, ~47s @1M), gargalo = ~80% materialização heap-tuple (M148). O gap NÃO é engine (DataFusion cobre tudo
nativamente) — é largura de roteamento. Agora porque M148-M151 (o programa colunar) acabou de fechar e deixou o
alvo claro e medido.

**Q2 — dependências:** M151 `[x]` (o CustomScan + a cobertura 14/43 medida). Cadeia interna: M152 (spike) gate
M153/M154; M153 gate M155.

**Q3 — DoD verificável:** cada milestone tem DoD medível (cobertura `columnar_customscan_count` sobe para número
REAL; A/B `diverged=0`; guard de collation/approx declina ao nativo, provado por regressão). Measurement-first: sem
`×N` prometido antes de medir.

**Q4 — riscos novos:** (a) collation não-determinística agrupa/ordena diferente → guards obrigatórios (a lição do
HIGH temporal/float do M151); (b) COUNT DISTINCT via approx perde byte-identidade → nunca approx; (c) o spike M152
pode revelar que GROUP BY texto já roteia → pivota o alvo (é o propósito do gate).

## Decisão de escopo

4 milestones: M152 (spike measurement-first, estilo M148) + M153 (GROUP BY texto) + M154 (COUNT DISTINCT) +
M155 (Top-N). NÃO incluídos (candidatos futuros, fora deste batch por YAGNI): parallel-safety via DSM (amplificação
cross-cutting de todos os caminhos roteados), Substrait-based general plan translation (o escape geral do matcher
per-shape — grande, depois das fatias diretas provarem o padrão). Regex/LIKE arbitrário e projeções-largas
EXCLUÍDOS por design (RE2≠POSIX / imposto de re-materialização — ver blueprint).

## Out-of-scope cross-check
Sem conflito: os milestones roteiam query classes pelo DataFusion JÁ embutido — não são "reescrever o engine
PostgreSQL" nem "reescrever parser genérico" (os 2 itens de Fora de escopo). A nota de 2026-07-24 já superou a
restrição de columnar own-code.
