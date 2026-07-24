# Edge Case Review — columnar-toast-materialize

Date: 2026-07-24
Plan: `.claude/knowledge-base/plans/columnar-toast-materialize-plan.md` (v1.0)
Tasks analyzed: 7 (T1.1–T1.4, T2.1, T3.1, T3.2)
Cases found: 8 (EDGE: 4, NEGATIVE: 4 | MUST FIX: 3, SHOULD TEST: 3, DOCUMENT: 2)

O plano já cobre bem dois riscos clássicos deste caminho: o pseudo-código de T1.3 filtra explicitamente
por *"atributo i não-nulo e de tamanho variável"*, o que evita (a) destoastar um NULL — segfault que o
próprio código adverte em `columnar.rs:1246` — e (b) destoastar um datum by-value tratando um `int` como
ponteiro. Ambos ficam **fora** deste relatório por já estarem resolvidos. O que segue é o que o plano
ainda não enxerga.

## MUST FIX

### EC-1: uma única linha maior que `maintenance_work_mem` estoura o buffer antes de qualquer flush
- **Affected task:** T1.4 (e por consequência T1.3)
- **Kind:** EDGE (extremo de um valor válido)
- **Family:** Resource
- **Scenario:** `columnar.rs:1145-1152` acumula a linha **e só então** compara `pending_bytes > mwm`. Hoje
  uma linha com `url` TOAST externo entra no buffer como ponteiro de 18 bytes. Depois de T1.3 ela entra
  **materializada**. Um valor `text` de, digamos, 512 MB (varlena aceita até 1 GB) é copiado inteiro para o
  buffer antes de qualquer verificação de teto — e o flush só ocorre na linha seguinte.
- **Impact:** OOM do backend numa única linha. É a materialização deste plano que cria a exposição: hoje o
  ponteiro de 18 bytes torna o cenário impossível. Trocar um erro de INSERT por um OOM seria uma piora.
- **Suggested fix:** em T1.4, mover a verificação de teto para **antes** de empurrar a linha: se
  `p.bytes + bytes.len() > mwm` e `!p.rows.is_empty()`, flush primeiro, depois empurra. Uma linha
  isoladamente maior que o teto ainda passa (não há como parti-la), mas nunca soma com as anteriores.

### EC-2: datum EXPANDED (array/composite em forma expandida) não é varlena on-disk
- **Affected task:** T1.3
- **Kind:** NEGATIVE (entrada em formato inesperado)
- **Family:** Format
- **Scenario:** o PostgreSQL passa arrays e alguns tipos compostos pelo executor em *expanded form*
  (`VARATT_IS_EXTERNAL_EXPANDED`) — um ponteiro para uma estrutura em memória, não um varlena serializado.
  `grep -rn "EXPANDED\|flatten" theodb_rs/src/` não retorna **nenhuma** ocorrência: o crate nunca tratou
  esse caso. Uma tabela colunar com coluna `text[]` alimentada por uma expressão que produza array
  expandido cai nesse caminho.
- **Impact:** `pg_detoast_datum_copy` **trata** expanded (achata), então hoje o flush funciona por acidente.
  Se T1.3 materializar com uma checagem ingênua do tipo "só se `VARATT_IS_EXTERNAL`", o expanded escapa da
  materialização, permanece como ponteiro para memória do executor — e o buffer passa a guardar um
  **ponteiro para memória já liberada** quando o contexto per-tuple é resetado. Isso é use-after-free, pior
  que o bug original.
- **Suggested fix:** em T1.3, materializar **todo** varlena não-nulo com `pg_detoast_datum_copy`
  (que já cobre external, compressed e expanded), em vez de testar `VARATT_IS_EXTERNAL` primeiro.

### EC-3: o contexto de memória da cópia materializada
- **Affected task:** T1.3
- **Kind:** NEGATIVE (recurso)
- **Family:** Resource
- **Scenario:** `pg_detoast_datum_copy` sempre aloca no `CurrentMemoryContext`. Em `accumulate_row` esse
  contexto é o per-tuple do executor, **resetado a cada linha**. O plano diz "liberar as cópias
  materializadas", mas o risco real é o inverso: se o `heap_form_tuple` for feito e a cópia liberada na
  ordem errada, forma-se a tupla a partir de memória já liberada.
- **Impact:** corrupção silenciosa dos bytes bufferizados — o pior modo de falha possível, porque os dados
  entram no stripe errados sem erro algum.
- **Suggested fix:** ordem explícita em T1.3: (1) detoastar → (2) `heap_form_tuple` (que **copia** os
  valores) → (3) copiar `t_data` para o `Vec` → (4) `pfree` das cópias + `heap_freetuple`. O
  `check-toast` deve assertar md5 do conteúdo, que é o que pega corrupção silenciosa — o plano já pede isso
  em T1.1, o que é bom; basta amarrar a ordem.

