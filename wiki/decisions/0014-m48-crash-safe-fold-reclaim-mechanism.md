---
type: Decision
title: ADR 0014 — M48: fold crash-safe por meta-pivot, reclaim por região contígua (FSM rejeitado)
description: Elimina a corrupção silenciosa do VACUUM fold com pivot atômico do bloco 0; o reclaim reusa a região morta contígua em vez do FSM, com duas janelas residuais fail-loud.
resource: git:f7c7b93:docs/adr/0014-m48-crash-safe-fold-reclaim-mechanism.md
tags: [adr, crash-safety, vacuum, index-am, wal, m48]
adr_id: "0014"
adr_status: Accepted
decision_date: 2026-07-05
owner: human:paulohenriquevn
milestone: M48
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0014
    resource: git:f7c7b93:docs/adr/0014-m48-crash-safe-fold-reclaim-mechanism.md
    title: ADR 0014 — M48 fold crash-safe
    last_modified: 2026-07-16
---

Fecha o pior defeito já registrado no índice próprio: **corrupção silenciosa**, em que o scan
pontuava bytes obsoletos como se fossem vetores.

# O problema

O VACUUM fold reescrevia o índice **in-place, meta (bloco 0) primeiro**, com um registro
`GenericXLog` por página. `GenericXLog` **não tem atomicidade multi-registro**, então um crash no
meio do VACUUM deixava a meta nova apontando para páginas ainda com bytes da geração velha — no
pior caso, o scan pontuava bytes stale como vetores, produzindo **resultado silenciosamente
errado**.

# A descoberta que mudou o mecanismo

O blueprint recomendava dois componentes: (A) meta-pivot atômico, e (B) reclaim das páginas velhas
via **FSM**, com precedente upstream no GIN e no nbtree.

Ao implementar (B), descobriu-se que **o FSM não se aplica a este layout**: todos os leitores
assumem **ranges contíguos** — `read_chunked(first, npages)`, o directory IVF por cursor absoluto,
e a pending region como cauda `[pending_start, nblocks)`. O FSM devolve páginas **avulsas**, que
fragmentariam esses ranges. GIN e nbtree podem usar FSM porque suas páginas são **auto-contidas**
(uma página de posting list, uma página de B-tree, é válida sozinha). As nossas não são.

# Decisão

**(A) Meta-pivot atômico — aceita e completa.** O fold escreve a geração nova em páginas frescas —
inertes enquanto o bloco 0 apontar para a geração velha, na ordem dados-antes-pivot do GIN — e
pivota o bloco 0 **por último**, num único registro `GenericXLog` com `GENERIC_XLOG_FULL_IMAGE`,
à prova de torn page, seguindo o precedente do nbtree. Crash antes do pivot deixa a geração velha
íntegra; crash depois, a nova íntegra.

**(B) Reclaim — FSM rejeitado, região contígua aceita.** O reclaim reusa a região morta contígua
baixa `[1, cur_gen_start)` quando a geração nova cabe (alocador lowest-fit), senão estende no
tail; após o pivot, re-inicializa as páginas leftover como **vazias**, de modo que a pending region
leia limpo. Isso **limita o crescimento** — o índice para de crescer a partir do segundo fold,
alternando entre região baixa e alta — sem FSM.

## O limite honesto: duas janelas fail-loud

Ambas fecham totalmente apenas no [M55](/decisions/0017-m55-index-maintenance-at-scale.md):

1. **Crash no meio do reclaim**, entre o pivot e o fim do reinit, pode deixar bytes stale na
   pending range.
2. **Crash no meio do shadow-write quando o fold estende no tail** deixa páginas órfãs da geração
   nova *dentro* da pending range da geração velha.

Em ambas, `read_pending` valida o comprimento exato do item e **falha alto** — erro tipado levando
a REINDEX. A garantia é **"consistente OU fail-loud com REINDEX, nunca silenciosamente errado"**,
que é mais fraca que "sempre utilizável sem REINDEX". A distinção está declarada, não escondida.[^adr0014]

# Prova de crash — fechada

O teste ponta a ponta de crash-recovery real foi construído e roda: SIGABRT via GUCs de injeção
mais replay de WAL num cluster de verdade. Exercita os **3** pontos de crash do fold —
after-body-page, post-pivot e mid-reclaim — com **3 SIGABRT reais confirmados no log do
Postgres**, e um guard não-vacuoso que exige ao menos 3 crashes (senão o fold não disparou).

Veredito medido: crash **antes** do pivot deixa a geração antiga correta (índice pós-crash idêntico
ao rebuild limpo); crash **após** o pivot ou no meio do reclaim produz fail-loud REINDEX tipado,
nunca resultado errado.

# Notas de implementação

- **A recuperação da janela é REINDEX, não re-VACUUM.** Um re-VACUUM lê a *mesma* pending poluída e
  também falha alto; só o REINDEX cura. O teste originalmente planejado foi re-asserido para
  REINDEX — divergência plano↔realidade registrada.
- **O hook de injeção de crash é superuser-gated, por segurança.** `std::process::abort()` é
  *instance-wide*: o postmaster trata como crash e reinicia a instância inteira. Como o pgrx 0.16.1
  não força o `Suset` do GUC custom, um não-superuser poderia derrubar a instância. Os hooks
  retornam cedo se não for superuser.
- **Torn-pivot-page é segurança argumentada, não injetada.** Um crash *durante a escrita do próprio
  registro WAL do pivot* não tem ponto de injeção; a segurança vem do `GENERIC_XLOG_FULL_IMAGE`
  combinado com `full_page_writes=on`, herdado do precedente nbtree.

# Alternativas rejeitadas

**FSM** — fragmenta os ranges contíguos que todos os leitores assumem; precedente válido para
páginas auto-contidas, inaplicável aqui. **Manutenção in-place à la pgvector** — tombstones,
reparo de vizinhos, 4 passes e máquina de versão; é reescrita grande, e vira o escopo do M55.
**Tail-append puro sem reclaim** — o índice cresceria ~2× por fold sem limite. **Truncate do tail** —
a região morta é *baixa* após um tail-append, então truncate não a alcança.

# Consequências de formato

A meta estruturada do IVF migrou para **v3** (campo `gen_base`); v2 continua legível, com
`gen_base` implícito, e migra no primeiro fold. O HNSW não muda de formato.

[^adr0014]: ADR 0014 — M48 fold crash-safe: meta-pivot + reclaim por região contígua
