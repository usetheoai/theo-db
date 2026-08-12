---
slug: b034-guc-alias
items: [B-034]
date: 2026-08-12
base: 0c42144
head: b886d3b
verdict: READY_TO_MERGE
---

# Review — os GUCs de ajuste do pgvector passam a ter efeito

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Testes verdes na branch | **457 passed, 0 failed** |
| 2 | Segredos commitados | **0** |
| 3 | Commit direto em `main` | não — `workspace` |
| 4 | Trailer de coautoria | **0** |
| 5 | `CHANGELOG.md` atualizado | sim |

`/code-quality`: `PASS_WITH_CAVEATS`, Rust auditado, **0 achados HARD**, 149 em `SOFT_FLOOR` (crates locais não verificáveis em crates.io — benigno e conhecido).

## Cross-validation — 6 de 6

| # | Afirmação | Teste | Resultado |
|---|---|---|---|
| G1 | `hnsw.ef_search` tem efeito | `pg_pgvector_ef_search_alias_has_effect` | ok |
| G2 | `ivfflat.probes` tem efeito | `pg_pgvector_probes_alias_has_effect` | ok |
| G3 | Precedência determinística | `pg_alias_precedence_specific_wins` | ok |
| G4 | Comportamento próprio preservado | `pg_native_guc_unchanged` | ok |
| G5 | Visíveis em `pg_settings` | `pg_alias_gucs_are_registered_in_catalog` | ok |
| G6 | Sem regressão | suíte completa 457/457 | ok |

Mais o end-to-end `pg_pgvector_alias_changes_recall_end_to_end`, que mede **recall** com `ef=1` vs `ef=400` setados pelo nome pgvector — o teste que o defeito original não sobreviveria, porque com o GUC inerte os dois recalls seriam idênticos.

## Verificação no produto

Imagem construída (`docker build` exit 0) e exercitada com banco rodando.

| Verificação | Resultado |
|---|---|
| `pg_settings` após carregar a extensão | **2** aliases registrados (antes: 0) |
| `SET hnsw.ef_search = 250` no bootstrap, depois consulta | `current_setting` → **250** — valor preservado |
| `SET hnsw.ef_search = 99999` após carga | `ERROR: outside the valid range (1 .. 1000)` |
| `SET hnsw.ef_search = 99999` antes da carga | `WARNING` na conversão; valor volta ao default **64** |

## Achados

### R-1 — MÉDIO · O CHANGELOG publicado estava impreciso, e a verificação no produto pegou

A entrada afirmava que valor fora de faixa *"passará a dar erro"*. **São duas formas**, e a distinção importa para quem atualiza:

- `SET` inválido **depois** da primeira consulta vetorial → `ERROR`, valor anterior preservado
- `SET` inválido **antes** (o caso mais comum, porque aplicações configuram no bootstrap) → `WARNING` no carregamento e **retorno ao default**

O segundo é mais suave do que eu anunciara, mas carrega um custo que a primeira redação escondia: **o valor é descartado e o usuário fica com 64 achando que pediu 99999**. Corrigido em `b886d3b`.

Este é o terceiro ciclo seguido em que a verificação no produto encontra algo que os testes unitários não pegam. Aqui não foi defeito de código — foi **defeito de descrição**, publicada.

### R-2 — INFORMATIVO · O `_PG_init` só roda ao carregar a biblioteca

A primeira leitura na imagem pareceu falhar: `pg_settings` vazio e `SET` inválido aceito. Não é defeito — os GUCs de uma extensão não pré-carregada só existem depois que o `.so` é carregado na sessão, e conectar não carrega. Tocar a extensão (`SELECT '[1]'::vector`) registra.

**A consequência prática é benigna e foi verificada:** a conversão do placeholder **preserva valores válidos**, então uma app que configura no bootstrap e consulta depois obtém o comportamento certo. Se descartasse, a correção não serviria para o caso de uso real.

### R-3 — BAIXO · Dois erros meus na edição, corrigidos

1. Inseri os registros novos **no meio** do bloco de `theodb_hnsw.over_fetch`, quebrando-o. Peguei conferindo o balanceamento das chamadas.
2. **Inventei duas funções SQL** (`theodb_scan_ef_search`, `theodb_scan_probes`) que não existem, para os testes chamarem. A correção ficou melhor que a invenção: `#[pg_test]` roda dentro do PostgreSQL, então os testes chamam `super::ef_search()` direto — sem criar superfície pública só para teste, o que seria o degrau 1 da parsimony ladder violado.

O item 2 é o mesmo padrão de **supor em vez de ler** que o ciclo B-033 documentou três vezes.

### R-4 — INFORMATIVO · O gate do plano deu 70, e dois terços foi erro meu de formatação

Peso bruto **98,8**, cobertura **6/6**. O cap veio de: critérios em prosa (o checker só conta bullets sob `#### Acceptance criteria`) e, ao padronizar, eu **apaguei o cabeçalho de quatro tarefas** — o parser passou a ver 2 seções onde há 6. Corrigido: 11/13 executáveis.

O terceiro cap (`auditor_unavailable_cargo-udeps`) é ambiente: a ferramenta **está** na imagem (verificado, 0.1.61), mas um prune externo esvaziou o volume de `target` e o udeps precisa recompilar o crate com nightly antes de responder.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou. O R-1 foi encontrado por verificação no produto, não por revisão de código — e a imprecisão já estava publicada.
- **A aresta de precedência não foi testada no produto.** O teste unitário cobre "próprio setado ao default + alias setado"; não repeti no banco real.
- **`hnsw.iterative_scan` e outros GUCs do pgvector não foram inventariados.** É a Q1/Q2 do plano, encaminhada como item futuro se houver.
- **O CI continua vermelho** (B-029).

## Veredito

**`READY_TO_MERGE`.**

Nenhum gate duro disparou; 6 de 6 afirmações verificadas por teste e o comportamento confirmado no produto em execução, incluindo o caminho de uso real (configurar no bootstrap, consultar depois).

**Ressalvas:** o review é do próprio implementador; a imprecisão do R-1 chegou a ser publicada antes de ser corrigida; e a correção muda comportamento observável — valor inválido deixa de passar em silêncio, o que está declarado no CHANGELOG nas duas formas medidas.
