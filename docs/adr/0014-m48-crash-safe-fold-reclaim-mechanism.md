# ADR 0014 — M48 fold crash-safe: meta-pivot + reclaim por região contígua (FSM rejeitado), janela residual → M55

**Status:** Accepted · **Data:** 2026-07-05 · **Deciders:** paulohenriquevn (adjudicação da troca de mecanismo em /implement) · **Milestone:** M48
**Relacionado:** issues #46, #47; `ROADMAP.md § M48` / `§ M55`; blueprint `knowledge-base/discoveries/blueprints/m48-am-crash-safety-blueprint.md` (§Q1/§Q3/§Q4)
**Evidência:** `theodb_rs/src/am/fold.rs`, `benchmarks/tests/test_am_maintenance.py` (fold-preserves + reclaim size-stable), `benchmarks/tests/test_am_crash.py` (#46)

## Contexto e problema

O VACUUM fold do índice vetorial reescrevia o índice **in-place, meta (bloco 0) primeiro**, um registro
`GenericXLog` por página. `GenericXLog` não tem atomicidade multi-registro, então um crash no meio do VACUUM
deixava a meta nova apontando para páginas com bytes da geração velha — pior caso, o scan pontuava bytes
stale como vetores (resultado silenciosamente errado). Este é o **issue #47**.

O blueprint (SHIPPABLE 99.7) recomendou dois componentes: **(A)** meta-pivot atômico (shadow-write + pivot
bloco 0 por último, full-image) para a correção do #47; e **(B)** reclaim das páginas da geração velha via
**FSM** (`RecordFreeIndexPage` / `GetFreeIndexPage` / `IndexFreeSpaceMapVacuum`), com o precedente upstream
GIN (`ginfast.c:667-668`) e nbtree (`nbtpage.c:868-988`).

Ao implementar (B), descobriu-se que o FSM **não se aplica ao nosso layout**: todos os readers assumem
**ranges contíguos** — `read_chunked(first, npages)`, o directory IVF por cursor absoluto
(`page.rs` `structured_page_items`), e a pending region como cauda `[pending_start, nblocks)`. O FSM devolve
páginas **avulsas**, que fragmentariam esses ranges. GIN/nbtree usam FSM porque suas páginas são
**auto-contidas** (uma página de posting list / uma página de índice B-tree é válida sozinha); as nossas não.

## Decisão

**(A) Correção do #47 — ACEITA e completa:** meta-pivot atômico. O fold escreve a geração nova em páginas
frescas (inertes enquanto o bloco 0 aponta para a geração velha — ordem GIN dados-antes-pivot) e pivota o
bloco 0 por ÚLTIMO, num único registro `GenericXLog` com `GENERIC_XLOG_FULL_IMAGE` (torn-page-proof, precedente
nbtree meta-full-record). Crash antes do pivot ⇒ geração velha íntegra; crash depois ⇒ geração nova íntegra.
**Este é o núcleo do #47 e está fechado.**

**(B) Reclaim — FSM REJEITADO; região contígua ACEITA (com limite honesto):** o reclaim reusa a **região morta
contígua baixa** `[1, cur_gen_start)` quando a geração nova cabe (alocador puro `free_region`, lowest-fit),
senão estende no tail; após o pivot, re-inicializa as páginas leftover `[gen_end, nblocks)` como **vazias**, de
modo que a pending region leia limpo (0 entradas). Isso **bounda o crescimento** (o índice para de crescer a
partir do 2º fold, alternando low/high) sem FSM.

**Limite honesto (adjudicado):** o reclaim NÃO é totalmente atômico. Um crash **no meio do passo de reclaim**
(entre o pivot e o fim do reinit-leftover) pode deixar bytes stale na pending range. `read_pending`
**falha-alto** (erro tipado → REINDEX) sobre página stale — então a janela é **fail-loud, nunca corrupção
silenciosa**. Isso é uma restrição consciente do DoD do #47: a garantia "crash em qualquer ponto deixa o índice
**consistente e utilizável**" vale para o **pivot** (A); para o **reclaim** (B) a garantia é "consistente **OU**
fail-loud com REINDEX". O fechamento total (reclaim atômico sem janela REINDEX) é **manutenção incremental
crash-safe = M55** (`ROADMAP § M55` — "fold incremental vs in-place", pré-requisito de claim v1.0).

## Alternativas consideradas

- **FSM (recomendação original do blueprint §Q4)** — REJEITADA: fragmenta os ranges contíguos que todos os
  readers assumem (`read_chunked`, dir-cursor, pending-cauda). Precedente core válido para páginas
  auto-contidas (GIN/nbtree), inaplicável ao layout chunked.
- **Manutenção in-place à la pgvector (`hnswvacuum.c`)** — REJEITADA para o M48: tombstones + repair de vizinhos
  + 4 passes + máquina de versão (blueprint §Q5). É uma reescrita grande; é o escopo do M55.
- **Tail-append puro sem reclaim (T2.1)** — REJEITADO como estado final: o índice cresceria ~2× por fold sem
  limite. Mantido só como o passo intermediário do T2.1; T2.2 adiciona o reclaim.
- **Truncate do tail após relocar a geração baixa** — REJEITADO: a região morta é BAIXA (não o tail) após um
  tail-append, então truncate não a alcança; e relocar-baixo-e-truncar-alto tem a MESMA janela de crash.

## Consequências

- **Habilita:** #47 fechado (corrupção eliminada); crescimento do índice boundado; zero dependência de FSM.
- **Restringe:** o reclaim tem uma janela de crash fail-loud (REINDEX) — assumida até o M55. O tamanho
  estabiliza mas não encolhe ao mínimo (alternância low/high ⇒ ~2 gerações + pending).
- **Formato:** meta structured do IVF migrada para **v3** (campo `gen_base`); v2 continua legível (gen_base
  implícito = 1) e migra no primeiro fold. HNSW não muda de formato (elem_first/nbr_first já são pointers).
- **Prova pendente (dependência T2.2 → T2.3):** o teste que exercita o crash **no meio do reclaim** e prova
  o fail-loud é o T2.3 (crash-injection GUC). Até lá, "reclaim crash-safe fail-loud" é DEVIDO, não fechado.

## Rules consumidas

Regra 9 (não reinventar — meta-pivot ancorado em GIN/nbtree; FSM rejeitado por inaplicabilidade, não por
preguiça), Regra 8 (fail-fast/fail-loud — read_pending typed error), Regra 3 (honestidade — a janela residual
é declarada aqui e no CHANGELOG, não escondida), `discover-phd-rigor` (o mecanismo diverge do blueprint com
justificativa registrada).
