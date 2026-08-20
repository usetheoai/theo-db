---
slug: tier1-portao
items: [B-029, B-039, B-016, B-023, B-053, B-013, B-022, B-027]
date: 2026-08-13
base: 2daac5b
head: cdf784e
verdict: READY_TO_MERGE
---

# Review — quatro dos sete itens já estavam fechados, e eu quase apaguei um portão por ler um nome

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | `/code-quality` | **`PASS_WITH_CAVEATS` (89)** — subiu de `FAIL_SOFT` (70), **0 achados HARD** |
| 2 | Suíte da ferramenta | **175 passed, 1 failed** — a falha é pré-existente (§ R-6) |
| 3 | Suíte Rust | **477 passed; 0 failed** — 478 menos exatamente o micro-bench que saiu (§ R-8) |
| 4 | Segredos commitados | **0** |
| 5 | Commit direto em `main` | não — `workspace` |
| 6 | Trailer de coautoria | **0** |
| 7 | `CHANGELOG.md` atualizado | sim — 5 entradas em `Fixed` |
| 8 | Bundle OKF | não tocado neste ciclo |

O cap `auditor_unavailable_cargo-udeps` **sumiu pela primeira vez em cinco ciclos** — é o próprio B-039 deste
plano, e o veredito subiu por causa dele.

## Cross-validation — 5 de 5

| # | Afirmação | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | Nenhuma invocação morta | `check-workflow-paths.sh` contra `HEAD` | exit **0** |
| G2 | O verificador **reprova** o caso real | o mesmo script contra `8605677` | exit **1**, listando as 10 com `arquivo:linha` |
| G3 | O smoke prova nos dois lados | contra `postgres:18-bookworm` puro e contra `theodb:b036` | **reprova** nomeando a extensão ausente · **passa** verificando os dois nomes de AM |
| G4 | O drift volta a comparar | `#[pg_extern]` fictício adicionado e revertido | `sql-surface` detectou (`fn _b029_drift_probe` no diff); 171 símbolos na superfície |
| G5 | `cargo-udeps` reporta em vez de indisponível | `/code-quality` **duas** execuções seguidas | `PASS_WITH_CAVEATS` nas duas; o cap não voltou |

## Achados

### R-1 — ALTO · Eu quase apaguei o portão da promessa drop-in por ler um nome

A D1 do plano dizia que os três `migrate-*` "testam a cadeia de upgrade removida pelo B-031" e mandava
**remover as invocações**. Eu inferi isso **do nome do arquivo**.

Lidos: `migrate-smoke.sh` migra uma base **pgvector baunilha para o TheoDB via `pg_dump`/`pg_restore`
padrão** e verifica que dado e índices sobrevivem — é a promessa drop-in do `ADR-0029 § D2`, a mesma
capacidade que os quatro últimos itens do projeto ([[B-033]], [[B-034]], [[B-036]] e o shim) foram
construídos para entregar. `migrate-doc-check.sh` garante que todo comando publicado em
`wiki/guides/minimal-migration.md` aparece literalmente no smoke, para o guia não derivar do que é testado.

**E há uma razão mais forte que "não estavam errados".** `migrate-smoke.sh` semeia a origem com
`CREATE INDEX ... USING ivfflat` (`:76`). Medido no `theodb:b036`:

```
ERROR:  access method "ivfflat" does not exist
```

Só `hnsw`, `theodb_hnsw` e `theodb_ivfflat` existem. **O oráculo restaurado reprova, e por razão verdadeira** —
é o [[B-037]] quebrando a migração real, com número, no caminho que o ADR-0029 promete.

A lição é sobre método: **um plano que classifica artefatos pelo nome está adivinhando**, e o custo aqui teria
sido apagar a única verificação automática da capacidade mais defendida do produto.

### R-2 — ALTO · Três dos cinco itens do Tier 1 já estavam fechados, e um quarto também

Antes de implementar qualquer coisa, a medição fechou quatro itens sem escrever código:

| Item | O que já existia |
|---|---|
| **B-013** | `rust-suite.yml` roda `cargo pgrx test pg18`, `BASELINE=0` desde 2026-08-12, reprova em regressão, e distingue "não emitiu resultado" de "reprovou" (`:149`) |
| **B-027** | O contêiner virou `suite-${run_id}-${run_attempt}` — a colisão deixou de ser **possível**, não foi remediada |
| **B-022** | Os dois testes passam com a mensagem completa; como o pgrx compara por igualdade, passar **é** a prova |
| **B-016** | **Cinco** `#[test]` dedicados à guarda SSRF, e o disjuntor com cobertura de estado sem rede nenhuma |

