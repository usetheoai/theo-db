---
slug: columnar-toast-materialize
created_at: 2026-07-24
goal: Eliminar a dependência de snapshot no flush do theodb_columnar materializando valores TOAST na ingestão, provado por dois INSERTs sucessivos de 5.000 linhas com coluna > 2 KB retornando count(*) = 10.000
---

# Plan: Materializar TOAST na ingestão do `theodb_columnar` (#190)

**Versão:** v1.1 (MUST FIX do edge-case review absorvidos) · **Issue:** [#190](https://github.com/usetheodev/theo-db/issues/190) · **Data:** 2026-07-24

## Goal

Eliminar a dependência de snapshot ativo no flush do `theodb_columnar` materializando os valores TOAST no
momento da ingestão, provado por **dois INSERTs sucessivos de 5.000 linhas cada, numa tabela com coluna
`text` acima de 2 KB, retornando `count(*) = 10.000`** (hoje o segundo INSERT aborta).

## Context

O gate de escala do ClickBench (`docs/benchmarks/clickbench-scale-gate-2026-07-24.md`) foi bloqueado por um
defeito que impede **qualquer carga real** no armazenamento colunar:

```
INSERT INTO hits SELECT * FROM hits_heap
ERROR:  cannot fetch toast data without an active snapshot
```

O defeito estava latente havia meses e só apareceu quando a amostragem do benchmark foi corrigida para
percorrer o dataset inteiro (antes usava as primeiras N linhas — uma fatia temporal sem valores largos o
bastante para gerar TOAST externo).

O usuário determinou explicitamente: **"SEMPRE A CORREÇÃO MAIS ROBUSTA POSSÍVEL"** — o que descarta o
paliativo de envolver o flush num snapshot emprestado e exige remover a dependência na origem.

## Baseline Context (deep review of current state)

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Por que existe |
|---|---|---|---|
| `theodb_rs/src/am/columnar.rs` | **1909** | `8df9781` (2026-07-23) | O TableAM colunar own-code do M99: MVCC-via-catálogo-heap + armazenamento column-major |
| `theodb_rs/isolation/columnar_toast_check.sh` | (NEW) | — | Harness de regressão in-PG do #190 |

Orçamento de tamanho: `columnar.rs` está em 1909 LoC, **acima** do teto de 500 LoC de
`.claude/rules/architecture.md`. Este plano **não pode aumentar** o arquivo de forma significativa; a
tarefa T1.2 extrai a rotina compartilhada, reduzindo duplicação (três cópias do mesmo laço `deform` +
`extract` hoje).

### Current callers / dependents

Levantado por `grep -n` em `theodb_rs/src/am/columnar.rs`:

| Símbolo | Call-sites (file:line) | Contexto de snapshot |
|---|---|---|
| `flush_pending` | `:206` (callback `XACT_EVENT` pré-commit) | ❌ **sem snapshot ativo** — a causa-raiz |
| `flush_pending` | `:1159` (mid-executor, buffer > `maintenance_work_mem`) | ✅ snapshot do INSERT |
| `flush_pending` | `:1199` | a confirmar em T1.1 |
| `flush_pending` | `:1526` (função de teste `theodb_columnar_test_stripe_info`) | contexto de teste |
| `extract_value_bytes` (faz o detoast) | `:822`, `:956`, `:1257` | os três laços `deform`+`extract` |
| `with_active_snapshot` | `:121` (`read_visible_stripes`), `:156` (`insert_stripe_row`) | já protegidos |
| `accumulate_row` | ingestão, `:1135` | ✅ snapshot do executor garantido |

**Nenhum consumidor fora do crate:** `flush_pending`, `accumulate_row` e `extract_value_bytes` são
`unsafe fn` privadas do módulo (sem `pub`), então a mudança é interna ao `am/columnar.rs`.

### Domain glossary

- **TOAST** — mecanismo do PostgreSQL que move valores acima de ~2 KB para uma tabela auxiliar, deixando na
  tupla um ponteiro de 18 bytes (`varatt_external`). Ler o valor exige buscar na toast table, o que **exige
  um snapshot ativo** para resolver visibilidade.
- **Datum externo vs inline-comprimido** — TOAST tem duas formas: comprimido *dentro* da tupla (não precisa
  de snapshot) e **externo** (precisa). Isso explica por que INSERTs pequenos passavam.
- **Stripe** — unidade de escrita do colunar: N linhas codificadas em chunks por coluna + uma linha de
  catálogo escrita **por último** (a ordem que garante atomicidade).
- **Pending write state** — buffer por-backend (`WRITE_STATES`) que acumula linhas como bytes de heap-tuple
  até o flush.
- **Flush** — converte o buffer em um stripe persistido. Ocorre em dois momentos: mid-executor (por
  `maintenance_work_mem`) e no pré-commit.

### Architecture boundaries affected

Por `.claude/rules/architecture.md`: a mudança é **interna à camada de infraestrutura** (o TableAM é um
adaptador sobre o contrato de storage do PostgreSQL). Não cruza fronteiras, não altera API pública SQL,
não muda formato on-disk do stripe. O `.claude/rules/error-handling.md` se aplica: o caminho de falha deve
levantar erro **tipado e claro**, nunca silencioso.

## Prior Art & Related Work

- **Blueprint interno:** `.claude/knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md`
  — precedente metodológico de diagnóstico do mesmo módulo (gdb backtrace no backend travado, M131/#135).
- **Precedente interno direto (M99):** o próprio arquivo já resolve este contrato duas vezes com
  `with_active_snapshot` (`columnar.rs:121`, `:156`) — a existência dessa função é a evidência de que o
  problema de snapshot já foi enfrentado neste módulo, só não no caminho de flush.
- **Contrato do PostgreSQL:** destoastar exige snapshot ativo; a alternativa canônica é *achatar* a tupla
  na ingestão, o mesmo princípio que o `heap_toast_insert_or_update` do heap aplica ao gravar.
- **Nenhum `*-patterns` skill existe neste repositório** (verificado: `ls .claude/skills/*-patterns/` vazio),
  então não há pattern de domínio a citar ou sobrepor.

## Objective

Tornar o `flush_pending` **independente de contexto de snapshot** — puro processamento de bytes já
materializados — de modo que funcione identicamente no executor, no pré-commit ou em qualquer callback
futuro.

## ADRs

### ADR-1 — Materializar TOAST na ingestão, não emprestar snapshot no flush

**Decisão:** detoastar os valores em `accumulate_row`, antes de formar a heap-tuple que vai para o buffer.
O buffer passa a conter apenas dados materializados; `flush_pending` nunca mais toca a toast table.

**Rationale:** o usuário exigiu a correção mais robusta. Materializar na origem **elimina** a dependência
em vez de contorná-la, e traz três garantias que o paliativo não dá:

1. **Independência de contexto** — o flush funciona em qualquer callback, presente ou futuro. Um novo
   call-site não reintroduz o bug.
2. **Imunidade a VACUUM** — um ponteiro TOAST guardado num buffer de transação longa pode apontar para um
   chunk já removido. Materializar na ingestão fecha essa janela; emprestar snapshot no flush, não.
3. **Correção da semântica** — o valor gravado passa a ser o que existia no instante do INSERT, que é o que
   a transação enxergava.

Cita `.claude/rules/parsimony-ladder.md` (rung 3: usar o recurso nativo da plataforma — o próprio mecanismo
de detoast do PostgreSQL, na posição correta) e a Regra 8 de error-handling (falhar cedo, na fronteira).

**Alternativa rejeitada — envolver `flush_pending` em `with_active_snapshot`:** é a correção de uma linha e
tentadora, mas (a) no pré-commit `GetTransactionSnapshot()` pode não ser apropriado ou válido, (b) não
resolve a janela do VACUUM, e (c) mantém a fragilidade latente para todo call-site futuro. Contraria a
diretriz explícita de robustez máxima.

**Alternativa rejeitada — proibir colunas TOASTáveis no colunar:** inviabilizaria o ClickBench (o `hits`
tem `url` de 3.951 bytes) e qualquer carga real com texto.

### ADR-2 — Unificar os três laços `deform` + `extract` numa rotina só

**Decisão:** extrair a sequência `heap_deform_tuple` → `extract_value_bytes` (hoje duplicada em `:822`,
`:956`, `:1257`) para uma função única.

**Rationale:** DRY sobre conhecimento, não sobre linhas — as três cópias implementam a **mesma regra**
(como converter uma heap-tuple bufferizada em colunas de bytes). Se a materialização de TOAST for aplicada
a apenas uma cópia, as outras duas guardam a inconsistência. Além disso, `columnar.rs` está em 1909 LoC,
muito acima do teto de 500 de `architecture.md`; consolidar reduz o débito em vez de aumentá-lo.

**Alternativa rejeitada — corrigir só o sítio `:1257`:** deixaria dois caminhos com a semântica antiga, e o
próximo leitor não teria como saber qual é o correto.

### ADR-3 — Provar a correção com harness in-PG, não com `cargo test`

**Decisão:** o teste de regressão é um script em `theodb_rs/isolation/` executado contra um PostgreSQL real,
e um job no CI que o executa.

**Rationale:** convenção estabelecida do crate — `cargo test`/`cargo pgrx test` não linkam neste projeto
(símbolos PG indefinidos), e todo gate de correção do módulo (crash-safety, isolamento) já vive em
`isolation/`. Cita `.claude/rules/testing.md` § 5 (convenção de pareamento por projeto).

**Alternativa rejeitada — teste unitário Rust puro:** não exercita o caminho TOAST, que só existe com um
PostgreSQL real gravando na toast table.

## Drawbacks & Risks

| # | Risco | Severidade | Mitigação | Dono |
|---|---|---|---|---|
| R1 | **Consumo de RAM sobe.** Materializar troca ponteiros de 18 bytes por valores inteiros no buffer. Uma tabela com muitos valores de 3 KB pode multiplicar o uso de memória do pending state. | **ALTA** | O flush incremental por `maintenance_work_mem` (`:1159`) já limita o buffer — mas ele mede `bytes` acumulados, e a contabilidade precisa passar a refletir o tamanho **materializado**, senão o teto é furado. T1.3 cobre isso explicitamente. | implementador |
| R2 | **Regressão de throughput na ingestão.** Detoastar por linha na ingestão move custo do flush para o INSERT. | MÉDIA | Medir INSERT antes/depois no mesmo box (T3.1). O custo total não deveria crescer — o detoast acontecia de qualquer forma, só mais tarde. Se crescer > 20%, reavaliar. | implementador |
| R3 | **A relação pode ficar ilegível após abort** (`could not read blocks 119..119`), observado uma vez no gate. Pode ser um **segundo defeito** independente. | **ALTA** | T2.1 investiga separadamente. Se confirmado, é issue própria — não deve ser silenciosamente "resolvido" por este plano sem prova. | implementador |
| R4 | Consolidar os três laços (ADR-2) pode introduzir regressão em caminhos de leitura (`:822`, `:956`) que hoje funcionam. | MÉDIA | A rotina extraída preserva comportamento byte-a-byte; T1.2 exige A/B de leitura antes/depois numa tabela existente. | implementador |

## Unresolved Questions

- Q1 — **[RESOLVIDA na T2.1]** O estado ilegível pós-abort é um **defeito INDEPENDENTE** (#191), não o #190.
  O repro mínimo (`TRUNCATE` + re-INSERT, SEM TOAST, 5.000 linhas) reproduz `bad metapage magic 0x​fd2fb528`
  + `could not read blocks` **também no binário pré-#190** (`9940da0`, build A/B) — logo não é regressão do
  fix. É um bug de TRUNCATE do TableAM colunar, mascarado pelo #190 (a carga abortava antes). Aberto como
  #191. Original:</p>**O estado ilegível pós-abort (R3) é o mesmo defeito ou outro?** Observado uma vez, com `TRUNCATE` +
   INSERT repetidos. T2.1 existe para responder com repro determinístico. Se for independente, vira issue
   separada — não será encoberto pelo fix.
- Q2 — **O call-site `:1199` está em qual contexto de snapshot?** Não foi classificado no levantamento; T1.1
   deve determiná-lo antes de qualquer alteração, porque muda o alcance da correção.
- Q3 — **TOAST aninhado dentro de tipo composite (EC-7).** Um `ROW(text, text)` pode ter campos internos
   toastados que `pg_detoast_datum_copy` no valor externo não resolve. É raro (exige coluna composite numa
   tabela colunar) e o ClickBench não usa. **Decisão:** declarado como limitação conhecida, fora do escopo
   do #190; se aparecer em uso real, vira issue própria. Fixá-lo aqui seria scope creep.
- Q4 — **Qual o teto real de RAM aceitável para o pending state?** Hoje `maintenance_work_mem`. Se a
   materialização estourar esse orçamento em cargas reais, pode ser necessário reduzir o gatilho de flush —
   decisão que depende da medição de T3.1.

## Dependency Graph

```
Fase 1 (correção)  →  Fase 2 (investigação do R3)  →  Fase 3 (validação em escala)
   T1.1 → T1.2 → T1.3 → T1.4          T2.1                  T3.1 → T3.2
```

T1.1 (RED) bloqueia todo o resto. T1.2 (consolidação) precede T1.3 (materialização) para que a mudança
seja aplicada num ponto só. Fase 2 é independente da Fase 1 e pode correr em paralelo. Fase 3 exige Fase 1
completa.

## Phase 1: Correção da dependência de snapshot

### T1.1 — Harness de regressão que reproduz o #190 (RED)

#### Objective
Um script in-PG que falha hoje com `cannot fetch toast data without an active snapshot` e passa após o fix.

#### Why this step (action + reasoning)
**Ação:** escrever `theodb_rs/isolation/columnar_toast_check.sh` reproduzindo os dois INSERTs sucessivos.

**Raciocínio:** a Regra 7 (`.claude/rules/testing.md`) exige teste de regressão **antes** do fix — sem ele
não há prova de que o defeito existia nem de que sumiu. O repro mínimo já está caracterizado no #190 (dois
lotes de 5.000 com coluna > 2 KB), então o custo é baixo e o valor é o gate de todo o plano.

#### Evidence
Issue #190 e `docs/benchmarks/clickbench-scale-gate-2026-07-24.md`: `LIMIT 1000` passa, segundo INSERT de
5.000 falha, `max(octet_length(url)) = 3951`.

#### Files to edit
- `theodb_rs/isolation/columnar_toast_check.sh` **(NEW)**
- `theodb_rs/isolation/Makefile` — alvo `check-toast` (um harness sem chamador é prova que ninguém executa)

#### Deep file dependency analysis
O `Makefile` de `isolation/` já hospeda `check-isolation`, `check-crash`, `check-corrupt`, `check-compat`.
O novo alvo segue o mesmo padrão; nenhum outro arquivo depende dele.

#### TDD
- **RED:** rodar o harness contra o binário atual → sai **1** com `cannot fetch toast data`.
- **GREEN:** após T1.3, sai **0** com `count(*) = 10000`.
- **Não-vacuidade:** reverter T1.3 e confirmar que o harness volta a falhar.

#### Concurrency tests
`(none — single-threaded)`. O repro é sequencial num único backend; o buffer `WRITE_STATES` é
**por-backend** (thread-local), então não há estado compartilhado entre sessões neste caminho.

#### Acceptance Criteria
- O harness cria tabela colunar com coluna `text`, insere dois lotes de 5.000 com valores > 2 KB.
- Asserta `count(*) = 10000` — não apenas "não deu erro".
- Asserta que os **valores** sobrevivem (comparação de `md5(string_agg(...))` contra a origem heap), não só
  a contagem: um fix que grave lixo passaria num teste de contagem. **Esta asserção é a defesa principal
  contra EC-2/EC-3** (corrupção silenciosa), não um detalhe do teste.
- **EC-6 (SHOULD TEST) — os dois lotes em transações DIFERENTES.** O repro do #190 usa dois INSERTs na
  mesma transação, mas o caminho sem snapshot é o **pré-commit** (`columnar.rs:206`), que só roda no COMMIT.
  Um teste sem commit entre os lotes pode passar sem exercitar o caminho que originou o defeito. O harness
  cobre os dois arranjos: (a) dois INSERTs na mesma txn, (b) INSERT+COMMIT × 2.
- **EC-4 (SHOULD TEST) — limiar de TOAST.** Valores de 1.900, 2.000 e 2.100 bytes na mesma tabela,
  assertando count e md5. O limiar depende do tamanho total da tupla, não só da coluna; testar só com 3 KB
  não distingue "materializou tudo" de "materializou só o externo" — os três estados (inline, comprimido
  inline, externo) precisam passar.
- **EC-5 (SHOULD TEST) — tipos de tamanho fixo intactos.** Tabela com `int`, `bigint`, `uuid`, `char(10)` e
  `text`, assertando que os quatro primeiros trafegam sem alteração. A materialização de T1.3 usa o mesmo
  critério `attlen == -1` de `extract_value_bytes:535`; divergir aí quebra o que o flush espera.
- Qualquer violação produz exit **1** com uma linha `TOAST_CHECK_FAIL <motivo>` no stdout.

#### DoD
`make -C theodb_rs/isolation check-toast` sai 1 hoje (RED) e 0 após o fix, com não-vacuidade demonstrada.

### T1.2 — Consolidar os três laços `deform` + `extract` (ADR-2)

#### Objective
Uma única rotina converte heap-tuple bufferizada em colunas de bytes; os três sítios passam a chamá-la.

#### Why this step (action + reasoning)
**Ação:** extrair o laço de `:822`, `:956` e `:1257` para uma função com assinatura única.

**Raciocínio:** a materialização de TOAST (T1.3) precisa ser aplicada num ponto só; com três cópias, duas
ficariam com a semântica antiga. É DRY sobre conhecimento (a regra de conversão), não sobre linhas — e
reduz o débito de tamanho de `columnar.rs` (1909 LoC vs teto de 500 em `architecture.md`).

#### Evidence
`grep -n "heap_deform_tuple" theodb_rs/src/am/columnar.rs` → `:817`, `:947`, `:1252`, cada um seguido de um
laço `isnull ? None : extract_value_bytes(...)` idêntico em forma.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` (`:817-824`, `:947-958`, `:1252-1260`)

#### Deep file dependency analysis
Os três sítios diferem no conjunto de colunas (`wanted` em `:822`, coluna única em `:956`, todas em
`:1257`). A rotina extraída precisa aceitar a **seleção de colunas** como parâmetro para cobrir os três.

#### TDD
- **RED:** um A/B de leitura numa tabela colunar existente (antes da mudança) captura os bytes lidos;
  após a extração, os bytes devem ser **idênticos**.
- **GREEN:** A/B byte-idêntico nos três caminhos.
- **REFACTOR:** confirmar redução líquida de linhas.

#### Concurrency tests
`(none — single-threaded)`.

#### Acceptance Criteria
- `grep -c "heap_deform_tuple" theodb_rs/src/am/columnar.rs` retorna **≤ 3** (era 5).
- `md5` do `string_agg` de um `SELECT *` sobre tabela colunar pré-existente é **idêntico** antes e depois da extração (0 bytes de diferença).
- `make -C theodb_rs/isolation check-isolation check-crash` sai **0** (sem regressão nos gates existentes).

#### DoD
Build limpo, A/B de leitura byte-idêntico, `columnar.rs` com menos linhas do que antes.

### T1.3 — Materializar TOAST na ingestão (ADR-1) — o fix

#### Objective
`accumulate_row` passa a gravar valores materializados; `flush_pending` deixa de depender de snapshot.

#### Why this step (action + reasoning)
**Ação:** em `accumulate_row` (`columnar.rs:1135`), detoastar os valores do slot **antes** do
`heap_form_tuple`, de modo que os bytes bufferizados nunca contenham ponteiro TOAST externo.

**Raciocínio:** é a decisão da ADR-1 — remover a dependência na origem, onde o snapshot do executor é
garantido por construção, em vez de emprestar um snapshot no flush. Fecha também a janela do VACUUM (R1 da
issue) e imuniza call-sites futuros.

#### Evidence
`accumulate_row` (`:1135-1141`) faz `slot_getallattrs` + `heap_form_tuple` + copia `t_data` para o buffer.
Um `varatt_external` sobrevive essa cópia como ponteiro de 18 bytes; `extract_value_bytes` (`:534`, ramo
`None` → `pg_detoast_datum_copy`) é quem tenta resolvê-lo depois, já fora do snapshot.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` (`accumulate_row`, `:1134-1151`)

#### Deep file dependency analysis
`accumulate_row` é chamada pelo caminho de `tuple_insert`. Materializar ali afeta **todo** INSERT no
colunar — inclusive os que hoje funcionam. Por isso T1.2 vem antes (ponto único) e o A/B de T1.2 protege os
caminhos de leitura.

#### Pseudo-code / Signatures
```
accumulate_row(rel, slot):
    slot_getallattrs(slot)
    para cada atributo i:
        se isnull[i]                 -> pular (detoast de NULL é segfault, columnar.rs:1246)
        se attlen != -1              -> pular (mesmo critério de extract_value_bytes:535; um
                                        datum by-value tratado como ponteiro é segfault)
        senão (varlena):
            materializar SEMPRE com pg_detoast_datum_copy — EC-2
    // ORDEM OBRIGATÓRIA (EC-3): formar a tupla ANTES de liberar as cópias
    heap_form_tuple(tupdesc, valores_materializados, isnull)   // copia os valores
    bytes = copiar t_data                                       // copia de novo, para o Vec
    pfree(cada cópia materializada); heap_freetuple(htup)       // só agora é seguro liberar
```

**EC-2 (MUST FIX do edge-case review) — materializar TODO varlena, sem pré-teste de
`VARATT_IS_EXTERNAL`.** Arrays chegam do executor em *expanded form*
(`VARATT_IS_EXTERNAL_EXPANDED`): um ponteiro para estrutura em memória, não um varlena serializado.
`grep -rn "EXPANDED\|flatten" theodb_rs/src/` retorna **zero** ocorrências — o crate nunca tratou o caso.
Hoje funciona por acidente porque `pg_detoast_datum_copy` achata expanded. Se filtrarmos por
`VARATT_IS_EXTERNAL`, o expanded escapa da materialização e o buffer guarda ponteiro para memória do
contexto per-tuple já resetado: **use-after-free, pior que o bug original**. `pg_detoast_datum_copy`
cobre external, compressed e expanded — usar sempre.

**EC-3 (MUST FIX) — ordem de liberação.** `pg_detoast_datum_copy` aloca no `CurrentMemoryContext`, que em
`accumulate_row` é o per-tuple do executor. Liberar antes do `heap_form_tuple` copiar produz bytes a partir
de memória liberada — **corrupção silenciosa**, sem erro algum. A ordem acima é obrigatória, e a asserção
de md5 em T1.1 é o oráculo que a protege.

#### TDD
- **RED:** `check-toast` de T1.1 falhando.
- **GREEN:** `check-toast` passa com `count(*) = 10000` e md5 dos valores igual à origem.
- **REFACTOR:** garantir que nenhuma cópia materializada vaze (o `pg_detoast_datum_copy` sempre aloca).

#### Concurrency tests
`(none — single-threaded)`. `WRITE_STATES` é por-backend; a materialização não introduz estado compartilhado.

#### Acceptance Criteria
- `make -C theodb_rs/isolation check-toast` sai **0**; revertendo o diff de T1.3 o mesmo comando sai **1** (não-vacuidade).
- `md5(string_agg(col ORDER BY id))` da tabela colunar é **igual** ao da origem heap (comparação exata, não amostrada).
- Após INSERT de 100.000 linhas, o RSS do backend fica **abaixo de 2× `maintenance_work_mem`** (medido com `ps -o rss= -p <backend>` antes e depois).
- `grep -c "snapshot-safe because" theodb_rs/src/am/columnar.rs` retorna **0** (o comentário afirma uma garantia inexistente no pré-commit).

#### DoD
`make -C theodb_rs/isolation check-toast` sai 0; suíte `check-isolation` e `check-crash` continuam verdes.

### T1.4 — Contabilidade de bytes reflete o tamanho materializado (R1)

#### Objective
O gatilho de flush incremental passa a medir o tamanho real bufferizado.

#### Why this step (action + reasoning)
**Ação:** garantir que `p.bytes` (`:1148`) some o tamanho **materializado**, e verificar que o teto de
`maintenance_work_mem` continua sendo respeitado.

**Raciocínio:** R1 é o risco de maior severidade deste plano. Se a contabilidade continuar medindo o
ponteiro de 18 bytes enquanto o buffer guarda 3 KB, o teto de RAM é furado por ~170× em cargas com TOAST —
trocaríamos um bug de correção por um de esgotamento de memória.

#### Evidence
`:1147-1150` — `p.bytes += bytes.len()`. Com a mudança de T1.3, `bytes` já será maior; a tarefa é
**verificar** e cobrir com teste, não presumir.

**EC-1 (MUST FIX do edge-case review) — a verificação de teto está na ordem errada.** `columnar.rs:1145-1152`
empurra a linha no buffer **e só então** compara `pending_bytes > mwm`. Hoje isso é inofensivo porque um
valor grande entra como ponteiro de 18 bytes. Depois de T1.3 ele entra **materializado**: um `text` de
512 MB (varlena aceita até 1 GB) é copiado inteiro antes de qualquer verificação, e o flush só ocorreria na
linha seguinte. Trocaríamos um erro de INSERT por **OOM do backend** — piora, não correção.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` (`:1144-1151`)
- `theodb_rs/isolation/columnar_toast_check.sh` — asserção adicional

#### Tasks
1. Inverter a ordem: se `p.bytes + bytes.len() > mwm` **e** `!p.rows.is_empty()`, fazer flush **antes** de
   empurrar a linha nova. Uma linha isoladamente maior que o teto ainda passa (não há como parti-la), mas
   nunca soma com as anteriores.
2. Garantir que `p.bytes` some o tamanho materializado (consequência natural de T1.3 — verificar).

#### TDD
- **RED:** com `maintenance_work_mem` baixo (1 MB) e valores de 3 KB, contar stripes gerados; se a
  contabilidade estiver errada, sai **1 stripe** (buffer estourou o teto sem flush).
- **GREEN:** múltiplos stripes, cada um respeitando o teto.
- **RED (EC-1):** uma única linha com valor de ~64 MB e `maintenance_work_mem = '1MB'`; com a ordem antiga o
  RSS do backend salta para dezenas de MB antes de qualquer flush.

#### Concurrency tests
`(none — single-threaded)`.

#### Acceptance Criteria
- Com `maintenance_work_mem = '1MB'` e 5.000 linhas de ~3 KB, o número de stripes é > 1.
- RSS do backend permanece na ordem de `maintenance_work_mem`, não de linhas × 3 KB.
- **EC-1:** inserir 10 linhas de ~8 MB com `maintenance_work_mem = '1MB'` gera ≥ 10 stripes e o RSS não
  acumula as 10 linhas simultaneamente.

#### DoD
Asserção de contagem de stripes verde no harness.

## Phase 2: Investigação do estado ilegível pós-abort (R3)

### T2.1 — Determinar se o estado corrompido é defeito independente

#### Objective
Repro determinístico (ou refutação) do `could not read blocks 119..119` após INSERT abortado.

#### Why this step (action + reasoning)
**Ação:** tentar reproduzir com `TRUNCATE` + INSERT abortado, N vezes, verificando legibilidade após cada
abort.

**Raciocínio:** foi observado **uma vez** e marcado `[NEEDS-REPRO]` no #190. Se o abort deixa metadados
apontando para blocos não materializados, isso é **corrupção**, mais grave que o erro de INSERT — e não
pode ser dado como resolvido só porque o INSERT parou de falhar. Honestidade (Regra 3): um defeito
observado e não investigado é dívida oculta.

#### Evidence
`docs/benchmarks/clickbench-scale-gate-2026-07-24.md` § Estado pós-falha: `count(*)` retornou
`could not read blocks 119..119 in file "base/5/27074": read only 0 of 8192 bytes` após o abort.

#### Files to edit
- `theodb_rs/isolation/columnar_toast_check.sh` — cenário de abort
- Issue nova, **se** confirmado independente

#### TDD
- **RED/GREEN condicional:** se reproduzir com o fix de T1.3 aplicado, é defeito independente → issue nova
  + teste próprio. Se não reproduzir, documentar que era consequência do abort do INSERT.

#### Concurrency tests
`(none — single-threaded)`.

#### Acceptance Criteria
- 10 ciclos de `abort` + `SELECT count(*)` executados; a contagem de ciclos que falharam na leitura é **registrada no relatório** (0 = não reproduz).
- O relatório declara **um** dos dois: `INDEPENDENTE` (com número de issue aberta) ou `CONSEQUÊNCIA` (com o log do abort que o explica).

#### DoD
Conclusão documentada com evidência; nenhuma afirmação sem repro.

## Phase 3: Validação em escala

### T3.1 — Medir custo da materialização na ingestão (R2)

#### Objective
Comparar throughput de INSERT antes/depois num mesmo box.

#### Why this step (action + reasoning)
**Ação:** medir tempo de carga de 1M linhas do `hits` antes e depois do fix, no mesmo droplet efêmero.

**Raciocínio:** R2 prevê que o custo do detoast se desloca do flush para a ingestão sem crescer no total —
mas isso é hipótese, não medição. `.claude/rules/public-copy.md` § 4 e a regra 5 do `CLAUDE.md` exigem
benchmark para qualquer afirmação de performance.

#### Evidence
O gate anterior carregou 1M linhas no heap com sucesso, então há baseline de carga disponível.

#### Files to edit
- `docs/benchmarks/columnar-toast-materialize.md` **(NEW)**

#### Concurrency tests
`(none — single-threaded)`. A medição roda um INSERT por vez num box dedicado; concorrência introduziria
ruído na comparação antes/depois, que é justamente o que a tarefa mede.

#### Acceptance Criteria
- Tempo de INSERT de 1M linhas medido antes e depois, mesmo box, ≥ 3 repetições.
- Se `tempo_depois / tempo_antes > 1.20`, o artefato registra `REAVALIAR_ADR1` explicitamente (a regressão não pode ser omitida).

#### DoD
Artefato de benchmark commitado com metodologia e comando de reprodução.

### T3.2 — Destravar o gate de escala do ClickBench

#### Objective
O run de 1M linhas com `--sample systematic --agg` completa as 43 queries.

#### Why this step (action + reasoning)
**Ação:** repetir o gate bloqueado em `docs/benchmarks/clickbench-scale-gate-2026-07-24.md` num droplet efêmero.

**Raciocínio:** é a validação de que o fix resolve o problema **real** que o originou, não apenas o repro
sintético. Fecha o ciclo aberto pelo goal do usuário.

#### Files to edit
- `docs/benchmarks/clickbench-scale-gate-2026-07-24.md` — atualizar com o resultado

#### Concurrency tests
`(none — single-threaded)`. O protocolo do ClickBench é serial por definição (cada query 3×, cold + 2 hot);
executar em paralelo invalidaria a comparação com o leaderboard.

#### Acceptance Criteria
- 43/43 queries executam; A/B byte-idêntico vs heap preservado.
- `doctl compute droplet list --tag-name ephemeral-bench` retorna **0 linhas** ao fim da tarefa.

#### DoD
Gate verde e documentado; `doctl compute droplet list` sem instâncias efêmeras.

## Coverage Matrix

| Requisito / gap | Origem | Task(s) |
|---|---|---|
| INSERTs sucessivos com TOAST não podem falhar | #190 | T1.1, T1.3 |
| Remover dependência de snapshot no flush (robustez máxima) | diretriz do usuário + ADR-1 | T1.3 |
| Não deixar caminhos com semântica antiga | ADR-2 | T1.2 |
| Não trocar bug de correção por esgotamento de RAM | R1 | T1.4 |
| Estado ilegível pós-abort investigado, não encoberto | R3 / #190 `[NEEDS-REPRO]` | T2.1 |
| Custo de ingestão medido, não presumido | R2 | T3.1 |
| Gate do ClickBench destravado | `docs/benchmarks/clickbench-scale-gate-2026-07-24.md` | T3.2 |
| Comentário enganoso sobre snapshot-safety corrigido | `columnar.rs:1153-1155` | T1.3 |
| Linha isolada > `maintenance_work_mem` não pode causar OOM | EC-1 (edge-case review) | T1.4 |
| Datum EXPANDED materializado (senão use-after-free) | EC-2 (edge-case review) | T1.3 |
| Ordem de liberação de memória (senão corrupção silenciosa) | EC-3 (edge-case review) | T1.3 |
| Limiar de TOAST (inline / comprimido / externo) coberto | EC-4 (edge-case review) | T1.1 |
| Tipos de tamanho fixo trafegam intactos | EC-5 (edge-case review) | T1.1 |
| Caminho de pré-commit exercitado (lotes em txns distintas) | EC-6 (edge-case review) | T1.1 |
| TOAST aninhado em composite — limitação declarada | EC-7 (edge-case review) | Unresolved Q4 |
| `maintenance_work_mem` fixado no harness (determinismo) | EC-8 (edge-case review) | T1.4 |

Cobertura: **16/16 = 100%**.

## Global Definition of Done

- [ ] `make -C theodb_rs/isolation check-toast` sai 0, com não-vacuidade demonstrada (reverter o fix → falha).
- [ ] `check-isolation`, `check-crash`, `check-corrupt`, `check-compat` continuam verdes (sem regressão).
- [ ] Valores preservados: md5 do conteúdo colunar == md5 da origem heap.
- [ ] `columnar.rs` **não cresce** (teto de 500 LoC de `architecture.md` já violado em 1909 — a consolidação
      da ADR-2 deve reduzir).
- [ ] `cargo clippy` limpo; `cargo fmt --check` sem drift.
- [ ] `/code-quality` sem `FAIL_HARD`.
- [ ] CHANGELOG `[Unreleased]` atualizado (Regra 6), redigido para o consumidor.
- [ ] Benchmark de ingestão commitado em `docs/benchmarks/` (regra 5 do `CLAUDE.md`).
- [ ] Nenhum droplet efêmero remanescente.

## Failure scenarios

`(none — no external I/O touched)`. O plano opera dentro do processo do PostgreSQL: buffer em memória,
páginas locais e a toast table do próprio cluster. Não há cliente HTTP, driver de rede, fila ou object
store no caminho alterado. O modo de falha relevante — abort de transação no meio de um INSERT
multi-stripe — é tratado em T2.1, não como I/O externo.

## Final Phase: Integration Validation

Após T3.2, executar em sequência:

1. `make -C theodb_rs/isolation check-toast check-isolation check-crash check-corrupt check-compat`
2. `cargo clippy --all-targets` + `cargo fmt --check`
3. Gate do ClickBench 1M com `--sample systematic --agg` num droplet efêmero, destruído ao fim
4. `/code-quality` → verdict ∉ {FAIL_HARD, INVALID}

O plano só está completo quando os quatro passam. Um `check-toast` verde com `check-crash` vermelho
significa que trocamos um defeito por outro.
