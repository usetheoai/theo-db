---
type: Decision
title: ADR 0022 — Vector JOIN via LATERAL index-scan, sem nó de join customizado e sem helper
description: O padrão CROSS JOIN LATERAL já é um similarity join servido pelo índice ANN — provado por EXPLAIN —, então o M63 entrega validação e medição em vez de código de engine.
resource: git:f7c7b93:docs/adr/0022-m63-vector-join-lateral-not-node.md
tags: [adr, vector-join, lateral, planner, yagni, m63]
adr_id: "0022"
adr_status: Accepted
decision_date: 2026-07-09
milestone: M63
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0022
    resource: git:f7c7b93:docs/adr/0022-m63-vector-join-lateral-not-node.md
    title: ADR 0022 — M63 vector JOIN
    last_modified: 2026-07-09
---

O ADR que entrega **zero código de produção** — e explica por que isso é o resultado certo.

# Contexto

O critério do M63 pedia um *similarity join* que **usa o índice ANN** (não nested-loop O(n·m)),
integrado ao planner, com recall preservado, mais um caso ponta a ponta de deduplicação em SQL
puro. A investigação concluiu que o padrão

```sql
a CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j
```

**já é** esse join, sem engine novo: cada iteração do LATERAL reduz `b.emb <=> a.emb` ao top-k
single-vector que o `amcanorderbyop` do `theodb_hnsw` serve.

# Decisão D1 — adotar o LATERAL; não construir nó de join

O achado central, empírico: o teste roda `EXPLAIN (COSTS OFF, VERBOSE)` sobre o LATERAL, e o plano
real é

```
Nested Loop
  ->  Seq Scan on vja                        ← lado externo, o driver do LATERAL
  ->  Limit
        ->  Index Scan using vjb_idx on vjb  ← o ramo INTERNO é um Index Scan
              Order By: (vjb.emb <=> vja.emb) ← ordenado pelo operador de distância
```

O ramo interno é um **Index Scan ordenado** no índice — não um `Seq Scan` com `Sort` sobre o
produto cruzado. **O planner empurra o índice para dentro do LATERAL sem nenhum código novo.** O
plano imprime o nome do índice, não o do AM; a identidade é estrutural, já que um Index Scan
ordenado servindo `emb <=> a.emb` só existe porque o AM declara `amcanorderbyop = true`.

**Alternativa rejeitada:** um nó `CustomScan`/join que empurrasse o AM. Nível PhD — hook de
planner, geração de paths, estado de scan customizado, modelo de custo de join —, duplicando o que
LATERAL e `amcanorderbyop` já fazem. O próprio mantenedor do pgvector confirma que ainda seriam N
lookups de índice separados: nenhum ganho algorítmico, só complexidade acidental.

# Decisão D2 — rejeitar o helper `theodb.vector_join(...)`

O helper **não embarca**, por três razões:

1. **Falha o primeiro degrau da parcimônia** — "isto precisa existir?". O LATERAL cru resolve o
   caso de uso sem código novo; o helper é açúcar puro.
2. **Arriscaria o próprio pushdown que embrulha.** SQL dinâmico via `regclass` e `format()` pode
   derrotar a escolha de Index Scan que o LATERAL estático mantém — ou seja, o helper poderia
   entregar um caminho **mais lento que a coisa embrulhada**. Nunca embarcar um wrapper mais lento
   que o original.
3. **Adicionaria contrato público de SemVer** para zero ganho de capacidade — só de digitação.

O fallback adotado foi documentar o idioma (top-k, threshold, self-join de dedup) no relatório de
benchmark. O caso negativo (`τ < 0`) é o contrato documentado de "conjunto vazio" no SQL cru,
provado por teste — e não um erro tipado, que só existiria no helper rejeitado.[^adr0022]

# Consequência

Vector join first-class hoje, com recall herdado do próprio AM e preservado por construção. **Zero
código novo de produção:** o milestone é validação, medição e documentação —
[m63](/benchmarks/m63-vector-join.md).

# Débito honesto

O LATERAL faz N buscas independentes, sem compartilhar trabalho entre linhas externas próximas —
o gap de throughput conhecido. Um ANN-join amortizado é semente futura, rastreada como débito e só
com evidência.

[^adr0022]: ADR 0022 — M63 vector JOIN: LATERAL-index-scan, not a custom join node