## SHOULD TEST

### EC-4: valor exatamente no limiar de TOAST (~2 KB)
- **Affected task:** T1.1
- **Kind:** EDGE
- **Suggested test:** `test_toast_boundary_inline_vs_external` — inserir valores de 1.900, 2.000 e 2.100
  bytes na mesma tabela e assertar `count(*)` e md5. O limiar (`TOAST_TUPLE_THRESHOLD`) depende do tamanho
  total da tupla, não só da coluna; um teste só com 3 KB não distingue "materializou tudo" de "materializou
  só o externo". Este teste exercita os três estados (inline, comprimido inline, externo).

### EC-5: coluna varlena de tamanho fixo declarado (`char(n)`) e tipos não-byval de tamanho fixo
- **Affected task:** T1.3
- **Kind:** EDGE
- **Suggested test:** `test_materialize_ignores_fixed_length_types` — tabela com `int`, `bigint`, `uuid`,
  `char(10)` e `text`, assertando que os quatro primeiros trafegam intactos. `extract_value_bytes`
  (`columnar.rs:535-544`) trata `attlen_fixed` **antes** do ramo varlena; a materialização em T1.3 precisa
  usar exatamente o mesmo critério (`attlen == -1`), senão diverge do que o flush espera.

### EC-6: segundo INSERT em **transação diferente** (não só segundo lote na mesma)
- **Affected task:** T1.1
- **Kind:** EDGE
- **Suggested test:** `test_toast_across_transactions` — INSERT + COMMIT, depois novo INSERT + COMMIT,
  assertando `count(*)` somado. O repro do #190 usa dois INSERTs na **mesma** transação; o caminho de
  pré-commit (`columnar.rs:206`) — que é onde o snapshot falta — só é exercitado no COMMIT. Um teste que
  não commita entre os lotes pode passar sem provar o caminho que originou o defeito.

## DOCUMENT

### EC-7: TOAST aninhado dentro de tipo composite
- **Kind:** EDGE
- **Accepted risk:** um valor `ROW(text, text)` pode ter campos internos toastados que
  `pg_detoast_datum_copy` no valor externo **não** resolve. É raro (exige coluna de tipo composite numa
  tabela colunar) e o ClickBench não usa. Documentar como limitação conhecida; se aparecer, é issue própria
  com `flatten_composite`. Fixá-lo agora seria escopo além do #190.

### EC-8: `maintenance_work_mem` pode ser alterado por sessão entre lotes
- **Kind:** EDGE
- **Accepted risk:** `SET maintenance_work_mem` no meio de uma transação muda o teto entre lotes, o que
  torna o número de stripes não-determinístico. Afeta apenas a **asserção de contagem de stripes** de T1.4,
  não a correção. Mitigação de teste: fixar o GUC no início do harness — o plano já faz isso implicitamente
  ao setá-lo para 1 MB.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 2 | 0 | 0 | 2 (EC-4, EC-6) | 0 |
| T1.2 | 0 | 0 | 0 | 0 | 0 |
| T1.3 | 1 | 2 | 2 (EC-2, EC-3) | 1 (EC-5) | 1 (EC-7) |
| T1.4 | 1 | 0 | 1 (EC-1) | 0 | 1 (EC-8) |
| T2.1 | 0 | 1 | 0 | 0 | 0 |
| T3.1 | 0 | 0 | 0 | 0 | 0 |
| T3.2 | 0 | 1 | 0 | 0 | 0 |

**Coverage check:** T1.3 e T1.4 são as tarefas que tocam a fronteira de entrada (datums vindos do
executor) e ambas têm EDGE **e** NEGATIVE considerados. T1.2 é refactor puro sem fronteira nova — sua
proteção é o A/B byte-idêntico já previsto. T2.1 é investigação (lente NEGATIVE por construção). T3.1/T3.2
são medição e validação; a lente NEGATIVE de T3.2 é o gate de destruição do droplet, já no DoD.

**Verdict:** PLAN NEEDS ADJUSTMENT

Três MUST FIX, todos concentrados no coração da mudança (T1.3/T1.4). Nenhum deles exige nova abstração —
são uma reordenação de verificação (EC-1), uma escolha de API mais abrangente (EC-2) e uma ordem explícita
de operações (EC-3). O plano continua sólido: as correções cabem como sub-passos das tarefas existentes,
sem alterar as ADRs.

Observação sobre o mais perigoso: **EC-2 e EC-3 podem transformar o bug atual (erro alto e visível) em
corrupção silenciosa** — bytes errados gravados no stripe sem erro algum. O `check-toast` de T1.1 já prevê
asserção de md5, que é exatamente o oráculo que pega isso; ele deixa de ser um detalhe do teste e passa a
ser a defesa principal contra os dois.
