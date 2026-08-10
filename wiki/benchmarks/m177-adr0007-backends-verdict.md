---
type: Measurement
title: m177 — o footgun do ADR 0007 medido: a latência escala com a concorrência, os backends não
description: Cada theodb.embed prende um backend pela latência inteira do modelo, mas a saturação do servidor chega antes de max_connections — o custo real é a latência do usuário, não o esgotamento de conexões.
resource: benchmarks/artifacts/m177/adr0007-backends.json
tags: [benchmark, m177, adr-0007, backend, max-connections, concorrencia, postgres-no-laco]
milestone: M177
generated: { by: claude-code/opus-5, at: 2026-08-08T03:30:00Z }
sources:
  - id: adr7
    resource: benchmarks/artifacts/m177/adr0007-backends.json
    title: theodb.embed sob concorrência, com PostgreSQL no laço (1–16 clientes)
---

Fecha a última pergunta aberta desta área. O [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)
registrou em junho de 2026 que cada chamada segura **um backend PostgreSQL inteiro** pela latência do
modelo, e decidiu que *"máquina de fila é complexidade essencial apenas depois de um gargalo medido"*.
Todas as medições anteriores deste milestone mediram **o servidor**; esta é a primeira com **o banco no
laço** — extensão `theodb` instalada, `theodb.embed()` real, `pg_stat_activity` observado durante a carga.

# O número

`theo-db:0.139.0`, `max_connections = 100`, servidor de embeddings local, 4 chamadas por cliente:

| clientes | rps | p50 | p99 | **backends ativos (pico)** | % de `max_connections` |
|---|---|---|---|---|---|
| 1 | 9,7 | 104,4 ms | 113,5 ms | 1 | 1,0% |
| 4 | 25,7 | 156,3 ms | 184,0 ms | 3 | 3,0% |
| 8 | 30,5 | 247,9 ms | 324,9 ms | 5 | 5,0% |
| 16 | 34,1 | **443,4 ms** | **715,0 ms** | **8** | **8,0%** |

**O mecanismo do ADR 0007 é real e está confirmado:** os backends ficam ativos durante a chamada, e o
número deles cresce com a concorrência. Zero erros em todos os níveis.

# Mas o custo não é onde o ADR temia

**`max_connections` não é o gargalo.** A 16 clientes concorrentes, o pico de backends ativos foi **8 —
8% do limite**. Extrapolando linearmente, esgotar as 100 conexões exigiria ~200 clientes concorrentes,
e o servidor de embeddings satura muito antes disso ([~195 rps em CPU dedicada, com 13% de recusa a
128 clientes](/benchmarks/m177-stress-colapso-verdict.md)).

**O custo real é a latência.** De 1 para 16 clientes o throughput sobe **3,5×** enquanto a p50 sobe
**4,2×** e a p99 **6,3×**. O sistema não quebra — ele fica lento, e a degradação é sentida pelo usuário
antes de qualquer limite de conexão ser tocado.

**Consequência para a fila assíncrona:** o ADR condicionou a máquina de fila a um gargalo medido. O
gargalo medido **não é o esgotamento de conexões** — é o enfileiramento no servidor de inferência. Uma
fila assíncrona no banco (modelo `pg_net`) resolveria o backend preso, mas o backend preso não é o que
dói primeiro. **O que dói é a capacidade do servidor**, e isso se resolve com batching, limite de
concorrência e réplicas — que é o [M180](/benchmarks/m177-stress-colapso-verdict.md) e o M178, não uma
reescrita assíncrona da superfície.

# Um achado colateral: o guard de SSRF funciona

A primeira tentativa de configurar o endpoint **foi recusada pelo banco**:

> `theodb.embed: refusing to call host.docker.internal — it resolves to a blocked internal address`

O guard do M134 barrou um alvo interno e apontou a saída correta (`theodb.egress_allowlist`). Não era o
objeto desta medição, mas é evidência de execução de que a defesa opera em runtime — e não apenas em
teste.

# Limites honestos

- **Uma máquina compartilhada** (a mesma que produziu cinco retratações neste milestone). A p50 de
  104 ms a um cliente é ~2× a medida em CPU dedicada; os **valores absolutos são pessimistas**, e o que
  vale é a **forma**: backends crescem devagar, latência cresce rápido.
- **Cada cliente é um `docker exec psql`** — abre processo e conexão a cada chamada, o que infla a
  latência medida e **subestima** o número de backends simultâneos que um pool de aplicação real
  produziria. Um pool que mantém conexões abertas manteria mais backends residentes.
- **`max_connections = 100`** é o default da imagem. Uma instalação com limite menor chegaria mais perto.
- **Não medido:** o comportamento com o modelo multilíngue (3× mais lento, o que aumentaria a janela em
  que cada backend fica preso) e com pool de conexões do lado da aplicação.

# Relacionados

- A decisão que pediu esta medição: [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)
- Capacidade do servidor, o gargalo que chega antes: [m177 stress](/benchmarks/m177-stress-colapso-verdict.md)
- A fila que já existe para a ingestão: [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md)
- O guard que barrou o endpoint: [filtro fail-closed](/references/m120-fail-closed-filter.md)
