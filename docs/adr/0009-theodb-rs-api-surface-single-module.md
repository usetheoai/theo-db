# ADR 0009 — `theodb_rs` api-surface as a single `api.rs` facade module (M25)

**Status:** Accepted · **Data:** 2026-07-01 · **Owner:** paulohenriquevn
**Amenda:** o ADR intra-plano **ADR-2** de `.claude/knowledge-base/plans/m25-craft-hardening-plan.md`
("per-feature modules own their externs + DDL") — esta é a decisão de implementação que o superseda.
**Relacionado:** `.claude/rules/architecture.md` (§4 boundary enforcement; §6 anti-god-module), blueprint
ADR-1 (3-boundary layering), `docs/benchmarks/m25-craft-hardening.md` (evidência antes/depois).

## Contexto

M25 dividiu o god-file `lib.rs` (721 LoC — acima do budget heurístico de 500 de `architecture.md`). O plano
locou, no ADR-2 intra-plano, a distribuição **por-feature**: cada módulo de domínio (`embed`, `nl`, `hybrid`,
`sbq`, …) passaria a possuir seus próprios `#[pg_extern]` + blocos `extension_sql!`, citando o layout do
`pgvectorscale` (externs no módulo da feature).

Na implementação, ao mover o código, ficou evidente uma restrição estrutural que o plano não previu: **todos os
externs compartilham um único `#[pg_schema] mod theodb_rs`** (o schema SQL vem do *ident* do módulo). Distribuí-los
por-feature exigiria ou (a) N blocos `#[pg_schema] mod theodb_rs` espalhados por N arquivos — múltiplas declarações
do mesmo ident de schema em módulos-pai distintos, um padrão pgrx menos comum e não validado neste repo — ou (b)
reescrever a fronteira de schema. Ambos introduzem **risco novo** sobre um código que já foi provado
byte-idêntico.

O resultado adotado foi um único `api.rs` (640 LoC) contendo o `mod theodb_rs {externs}` + os 8 blocos
`extension_sql!` movidos **verbatim**. Isso deixou `lib.rs` em 92 LoC (raiz de composição fina) mas mantém
`api.rs` acima do budget heurístico de 500 LoC — o achado HIGH do `/review`.

## Tensão reconhecida (honestidade — Regra 3)

- **DoD do plano:** "todo arquivo alterado dentro do budget de 500 LoC". `api.rs` = 640 LoC → literalmente não
  cumprido. Não escondemos isso (doc §2 registra 640).
- **Divergência do ADR-2:** o plano locou por-feature; entregamos facade único. Uma divergência de ADR locado
  exige amenda (este documento) — o defeito real que o `/review` (cross-validation) apontou.

## Decisão

**A api-surface de `theodb_rs` é um único módulo `api.rs` (facade), e o budget de 500 LoC é formalmente
dispensado para este arquivo**, pelos seguintes motivos:

1. **É a fronteira de camada, não um god-module.** `architecture.md §6` proíbe god-modules (`utils`, `misc`,
   acúmulo de código *não-relacionado*). `api.rs` tem **uma** responsabilidade coesa: a superfície SQL (os
   `#[pg_extern]` + o DDL `extension_sql!` que os envelopa). É a materialização da 3ª fronteira do blueprint
   ADR-1 (pg-glue · domain · **api-surface**). Coesão alta, não lixeira.
2. **~87% é SQL declarativo.** O arquivo é majoritariamente strings DDL (`extension_sql!`) — complexidade
   ciclomática ~zero. O global max CCN pós-M25 (`ann_query::knn`, 15) está fora de `api.rs`. O risco que o budget
   de LoC busca mitigar (funções longas e complexas) não se aplica a DDL declarativa.
3. **O budget de 500 é heurístico, não consenso.** `architecture.md §4` não o mecaniza; a própria auditoria de
   arquitetura tagueia `LOC ≤ 500` como *heurística/folclore — sem fonte forte*. A métrica **essencial** do Goal
   (`lib.rs < 200 LoC`) **foi cumprida** (92).
4. **Esforço ≠ Complexidade (CLAUDE.md).** Fatiar um facade declarativo coeso em 8 arquivos minúsculos só para
   satisfazer um número folclórico é **complexidade acidental auto-imposta** — proibida pelo princípio-guia do
   projeto ("complexidade desnecessária é proibida; nenhum montante de esforço a justifica"). Trocaria um
   arquivo coeso provado-idêntico por proliferação de arquivos + risco de re-validação, sem ganho essencial.
5. **Consistência com SOTA (parcial, honesta).** `pgvectorscale`/`vectorchord` mantêm `lib.rs` fino (47/83 LoC)
   e empurram a superfície para módulos dedicados — que é o que M25 fez. Divergência honesta: eles espalham por
   feature; nós concentramos em um facade, forçado pelo `#[pg_schema] mod theodb_rs` único e por a lógica de
   domínio já viver em `embed`/`nl`/`hybrid`/`sbq`/`vec` (os shims em `api.rs` são delegados finos).

## Alternativas rejeitadas

- **A1 — Split por-feature (ADR-2 original):** N arquivos, cada um `#[pg_schema] mod theodb_rs`. *Rejeitada:*
  introduz risco novo (múltiplas declarações do mesmo schema-ident; re-topologia do grafo `requires` do pgrx)
  sobre código já provado byte-idêntico; ganho é cosmético (cumprir folclore de LoC) contra o princípio
  Esforço≠Complexidade. Reabri-la exige benchmark/prova de que o split multi-módulo do schema é seguro — um
  slice próprio, não escopo de um hardening behavior-preserving.
- **A2 — Separar externs (Rust) de DDL (SQL) em dois arquivos** (`api.rs` + `api_sql.rs`): reduz LoC por arquivo
  para < 500 sem multi-schema. *Rejeitada por ora:* o DDL envelopa diretamente cada extern (acoplamento de
  co-localização); separá-los espalha uma unidade de leitura ("o extern X e seu wrapper SQL") por dois arquivos,
  piorando a navegabilidade para cumprir um número. Reconsiderável se `api.rs` crescer com lógica não-declarativa.
- **A3 — Manter em `lib.rs` (status quo pré-M25):** *Rejeitada:* `lib.rs` a 721 LoC misturava raiz-de-crate com
  api-surface (duas responsabilidades) — o achado original da auditoria. O split resolve isso.

## Consequências

- **Positivas:** `lib.rs` vira raiz de composição fina (92 LoC, cumpre a métrica essencial); a api-surface fica
  numa fronteira nomeada e coesa; zero mudança de comportamento (schema/DDL byte-idênticos — provado por rebuild
  + 72 testes de integração verdes + revisor de paridade).
- **Negativas (aceitas):** `api.rs` = 640 LoC permanece acima do budget heurístico. Mitigação: se `api.rs` vier a
  acumular lógica **não-declarativa** (não apenas mais shims/DDL), reabrir A1/A2 num slice dedicado com prova de
  segurança do split multi-schema.

## Quando esta decisão muda

Se um extern futuro exigir lógica imperativa não-trivial dentro de `api.rs` (elevando o CCN do arquivo), ou se o
`#[pg_schema] mod theodb_rs` multi-módulo for provado seguro por um spike, reabrir A1/A2 via novo ADR + CHANGELOG.
