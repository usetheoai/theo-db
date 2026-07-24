# Discover Edge Case Review — m147-scan-version-dispatch

Date: 2026-07-24
Discovery plan analyzed: `.claude/knowledge-base/discoveries/plans/m147-scan-version-dispatch-plan.md`
Research questions analyzed: 6
Edge cases found: 4 (MUST FIX: 2, SHOULD TEST: 0, DOCUMENT: 2)

## MUST FIX

### EC-1: o dispatch de comportamento do pgvectorscale NÃO está em `storage.rs`

- **Affected question:** Q1
- **Family:** Reference path / Method
- **Scenario:** O Q1 aponta `storage.rs` como alvo principal e a Fase A busca `trait Storage`/`impl Storage` lá. Mas `storage.rs:145-169` só tem a **conversão** `u8 → StorageType` (`match value { 0 => Plain, 2 => SbqCompression, _ => panic! }`). O **dispatch de comportamento** — onde o tipo escolhe a implementação — vive em `scan.rs:68` (`match storage { Plain => …, SbqCompression => … }`) e `meta_page.rs:278`. A Fase A restrita a `storage.rs` acharia só o trait+conversão e perderia o padrão OCP real que o M147 quer emular (o match centralizado que despacha).
- **Impact:** O blueprint responderia Q1 com "pgvectorscale usa um trait Storage" sem mostrar ONDE o dispatch acontece — exatamente a informação que o bullet 1 do M147 (if-ladder → enum) precisa.
- **Suggested fix:** Adicionar `pgvectorscale/pgvectorscale/src/access_method/scan.rs` e `meta_page.rs` ao alvo do Q1 e à Fase A (`ast-grep -p 'match $S { StorageType::$V => $$$ }'`), citando `scan.rs:68` como o ponto de dispatch.

### EC-2: os testes de "comportamento preservado" do scan estão em `mod.rs`, não em `sbq/tests.rs`/`plain/tests.rs`

- **Affected question:** Q4
- **Family:** Reference path
- **Scenario:** O Q4 aponta `sbq/tests.rs`, `plain/tests.rs`, `upgrade_test.rs` como fonte do padrão de "prova que o scan retorna o resultado correto". Mas esses arquivos testam **build/vacuum** (`test_plain_storage_index_creation_*`, `test_plain_storage_delete_vacuum`) — grep por `ORDER BY`/`<=>`/`recall`/`assert_eq` neles retorna **zero**. O padrão de teste com asserção de resultado de scan está em `mod.rs`/`options.rs`.
- **Impact:** O Q4 é o corner `tests` — se ele mapear o padrão errado, o blueprint não entrega o modelo de teste que o M147 precisa para provar "comportamento byte-idêntico".
- **Suggested fix:** Trocar o alvo do Q4 para `pgvectorscale/pgvectorscale/src/access_method/mod.rs` (onde vivem os `#[pg_test]` que asseveram resultado de scan) + manter `upgrade_test.rs` (para o padrão de compat de versão antiga).

## DOCUMENT

### EC-3: `StorageType::from` do pgvectorscale usa `panic!` — padrão a NÃO transferir

- **Affected question:** Q1
- **Accepted risk:** `storage.rs:14/22` faz `panic!("Invalid storage type")` na conversão. Isso é exatamente o anti-pattern que o M146 acabou de eliminar do nosso código (panic atravessando a fronteira C → XX000). O nosso idioma é `err_input`/`err_corrupt` tipado. O blueprint DEVE registrar este como "padrão a NÃO copiar" — já está no Acceptance Criteria do plano ("declara qual padrão não se aplica e por quê"). Nenhuma mudança de plano necessária além dessa nota; documentado aqui para o execute não copiar o `panic!` cegamente.

### EC-4: pgvectorscale é pgrx 0.16.1; nós somos 0.19

- **Affected question:** Q5
- **Accepted risk:** As assinaturas de `amgettuple`/`amrescan` e os idiomas de `PgBox`/erro podem diferir entre pgrx 0.16 e 0.19. Isso NÃO é um defeito do plano — é o próprio objeto do Q5 (que pergunta o delta). O risco residual é o execute interpretar um idioma de 0.16 como diretamente aplicável a 0.19; o Q5 já força a comparação. Aceito; o blueprint nota o delta de versão como caveat de transferência.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 2 | 1 (EC-1) | 0 | 1 (EC-3) |
| Q2 | 0 | 0 | 0 | 0 |
| Q3 | 0 | 0 | 0 | 0 |
| Q4 | 1 | 1 (EC-2) | 0 | 0 |
| Q5 | 1 | 0 | 0 | 1 (EC-4) |
| Q6 | 0 | 0 | 0 | 0 |

Os corpos legados do lance (`previous/reader.rs`, `previous/format/`) e o `ivfscan.c` do pgvector foram confirmados existentes e do tamanho esperado — Q2, Q3 e Q6 sem edge-cases.

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT (2 MUST FIX — refinamento de path/alvo em Q1 e Q4, sem expandir escopo)
