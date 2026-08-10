---
type: Decision
title: ADR 0017 — Manutenção do HNSW a escala: híbrido tombstone-in-place + fold para compaction
description: Fecha o muro de RAM O(N) e a parada total de queries durante o VACUUM com tombstones in-place no caminho de DELETE, reusando o fold crash-safe do M48 apenas para compaction.
resource: git:f7c7b93:docs/adr/0017-m55-index-maintenance-at-scale.md
tags: [adr, hnsw, vacuum, escala, manutencao, m55, bloqueador-v1]
adr_id: "0017"
adr_status: Accepted
decision_date: 2026-07-07
owner: human:paulohenriquevn
milestone: M55
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0017
    resource: git:f7c7b93:docs/adr/0017-m55-index-maintenance-at-scale.md
    title: ADR 0017 — Manutenção do índice HNSW a escala
    last_modified: 2026-07-07
---

O ADR que nomeia a dívida classe-bloqueador de qualquer alegação de produção — e trava esse
vínculo explicitamente.

# O muro

O desenho original (grafo [HNSW](/technologies/hnsw.md) imutável com rebuild total no VACUUM) tem
um muro estrutural confirmado no código: o fold materializa o corpus **O(N) em RAM**, com múltiplas
cópias vivas no pico, sob o advisory lock **EXCLUSIVE** que cobre quase todo o rebuild.

Na escala north-star (1M+ × 768d) isso significa **gigabytes de RAM** — VmHWM projetado em ~14 GB
a 1M ([m55](/benchmarks/m55-vacuum-wall.md), ponto único e de baixa confiança, usado como proxy
porque a medição de RSS privado falhou), com estimativa analítica de working-set privado entre 6 e
10 GB — e **parada total de queries vetoriais durante o VACUUM**, medida em ~86 s a 100k. Além
disso, um scan longo segurando SHARE pode bloquear o VACUUM indefinidamente. O mesmo
`collect_corpus` sem teto limita também o **BUILD** (CREATE INDEX e REINDEX).

# Decisão: híbrido faseado

- **Fase 1 — tombstone-only in-place**, espelhando o pgvectorscale: DELETE marca a element-tuple
  in-place por página — RAM O(#deletados), lock em nível de buffer, **sem O(N) e sem parada
  total** —, e o scan filtra tombstones. Reuso de slot no INSERT segue o padrão do `hnswinsert.c`.
- **Compaction** reusa o **fold O(N) crash-safe do
  [M48](/decisions/0014-m48-crash-safe-fold-reclaim-mechanism.md)**, disparado por threshold. O
  meta-pivot atômico continua o único ponto de rewrite, e continua crash-safe.
- **Teto de memória do BUILD:** `collect_corpus` passa a alimentar o grafo **incrementalmente**,
  pelo próprio caminho de insert, em vez de materializar o `Vec` inteiro — alinhando build e
  manutenção no mesmo caminho. Raiz idêntica à do fold.

# Alternativas rejeitadas

**Fold incremental puro** — rejeitada como opção autônoma: o HNSW não faz merge de gerações
barato; ou vira edição in-place, ou multiplica o custo de scan pelo número de gerações. Foi
absorvida como o *lado compaction* da escolhida.

**In-place completo à la pgvector** (4 passes mais máquina de versão) — rejeitada como *primeira*
fase: quebra a invariante de grafo imutável e é reescrita grande. **Mantida como fase 2
condicional**, a ser adicionada **se** a medição de recall entre compactions sob tombstone-only
mostrar degradação inaceitável — a incerteza-chave declarada, já que este grafo pode não ter a
redundância de α-pruning do [DiskANN](/technologies/diskann.md).

**Status quo** — rejeitada: é dívida classe-bloqueador.

# Consequências

**Bom:** fecha o muro de RAM e de parada total no caminho de DELETE — o caso comum — sem descartar
o trabalho anterior; a compaction O(N) fica rara e já é crash-safe; e a mesma decisão resolve o
teto do BUILD.

**Risco:** fragmentação e degradação de recall entre compactions, a medir; e a fase 1 quebra
parcialmente a imutabilidade do grafo no caminho de delete, exigindo bump de magic e REINDEX.

**Gatilho v1.0, travado por este ADR:** a implementação da fase 1 é **pré-requisito de qualquer
alegação de produção ou v1.0**. Enquanto o rebuild-total-sob-EXCLUSIVE for o único mecanismo,
nenhuma alegação de "production-ready" é honesta na escala north-star.[^adr0017]

O trabalho decorrente foi medido em [m56 — manutenção in-place](/benchmarks/m56-inplace-maintenance.md)
e [m56 — reuso de slot sob churn](/benchmarks/m56-slot-reuse-churn.md).

[^adr0017]: ADR 0017 — Manutenção do índice HNSW a escala