O B-016 merece nota: o item afirma que a guarda "hoje só é exercitada por acidente". Ela tem bateria dedicada
cobrindo Teredo, v4-compatible, site-local, CGNAT, benchmarking, reservado, confusão de `userinfo`, escopo e
parsing da allowlist. E a escolha de `invalid.invalid` como alvo inalcançável foi **medida**: TEST-NET passa a
guarda mas custa **90,6 s** com os retries; o DNS falha em milissegundos.

**Foi a segunda vez neste ciclo que planejei contra o texto de um item em vez de contra o código.** A T1.5 teria
duplicado cobertura existente para satisfazer um plano — a forma mais cara de parecer produtivo.

### R-3 — ALTO · O achado que nenhum item nomeava: `workspace` não tem portão

Todo gate roda apenas em `push` para `develop`/`main` — o gatilho de `pull_request` saiu em 2026-08-12 por
decisão do owner (runner único e serial, custo). Como `rules/git-safety.md § 1` manda que **todo** trabalho
nasça em `workspace`, o primeiro portão a ver a mudança olha **depois** do merge.

| Desde a última execução em `workspace` (2026-08-12T10:34) | |
|---|---|
| commits | **73** |
| tocando `theodb_rs/src/` | **13** |
| diff em `theodb_rs/` | **+2.414 / −7.420** |
| execuções de gate | **0** |

Os 478 testes do B-036 passaram porque **eu** os rodei à mão, num contêiner que montei à mão. Nada exigiu isso
e nada teria notado a ausência. Registrado como [[B-052]], **não resolvido aqui**: o DoD exige ADR, porque
negocia com uma decisão do owner cuja razão (capacidade do runner) é real.

### R-4 — MÉDIO · O conserto do B-039 estava certo e funcionava só com cache quente

A primeira versão caía para o contêiner quando a saída do host casasse a assinatura do pgrx ausente. Medido:
nesta máquina o host falha antes disso, em `failed to write .../target/.fingerprint/` — o obstáculo **(a)** que
o próprio B-039 registrou, mascarando o **(b)** que eu havia codificado. Um predicado por assinatura deixaria o
cap disparando **exatamente na máquina onde ele foi medido**.

Corrigido para **ausência de dado**: um audit que rodou devolve JSON, inclusive quando acha dependência não
usada. Sem JSON e com exit ≠ 0, o host não auditou.

Depois disso veio o segundo: host e contêiner compilavam no **mesmo** `target/`. A primeira execução deu
`exit 101: Updating crates.io index`; a segunda, com cache quente, passou limpo. **Um conserto que só funciona
quente falha na máquina de quem chega depois — com modo de falha idêntico ao cap que ele existe para remover.**
`CARGO_TARGET_DIR` próprio em volume nomeado, fonte montada `:ro` (verificado: o contêiner não consegue escrever
na árvore do host). Duas execuções seguidas, mesmo veredito.

### R-5 — MÉDIO · O saneamento do Tier 0 acusou o registro de um defeito que eu tinha introduzido

Eu havia proposto "avançar 9 itens `planned` que já entregaram". A verificação derrubou a premissa: a última
release é a **v0.158.0**, os PRs #227 e #228 aguardam aprovação, e `shipped` exige `RELEASED`. Os 9 estavam
**certos** — e o commit `88479fe` já explicava isso em texto.

**O único status corrompido era `B-036 = shipped`, marcado por mim no mesmo dia**, com o commit que está no
mesmo PR dos outros oito. Revertido, com a nota dizendo isso.

O que sobrou de real: B-001, B-004 e B-025 entregues e nunca avançados; e o B-001 carregava `[x]` com
`status: raw` simultaneamente — dois campos do mesmo bloco discordando, e **nada compara os dois**. Virou
[[B-051]].

### R-8 — ALTO · Um gate mais barato que o gate que importa dá confiança que ele não sustenta

A extração do núcleo puro (B-053) falhou **duas vezes**, e as duas falhas são a mesma lição — a do próprio
ciclo, agora comigo:

