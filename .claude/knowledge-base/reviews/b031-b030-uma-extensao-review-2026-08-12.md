---
slug: b031-b030-uma-extensao
items: [B-031, B-030]
date: 2026-08-12
base: bcf7819
head: 123c910
verdict: pending
---

# Review — uma extensão, um caminho de instalação

Domínio detectado: `database` (primário), `infrastructure` (secundário).

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Testes verdes na branch | **446 passed, 0 failed** |
| 2 | Segredos commitados | **0** — o hook gitsafety passou em todos os 7 commits |
| 3 | Commit direto em `main` | não — branch `workspace` |
| 4 | Trailer `Co-Authored-By` | **0 ocorrências** |
| 5 | `CHANGELOG.md` atualizado | sim |

Nenhum gate duro disparou.

## Cross-validation — cada afirmação do Goal contra a realidade

| # | Afirmação | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | Cadeia `theodb_rs` removida | `ls theodb_rs/sql/theodb_rs--*--*.sql` | 0 arquivos |
| G2 | Cadeia do umbrella removida | `ls sql/theodb--*--*.sql` | 0 arquivos |
| G3 | Shim reduzido a uma versão | `ls sql/vector--*.sql` | 1 arquivo |
| G4 | Build coerente sem cadeia | `docker build -t theodb:b031 .` | ver § Achados, R-1 |
| G5 | Superfície verificada por teste | `pg_surface_contains_public_api` | ok |
| G6 | ACL de egress verificada | `pg_egress_surface_is_revoked_from_public` | ok |
| G7 | Wrappers validados no `CREATE` | `pg_ai_wrappers_are_sql_language` | ok |
| G8 | `ai.generate/summarize/agg` absorvidos | presentes na lista de membros da extensão | ok |
| G9 | Superfície NL absorvida | idem | ok |
| G10 | Registry `theodb_ml` absorvido | idem | ok |
| G11 | `import_vectors_chunked` absorvido | `pg_import_vectors_chunked_is_a_procedure` (`prokind='p'`) | ok |
| G12 | Superfície HTAP absorvida | presente na lista de membros | ok |
| G13 | Umbrella deixou de existir | `theodb.control`, `Makefile`, `sql/[0-9]*-theodb-*.sql` | 0, 0, 0 |
| G14 | Shim permanece separado | `grep "requires = 'theodb_rs'" vector.control` | 1 |

Estrutura conferida em separado: **6 arquivos** sob `theodb_rs/sql/surface/` e **6 invocações** de `extension_sql_file!` em `src/surface.rs` — casam. As outras 2 ocorrências do termo no arquivo são comentário de documentação e o `use`, não declarações.

## Achados

### R-1 — MÉDIO · O G4 foi afirmado antes de ser medido

Durante a implementação eu construí apenas `--target theodb-toolchain` e segui adiante. O critério de aceitação do T1.4 exige `docker build -t theodb:b031 .` (imagem **inteira**, incluindo o estágio de runtime) terminando em 0, e isso não havia sido executado.

O risco concreto que a lacuna cobria: a Fase 3 removeu `theodb.control`, o `Makefile` e a concatenação de corpos do estágio de runtime. Se algum resíduo tivesse ficado — um `COPY` de arquivo inexistente, um `install` com glob vazio — o estágio de runtime quebraria e nenhuma das verificações anteriores perceberia, porque todas rodam sobre o crate, não sobre a imagem.

Corrigido durante este review: o build completo foi executado. Resultado em § Veredito.

### R-2 — BAIXO · Três defeitos nas verificações novas, corrigidos antes do merge

Registrados no CHANGELOG e nos commits `842347c` e `829ddfe`. Resumidos aqui porque um reviewer precisa saber que as verificações que sustentam G5–G7 nasceram tortas:

1. `.ok().flatten()` colapsava "consulta falhou" e "não encontrei" — reportou `ai.generate = AUSENTE` para função instalada.
2. A superfície esperava `procedure …`, mas `pg_describe_object` imprime `function` mesmo para `prokind='p'` — testava a redação do catálogo, não a propriedade.
3. Faltava cast na coluna projetada (`lanname` é `name`, não `text`) — o defeito que o item 1 mantinha invisível.

O item 1 só apareceu porque `surface_contains_public_api` contradisse `ai_wrappers_are_sql_language`. **É o argumento a favor de separar os eixos de verificação** em vez de um teste único que "verifica a superfície": os dois se cruzaram e o erro caiu.

### R-3 — INFORMATIVO · O aparato de qualidade tinha três defeitos próprios

Fora do escopo do produto, mas descoberto por este ciclo e corrigido:

- `code-quality-languages.txt` com todas as linguagens comentadas → o gate devolvia `PASS` com `languages_audited: []`.
- O cabeçalho do mesmo arquivo documenta um formato que o parser rejeita (`malformed line`). **Não corrigido** — é mudança na skill, não no produto.
- `cargo-udeps` ausente → instalado na imagem, em vez de dispensado por ADR.

### R-4 — INFORMATIVO · Achado registrado como B-032, não tratado aqui

**2.872 ocorrências de `unsafe_op_in_unsafe_fn`** na saída do build (1.874 chamadas a função `unsafe`, 932 desreferências de ponteiro cru, 54 `static` mutáveis, 4 campos de `union`), concentradas em `src/am/columnar_agg.rs` (1.236) e `src/am/page/mod.rs` (354).

Deliberadamente **fora** deste ciclo: não é trabalho de um ciclo, e embuti-lo diluiria os dois. Registrado como B-032 com DoD que exige que a correção seja pura anotação.

## O que este review NÃO cobriu

Dito porque a ausência de menção lê-se como cobertura:

- **Não houve revisão por agentes especialistas independentes.** O `cycle-review` prevê 5–7 agentes em paralelo; esta revisão foi conduzida pelo mesmo agente que implementou. Olhos frescos teriam valor exatamente onde eu tenho ponto cego — foi um reviewer independente que teria pego o G4 antes de mim.
- **Não foi exercitada a superfície em uso.** Os testes provam que os objetos existem, com a ACL certa e na linguagem certa. Nenhum deles chama `ai.generate` contra um endpoint real, porque isso exige egress HTTP. A validação de ponta a ponta é a fase `cycle-acceptance`, não esta.
- **O CI continua vermelho** por causa das remoções de `benchmarks/` e `scripts/` (B-029). Esta mudança não conserta isso e não deve ser lida como se consertasse.

## Veredito

Preenchido após o resultado do G4 — ver § Achados, R-1.
