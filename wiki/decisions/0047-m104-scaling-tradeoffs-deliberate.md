---
type: Decision
title: ADR 0047 — Trade-offs de escala: limitados por desenho, não lacunas adiadas
description: Registra por que três itens residuais de escala são bounds deliberados com caminho de migração, e não gaps encobertos para mover uma nota de auditoria.
resource: git:f7c7b93:docs/adr/0047-m104-scaling-tradeoffs-deliberate.md
tags: [adr, escala, memoria, yagni, trade-offs, honestidade, m104]
adr_id: "0047"
adr_status: Accepted
decision_date: 2026-07-16
milestone: M104
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0047
    resource: git:f7c7b93:docs/adr/0047-m104-scaling-tradeoffs-deliberate.md
    title: ADR-0047 — M104 Scaling trade-offs
    last_modified: 2026-07-16
---

Escrito ao fechar um ciclo de hardening. A re-auditoria pontuou escala em 4,8 de 5, limitada por três
itens rotulados "pular, não corrigir". A leitura ingênua seria terminá-los para chegar a 4,9. **A
leitura honesta é que cada um já é limitado por desenho**, e o "conserto" restante é escopo de
pesquisa ou investimento num caminho depreciado — ambos proibidos pelos próprios princípios do
projeto.

# Contexto

O ciclo limitou **toda** trilha de memória ilimitada que a auditoria havia apontado: escrita colunar
com flush incremental de stripe, scan colunar em streaming de uma stripe, cache Arrow com teto de
entradas, batch de IA em chunks, fila do vectorizer com backpressure por coalescência, e dead-letter
com teto de retenção.

# As três decisões residuais

## 1. Compaction dentro do VACUUM — o guard mais REINDEX **é** o desenho

Os folds legados materializam o conjunto vivo em RAM, O(N). Foi adicionado um guard: quando o índice
em disco excede um limite configurável, o fold dentro do VACUUM **pula com WARN** e adia a compaction
para o `REINDEX`. **A correção é preservada** — o fold pendente do scan e a re-checagem MVCC já
retornam resultado correto sobre um índice não compactado; o que é adiado é apenas a **recuperação
de espaço**.

Isso é o desenho limitado correto, não um paliativo:

- **O [HNSW](/technologies/hnsw.md) é inerentemente O(N) em RAM para reconstruir.** Um fold de grafo
  precisa do conjunto completo de vetores para reconstruir as listas de vizinhos. Um "fold HNSW em
  streaming" é construção de grafo em memória externa — **tópico de pesquisa**, fora de escopo até
  haver necessidade medida. O `REINDEX` faz a mesma reconstrução como operação explícita e agendada
  pelo usuário, com memória idêntica — que é o lugar certo para ela.
- **Os formatos legados estão depreciados**, superseded pelo layout em streaming cujo fold já é
  limitado. Construir um fold limitado novo para um formato que estamos removendo é **complexidade
  acidental**.

**Adiado com escopo, não descartado em silêncio:** um fold de compaction HNSW em memória externa é
milestone futuro, gated numa necessidade **medida** — um workload real que não possa fazer REINDEX e
bata no guard. Registrar isso satisfaz a regra de não haver teto silencioso: o limite é documentado,
sinalizado por WARN, e tem caminho de migração.

## 2. Pool de conexões HTTP para IA — mitigado pelo batching

A auditoria apontou que o cliente abre conexão nova por chamada. Mas o chunking em lote já colapsa uma
superfície por linha em poucas requisições grandes, então o handshake TCP/TLS é **amortizado sobre até
256 linhas por conexão**.

Trocar o cliente mínimo por um com pool é mudança de dependência que teria de **re-provar toda a
postura de SSRF** — redirect zero, bloqueio de faixa privada, chave em header, falha fechada — e a
ligação do circuit breaker. Risco real por um benefício que o batching já capturou em boa parte.

## 3. Default do formato interleaved — estabilidade de formato acima de um flip arriscado

O caminho de build interleaved é propenso a OOM em escala; o layout limitado é o de storage separado.
Foi emitido um **WARN** apontando os escritores para a opção correta, **em vez de virar o default** —
porque o default governa o **formato on-disk**, e um flip silencioso muda o que índices novos
escrevem. Essa é decisão sensível a compatibilidade, que pertence a um ADR explícito de versão de
formato, não a uma passagem de hardening. O WARN fecha a surpresa do default invertido sem mudar
formato.

## 4. Decomposição do god-module de páginas — feita

O módulo de 1986 linhas foi dividido entre primitivas genéricas de página, buffer e WAL, e o cluster
de formato on-disk — com zero mudança em call-sites, via reexport de fachada, e a suíte inteira
verde. Fecha fisicamente o achado de namespace emaranhado, e não por racional.[^adr0047]

# Consequências

Toda limitação de escala é agora **limitada em código** ou **limitada por guard com caminho de
migração documentado**. Nenhuma trilha de memória ilimitada permanece sem sinalização.

**Aceito:** um índice legado ou HNSW muito grande não recupera espaço morto dentro do autovacuum —
exige `REINDEX` explícito. Isso é sinalizado por WARN e documentado aqui.

**Honestidade:** este ADR existe para que os itens "pular, não corrigir" da auditoria sejam lidos como
**desenhos limitados deliberados com caminho de migração**, que é sua verdadeira natureza — e não
como lacunas encobertas para mover uma nota.

[^adr0047]: ADR-0047 — M104 Scaling trade-offs: bounded-by-design, not deferred gaps