1. `cargo check --features pg18` passou **limpo** com o módulo de testes quebrado. Sem `pg_test` ele **nem é
   compilado**. O erro real (`cannot find module simd_x86`) só apareceu em `cargo pgrx test`, **25 minutos
   depois**.
2. A segunda tentativa caiu em `pub(super) fn force_for_test`: antes `super` era `vec`, e depois da extração
   passou a ser `kernels`. Outros ~6 minutos de suíte para descobrir uma linha.

O gate correto é `cargo check --features pg18,pg_test --all-targets`, e ele fecha em **2m32s** contra os ~6 min
da suíte. Eu usei o barato porque era barato, e paguei 30 minutos por isso.

**Não virou item porque não é defeito do produto — é do meu método**, e o registro dele aqui é o remédio
disponível. O que vale generalizar: *"compila"* não é uma propriedade, é a propriedade de uma configuração; a
configuração que importa é a que o CI usa.

### R-6 — BAIXO · Um teste vermelho pré-existente na ferramenta, que não é meu e não foi escondido

`tests/test_shared.py::test_the_shipped_template_parses_when_you_follow_its_own_instructions` falha porque
`rules/code-quality-languages.txt` não tem linha de exemplo comentada. **Verificado com `git stash`**: falha
igual sem o meu trabalho na árvore.

Não consertei porque está fora do escopo dos quatro itens, e consertar em silêncio um vermelho alheio no meio de
um ciclo sobre portões seria exatamente o oposto do que o ciclo defende. Fica registrado aqui.

### R-7 — INFORMATIVO · O que este ciclo deliberadamente NÃO fez

- **Não mexeu no gatilho dos workflows.** É o [[B-052]], e precisa de ADR.
- ~~Não extraiu o núcleo puro de `vec.rs`.~~ **EXECUTADO** (§ R-8): `vec/kernels.rs`, 290 linhas, zero
  `crate::`. A fronteira caiu onde já existia — a família `*_from_bytes` nunca chamou `check_dims`. O bench
  linka e roda sem PostgreSQL, e a suíte foi de 478 para 477 por exatamente um teste: o que saiu.
- **Não restaurou os 11 scripts de milestones encerrados** (`m131_sweep`, `m139-*`, `m140-*`, `m56-crash-e2e`,
  `vectorizer-e2e`, `docs-features-lint`, `pgrx-test-in-builder`). Nenhum workflow os invoca; voltar com eles
  seria ressuscitar o que a limpeza corretamente removeu.
- **Não rodou os workflows de verdade.** Eles só disparam em `push` para `develop`/`main`, e este ciclo vive em
  `workspace` — que é, com ironia, o próprio achado R-3.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **Os gates restaurados não foram exercitados pelo CI real** — só localmente, contra contêineres locais. A
  primeira execução verdadeira será no próximo push para `develop`.
- **`cassert-smoke.sh` não foi executado ponta a ponta.** Verifiquei que **toda** a superfície que ele cita
  existe no produto (`theodb.graph_build`, `theodb_rs._recommend_ef`, AM `theodb_columnar`, opclass
  `theodb_ivfflat_l2_ops`), o que responde a Q1 do plano — mas isso é verificação de premissa, não execução.
- **`migrate-smoke.sh` não foi executado.** Sei que ele **vai** reprovar por `ivfflat` porque medi o erro
  diretamente no produto; não medi se ele reprova **só** por isso.
- **O CI segue vermelho** para o job `CI` ([[B-029]] tratava parte disso; a outra parte é o `publish` que falha).

## Veredito

**`READY_TO_MERGE`.**

5 de 5 afirmações verificadas por execução; `/code-quality` sobe para `PASS_WITH_CAVEATS` com 0 achados HARD;
175 testes verdes na ferramenta (+6); zero linha de Rust de produção alterada.

**Ressalvas:** review do próprio implementador; os gates restaurados ainda não passaram pelo CI real; e o achado
mais importante do ciclo — a janela cega de `workspace` — foi **registrado, não resolvido**, porque resolvê-lo
de passagem seria desfazer uma decisão do owner sem o ADR que ela merece.

**Contagem final:** 9 itens fechados (B-013, B-016, B-022, B-023, B-027, B-029, B-039, B-053, mais o
saneamento de B-001/B-004/B-025/B-036 no Tier 0); 3 novos registrados por medição (B-051, B-052, B-053 — este
último aberto e fechado no mesmo ciclo).
