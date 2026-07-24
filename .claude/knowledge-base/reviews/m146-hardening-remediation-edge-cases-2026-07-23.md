# Edge Case Review — m146-hardening-remediation

Date: 2026-07-23
Plan analyzed: `.claude/knowledge-base/plans/m146-hardening-remediation-plan.md` (v1.0, `/plan-confidence` SHIPPABLE 98)
Tasks analyzed: 8 (T1.1–T1.3, T2.1–T2.4, T3.1)
Cases found: 6 (EDGE: 3, NEGATIVE: 3 | MUST FIX: 2, SHOULD TEST: 2, DOCUMENT: 2)

Todas as APIs/tipos citados foram verificados em disco antes desta análise — nada aqui é especulação.

## MUST FIX

### EC-1: `into_inner()` NÃO escreve o footer do Parquet — trocar `close()` por ele produz arquivo ilegível
- **Affected task:** T1.3
- **Kind:** NEGATIVE (a mudança "óbvia" corrompe a saída)
- **Family:** Format / API
- **Scenario:** O plano precisa recuperar o `File` de dentro do `ArrowWriter` para fazer `sync_all()`. O caminho ingênuo é trocar o `w.close()` atual (`parquet.rs:262`) por `w.into_inner()`. Mas em `parquet-54.3.1/src/arrow/arrow_writer/mod.rs:325`, `into_inner()` faz **apenas** `self.flush()?` + `self.writer.into_inner()` — **não** chama `finish()`, que é quem escreve o footer/metadata do Parquet (`:335-338`, e `close()` em `:341` é literalmente `self.finish()`).
- **Impact:** o export produziria um arquivo **sem footer**, isto é, um Parquet corrompido que nenhum leitor abre — trocando um defeito de durabilidade por um defeito de corretude, muito pior.
- **Suggested fix:** sequência obrigatória em T1.3: `w.finish()?` (escreve o footer, **não** consome `self` — `:333-338`) → `w.into_inner()?` (recupera o `File`) → `file.sync_all()?` → `rename` → fsync do diretório-pai.

### EC-2: `Path::parent()` de um nome de arquivo simples devolve `""` — `File::open("")` falha
- **Affected task:** T1.3
- **Kind:** EDGE (extremo de entrada válida)
- **Family:** Input / Path
- **Scenario:** Para fsyncar o diretório-pai é preciso `path.parent()`. Para um path relativo simples (ex.: `"out.parquet"`), `Path::parent()` devolve `Some("")` — e `File::open("")` erra com ENOENT. O export falharia num caso de uso legítimo.
- **Impact:** export quebra para path relativo sem diretório; ou, se o erro for engolido, o fsync do diretório silenciosamente não acontece (perde-se justamente a parte load-bearing do protocolo).
- **Suggested fix:** espelhar o upstream — `fd.c:3885-3886` faz exatamente isso: parent vazio → usar `"."`. Adicionar ao T1.3: `let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));`

## SHOULD TEST

### EC-3: índice vazio (`n == 0`) não pode virar falso-positivo na nova validação
- **Affected task:** T1.1
- **Kind:** EDGE
- **Suggested test:** `from_bytes` de um índice vazio (`n=0`, `neighbors` vazio) deve continuar `Ok`. `any()` sobre iterador vazio é `false`, então o comportamento correto já cai por construção — mas o teste trava a regressão (e o bloco existente já trata `None if n > 0`).

### EC-4: o harness de corrupção não deve depender de um offset "mágico" frágil
- **Affected task:** T3.1
- **Kind:** NEGATIVE
- **Suggested test:** em vez de calcular o offset exato do array de vizinhos (frágil a qualquer mudança de layout), corromper uma **faixa** de offsets dentro da região do blob e assertar a **propriedade** que interessa: qualquer corrupção resulta em erro SQL limpo com backend vivo — nunca `server closed the connection`. Isso é robusto a evolução de formato e continua provando o que o T1.1 promete.

## DOCUMENT

### EC-5: `::regclass` aceita string numérica como OID cru
- **Kind:** NEGATIVE (entrada inesperada aceita)
- **Accepted risk:** `regclassin` chama `parseDashOrOid` antes de resolver o nome, então `graph_build('12345','src','dst')` resolve o **OID 12345** em vez de erro. É semântica do PostgreSQL (o mesmo vale para qualquer `::regclass`), o caller continua precisando de privilégio `SELECT` na relação resolvida, e a função não é `SECURITY DEFINER`. Documentar no comentário; não adicionar gate extra (seria divergir do host sem ganho real).

### EC-6: `ERRCODE_INDEX_CORRUPTED` (XX002) é mais preciso que `ERRCODE_DATA_CORRUPTED` (XX001) para página de índice
- **Kind:** EDGE (escolha de granularidade)
- **Accepted risk:** ambos existem no pgrx 0.19 (`pgrx-pg-sys-0.19.0/src/submodules/errcodes.rs:384-385`). Para erro de desserialização de **índice**, o precedente do amcheck é `ERRCODE_INDEX_CORRUPTED`. Registrar em T2.1 que o código escolhido é XX002 para índices, deixando XX001 para corrupção de dados não-indexados (ex.: stripe columnar), caso surja.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|---|---|
| T1.1 | 1 | 0 | 0 | 1 (EC-3) | 0 |
| T1.2 | 0 | 1 | 0 | 0 | 1 (EC-5) |
| T1.3 | 1 | 1 | 2 (EC-1, EC-2) | 0 | 0 |
| T2.1 | 1 | 0 | 0 | 0 | 1 (EC-6) |
| T3.1 | 0 | 1 | 0 | 1 (EC-4) | 0 |

**Coverage check:** T1.3 (única task com I/O externo) tem EDGE e NEGATIVE cobertos. T1.1/T1.2 têm o lado negativo coberto pelos próprios testes RED do plano.

**Verdict:** PLAN NEEDS ADJUSTMENT — 2 MUST FIX no T1.3 (EC-1 footer do Parquet; EC-2 diretório-pai vazio) devem ser absorvidos antes do `/implement`. Ambos são de correção barata e ambos evitariam um defeito pior que o que o milestone corrige.
