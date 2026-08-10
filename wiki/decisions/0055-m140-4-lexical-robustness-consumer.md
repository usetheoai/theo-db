---
type: Decision
title: ADR 0055 — Robustez da engine lexical provada contra o binário embarcado, e o boundary do consumidor
description: Crash, VACUUM e MVCC provados no binário que ships; a disciplina de thread-safety vira garantia estrutural; e o cutover do consumidor fica explicitamente fora deste escopo.
resource: git:f7c7b93:docs/adr/0055-m140-4-lexical-robustness-consumer.md
tags: [adr, lexical, robustez, mvcc, crash, spi, dogfood, m140]
adr_id: "0055"
adr_status: Accepted
decision_date: 2026-07-22
milestone: M140.4
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0055
    resource: git:f7c7b93:docs/adr/0055-m140-4-lexical-robustness-consumer.md
    title: ADR 0055 — Robustez da engine lexical
    last_modified: 2026-07-22
---

# D1 — robustez provada contra o binário embarcado

Um script sobe um cluster real com a extensão instalada e prova três coisas, medidas
([m140.4](/benchmarks/m140-4-robustness-consumer.md)):

- **Crash:** uma construção de índice **commitada** sobrevive a `SIGABRT` mais replay de WAL — porque
  a tabela heap é WAL-logged —, e a busca retorna o mesmo resultado antes e depois.
- **VACUUM:** após 4 reconstruções, cada uma deletando as linhas antigas, as tuplas mortas caem de 24
  para 0 no VACUUM, e a busca segue correta, sem corromper o índice vivo.
- **MVCC nos dois níveis de isolamento:** sob REPEATABLE READ, um leitor com snapshot antigo **não vê**
  a construção de outra sessão; sob READ COMMITTED, o statement seguinte **vê** — provando que o cache
  invalida corretamente com o snapshot novo.

O motivo de exigir o binário embarcado é a lição já registrada de que **uma suíte unitária verde não
basta**: a robustez só vale provada no binário que efetivamente ships.

# D2 — a disciplina de thread-safety vira garantia estrutural

Um probe de gravação de threads, apenas para teste, registra de quais threads o storage é chamado num
build multi-thread real do [Tantivy](/technologies/tantivy.md). Mas a prova de verdade é
**estrutural**: o storage que fica no caminho das threads **vive no crate livre de pgrx**, então é
**impossível por construção** tocar o Postgres de qualquer worker thread — o crate nem linka pgrx.

Uma regressão que colocasse SPI numa thread teria de **mover código para fora do núcleo**, o que é
pego pelo gate objetivo de dependências. A convenção deixou de depender de code review manual.

# D3 — fechando o straddle de SPI

Um achado fino do review: `Spi::get_one` no pgrx **não é read-only**. Ele marca a transação como
mutável, o que **abre um snapshot fresco por statement** — reabrindo a janela em que a geração lida e
os bytes carregados poderiam divergir — **e** queima um XID por busca, quebrando o uso em réplica de
leitura.

A correção é usar a forma **genuinamente read-only** de SPI. Com ela, a leitura de geração e o
carregamento dos bytes **reusam o snapshot ativo do statement**, tornando a invariante entre tag de
cache e conteúdo airtight. Bônus: a busca passa a rodar em **réplica de leitura** sem queimar XID.[^adr0055]

# D4 — o boundary do consumidor: prova agora, cutover depois

Entrega-se **prova ponta a ponta** de que a forma real de query do consumidor funciona sobre a nova
superfície, e **wiring testada** no repositório consumidor — aditiva, com o caminho default
**intocado**, e com teste unitário que roda no CI normal do consumidor, de modo que não é código morto.

**O cutover de produção — trocar o caminho default e sustentar 30 dias de uso — é milestone
separado.** A honestidade aqui é explícita: **não se reivindica "consumidor em produção" antes
disso**; o que existe é um consumidor **provado e ligado**, alimentando a evidência do dogfood.

# Alternativas consideradas

**Cutover total agora** — é escopo do milestone de dogfood. **Editar o caminho quente do consumidor
agora** — adicionaria código não exercido ao caminho quente antes do cutover, o que é passivo de
código morto; a wiring auto-contida e testada é a forma honesta. **Reestruturar o cache para ler
geração e conteúdo numa query só** — desnecessário: a correção read-only já garante a consistência,
e seria complexidade extra sem benefício medido.

# Consequências

A engine BM25 é reivindicável **robusta** — crash, VACUUM e MVCC provados no binário embarcado — e
**consumida**, com wiring testada. Até o cutover, o caminho antigo permanece o default do consumidor.
O risco de flush sob merge em escala segue como residual documentado.

[^adr0055]: ADR 0055 — Robustez da engine lexical + boundary do consumidor
