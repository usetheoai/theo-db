---
slug: tier1-portao
items: [B-029, B-039, B-016, B-023]
date: 2026-08-13
upstream: .claude/knowledge-base/discoveries/opportunities/tier1-portao-opportunity.md
---

# Plano — devolver ao produto os portões que ele perdeu, e só os que ainda têm objeto

## Goal

Fazer com que a esteira volte a ter **capacidade de reprovar**: nenhuma invocação morta, cada oráculo
restaurado provado reprovando um caso quebrado, o detector de dependências rodando onde o ambiente existe, e
os dois testes que passam por acidente convertidos em cobertura que vale.

O que este plano **não** faz: mexer no gatilho dos workflows. A janela cega de `workspace` foi medida e virou
[[B-052]], cujo DoD exige ADR porque negocia com uma decisão do owner. Resolvê-la de passagem aqui seria
desfazer decisão registrada sem o registro.

## Baseline Context

### O estado medido

| Fato | Medido | Fonte |
|---|---|---|
| invocações mortas | **10**, de **6** scripts | `grep -n` nos três workflows |
| última execução de gate em `workspace` | 2026-08-12T10:34 (`48286921`) | `gh run list --branch workspace` |
| commits sem gate desde então | **73** (13 tocam `theodb_rs/src/`) | `git rev-list --count` |
| suíte Rust hoje | **478 passed; 0 failed** | `cargo pgrx test pg18` no `theodb-toolchain` |
| baseline do `rust-suite.yml` | **0** desde 2026-08-12 | `.github/workflows/rust-suite.yml:145` |

### Files that will be touched

| Arquivo | LoC | Papel |
|---|---|---|
| `.github/workflows/ci.yml` | 39.035 B | 7 invocações mortas (`:243,364,367,370,444,520,657`) |
| `.github/workflows/schema-drift-gate.yml` | 8.283 B | 2 invocações (`:87,88`) |
| `.github/workflows/cassert-sql-safety.yml` | 5.974 B | 1 invocação (`:94`) |
| `scripts/` | **inexistente** | recriado com **3** scripts, não com os 17 removidos |
| `scripts/check-workflow-paths.sh` | novo | o verificador que impede a classe de voltar |
| `.claude/skills/code-quality/scripts/detectors/rust.py` | 250 | `:123` invoca `cargo +nightly udeps` no host |
| `theodb_rs/src/egress.rs` + `http.rs` | — | teste dedicado da guarda SSRF; disjuntor sem rede |
| `theodb_rs/src/vec.rs` | `:553` | o teste de performance que sai da suíte funcional |

Estado do git: `2c0c31c` em `workspace`.

### Current callers / dependents

- Os três workflows são disparados por `push` em `develop`/`main` — nenhum roda hoje sobre `workspace`
- `theodb_rs/sql/schema_snapshot.sql` **existe** e é o insumo do oráculo de drift; o oráculo é que saiu
- `rust-suite.yml` **não** invoca script ausente — é o único gate íntegro, e serve de modelo
- Nada em `theodb_rs/src/` depende de `scripts/`

### Architecture boundaries affected

- `.github/workflows/` e `scripts/` — infraestrutura, fora do crate
- `theodb_rs/src/{egress,http,vec}.rs` — produto; as mudanças são **de teste**, e a guarda SSRF
  (`rules/architecture.md § 2`, validação na fronteira) **não é afrouxada**
- `.claude/skills/` — tooling do ciclo, fora do produto

### Domain glossary

- **oráculo** — o que sabe dizer *errado*. Um script que só sabe passar não é oráculo, é ritual.
- **invocação morta** — linha de workflow que chama caminho inexistente. Sob `set -e` mata o job; sem ele,
  produz saída vazia que o passo seguinte lê como "sem diferença".
- **capacidade de reprovar** — a propriedade que o B-029 nomeia: um gate vale pelo caso que ele barra, não
  pelo que ele deixa passar.

## Prior Art

- **[[B-028]]** consertou o `test-upgrade.sh` porque ele declarou "TODOS OS CENÁRIOS PASSARAM" com um cenário
  pulado — a **terceira** leitura falsa do mesmo harness. É a razão de a DoD exigir prova por reprovação.
- **[[B-030]]** removeu o umbrella `theodb`. Medido hoje: só `theodb_rs` e `vector` existem em
  `pg_available_extensions`.
- **[[B-031]]** removeu a cadeia de upgrade por ADR. `theodb_rs/sql/` tem hoje 2 itens.
- **[[B-027]]** eliminou a colisão de contêiner com nome único por run — o modelo de "eliminar a classe em vez
  de remediá-la" que este plano segue.
- **`rules/testing.md § 6`** proíbe teste dependente de tempo sem isolamento — é o que decide o B-023.

## Drawbacks & Risks

| # | Risco | Prob. | Mitigação |
|---|---|---|---|
| R1 | Restaurar `smoke.sh` verbatim reintroduz um teste que falha na primeira linha | **certa** — medido: ele faz `CREATE EXTENSION theodb` 3×, e o umbrella não existe | Ele é **reescrito**, não restaurado. T1.2 declara isso e o teste prova |
| R2 | Restaurar os `migrate-*` recria oráculo sem objeto | **certa** — a cadeia saiu no B-031 com ADR | As 3 invocações **saem**; o CHANGELOG diz qual garantia o produto deixou de ter |
| R3 | O verificador de caminhos vira mais um gate que sempre passa | média | Ele é provado **reprovando** a árvore de `8605677`, que é o caso real que ninguém pegou |
| R4 | Mover o teste SIMD esconde uma regressão real | baixa — o bullet 1 do B-023 já concluiu "não reproduz, é contenção" | O teste não some: vira `#[bench]`/harness com variância declarada, e o item registra que a causa foi contenção |
| R5 | O teste dedicado da guarda SSRF acaba afrouxando a guarda | baixa | O DoD proíbe explicitamente; o teste prova a guarda **recusando**, não passando por ela |
| R6 | `cargo-udeps` no contêiner encarece o `/code-quality` | média — medido: 2 min | É o custo de ter medição em vez de `auditor_unavailable`, e o cap some |

## Unresolved Questions

- Q1 — O `cassert-smoke.sh` restaurado ainda casa com a superfície atual? Ele cita `theodb.graph_build` e os
  quatro AMs; a T1.3 verifica antes de religar a invocação, em vez de assumir.
- Q2 — O disjuntor consegue cobertura de estado sem rede nenhuma, ou precisa de um seam de injeção? A T1.5
  decide lendo o código; se precisar de seam, isso é mudança de produto e o plano diz.
- Q3 — Há outros workflows com invocação morta que o `grep` de hoje não pegou (caminho construído em
  variável)? O verificador da T1.1 é a resposta, e a T1.1 mede quantos ele encontra.

## ADRs

### D1 — Só volta o que ainda tem objeto; o resto sai com registro

**Decisão.** Dos 6 scripts ausentes:

| Script | Destino | Razão medida |
|---|---|---|
| `sql-surface.sh` (79 linhas) | **restaurado verbatim** | Só faz `git archive` + `grep` sobre `theodb_rs/src`. Não toca umbrella nem cadeia |
| `cassert-smoke.sh` (116) | **restaurado, após verificação** | Usa `CREATE EXTENSION theodb_rs` (correto) e os AMs próprios |
| `smoke.sh` (204) | **reescrito** | Faz `CREATE EXTENSION theodb` 3× — umbrella removido pelo B-030 |
| `migrate-doc-check.sh`, `migrate-smoke.sh`, `migrate-smoke-selftest.sh` | **invocação removida** | Testam a cadeia de upgrade removida pelo B-031 com ADR |

**Alternativas consideradas.**

- *Restaurar os 17 scripts de `8605677^`.* Rejeitada: 11 deles são de milestones encerrados (`m131_sweep`,
  `m139-*`, `m140-*`, `m56-crash-e2e`), e nenhum workflow os invoca. Voltar com eles é ressuscitar o que a
  limpeza corretamente removeu.
- *Reescrever os `migrate-*` para a nova realidade.* Rejeitada: não há cadeia para testar. Um oráculo sem
  objeto é o pior dos dois mundos — custa manutenção e não pode reprovar.
- *Deixar as invocações e aceitar que os jobs falhem.* Rejeitada: falha por arquivo ausente é indistinguível
  de falha real, que é exatamente a classe que o B-027 acabou de eliminar.

**Custo aceito.** O produto perde permanentemente o oráculo do caminho de atualização, e o CHANGELOG dirá
isso com essas palavras. É consequência da decisão do B-031, não deste plano.

### D2 — O verificador de caminhos é provado contra a árvore quebrada

**Decisão.** `scripts/check-workflow-paths.sh` extrai todo caminho `scripts/…`, `packaging/…`, `hooks/…`
citado nos workflows e falha se algum não resolve. Roda no `actionlint.yml`, que já existe e é barato.

**Razão.** Sem ele, a classe volta na próxima limpeza — e voltou uma vez sem ninguém notar por um dia e meio.
`actionlint` valida sintaxe de workflow, não existência de arquivo referenciado em `run:`.

**Alternativas consideradas.**

- *Confiar no `actionlint`.* Rejeitada por medição: os 10 caminhos mortos estão lá e o `actionlint` passou
  verde em 2026-08-12.
- *Um teste em Rust.* Rejeitada: a fronteira é o workflow, não o crate. Um teste do crate não vê o YAML.
- *`shellcheck`.* Rejeitada: ele analisa o script, não a existência do alvo de `bash <path>`.

**Prova exigida:** o verificador **reprova** contra `8605677`, não apenas passa contra `HEAD`.

### D3 — `cargo-udeps` roda onde o ambiente de build existe

**Decisão.** `RustDetector.detect_dead_code` invoca dentro do `theodb-toolchain` quando o host não tem pgrx
inicializado, seguindo o padrão já estabelecido para `clippy`/`fmt`.

**Razão.** Medido no B-035: no host o erro real é `/home/paulo/.pgrx/config.toml not found`, não permissão.
Dentro do contêiner: `Finished dev profile in 2m 07s` / `All deps seem to have been used.` Um cap que dispara
sempre deixou de ser sinal — quatro ciclos o declararam como "limitação de ambiente", que é a forma educada
de dizer que ninguém investigou.

**Alternativas consideradas.**

- *Instalar pgrx no host.* Rejeitada: move o problema para a máquina de cada pessoa, e o projeto já decidiu
  que o ambiente de build é o contêiner pinado.
- *Remover o detector D1 para Rust.* Rejeitada: seria apagar a medição em vez de fazê-la.
- *Manter o cap e documentá-lo.* Rejeitada — é o estado atual, e ele custou 4 ciclos de `FAIL_SOFT`.

### D4 — O teste de performance sai da suíte funcional; a guarda SSRF ganha teste próprio

**Decisão.** `vec::cosine_simd_per_candidate_speedup` sai de `#[pg_test]` e vira benchmark com variância
declarada. A guarda SSRF ganha um teste que a prova **recusando**, e o disjuntor ganha cobertura de estado
sem rede.

**Razão.** `rules/testing.md § 6` proíbe tempo/aleatoriedade em teste unitário sem isolamento. O bullet 1 do
B-023 já mediu que `avx < scalar` **não reproduz** — é contenção. Um teste que passa hoje na minha máquina e
falha amanhã na do CI treina o time a ignorar vermelho.

**Alternativas consideradas.**

- *Afrouxar o limiar do teste.* Rejeitada pelo próprio DoD: *"não é afrouxado no lugar"*. Um limiar frouxo o
  bastante para nunca falhar não mede nada.
- *Marcar `#[ignore]`.* Rejeitada: `rules/testing.md § 6` chama teste permanentemente ignorado de dívida
  invisível.
- *Deixar como está, já que passa.* Rejeitada: passar hoje é o argumento mais fraco possível para um teste
  cuja falha anterior foi diagnosticada como ambiental.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa |
|---|---|---|
| C1 | Nenhuma invocação morta | T1.1, T1.2, T1.3, T1.4 |
| C2 | Cada oráculo restaurado é provado reprovando | T1.2, T1.3 |
| C3 | O drift de superfície SQL volta a comparar | T1.3 |
| C4 | `cargo-udeps` reporta em vez de `auditor_unavailable` | T1.6 |
| C5 | Os dois testes que passam por acidente viram cobertura que vale | T1.5, T1.7 |

## Tasks

### T1.1 — O verificador que impede a classe de voltar

#### Why this step

Vem antes das restaurações porque é o que **prova** que elas ficaram completas — e porque, sem ele, a próxima
limpeza recria o problema. Medido: `actionlint` passou verde sobre os 10 caminhos mortos.

#### TDD

RED — o verificador tem de reprovar a árvore de `8605677` e passar na de hoje:

```bash
git stash && git switch --detach 8605677
bash scripts/check-workflow-paths.sh    # exit != 0, listando os 6 ausentes
```

#### Concurrency tests

(none — single-threaded) Leitura de arquivo, sem estado compartilhado.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Caminho citado em comentário, não em `run:` | não conta como invocação — só linhas executáveis |
| Caminho construído em variável (`$SCRIPT/foo.sh`) | não detectável; o script **diz** quantos ignorou, em vez de calar |
| `scripts/` inteiro ausente | reprova listando cada caminho, não "diretório ausente" |

#### Acceptance criteria

- `bash scripts/check-workflow-paths.sh` sai `0` na árvore de hoje e **diferente de 0** contra `8605677`
- a saída **contains** cada um dos 6 caminhos ausentes, com `arquivo:linha`
- o script **contains** a contagem de caminhos dinâmicos que não soube verificar
- roda em `actionlint.yml` — `grep -c "check-workflow-paths" .github/workflows/actionlint.yml` `equals` 1

### T1.2 — `smoke.sh` reescrito para o produto que existe

#### Why this step

Medido: o script removido faz `CREATE EXTENSION theodb` **3 vezes**, e `pg_available_extensions` no
`theodb:b036` lista só `theodb_rs` e `vector`. Restaurá-lo daria um smoke que falha na primeira linha.

#### TDD

RED — o smoke novo tem de **reprovar** contra uma imagem sem a extensão:

```bash
docker run -d --name smoke-neg postgres:18-bookworm
bash scripts/smoke.sh   # exit != 0, com mensagem nomeando a extensão ausente
bash scripts/smoke.sh   # contra theodb:b036 -> exit 0
```

#### Concurrency tests

(none — single-threaded) Um cliente psql por vez.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Servidor não aceita conexão | falha nomeando host/porta, não `psql: error` cru |
| Extensão presente mas AM ausente | reprova — presença da extensão não é presença da capacidade |
| Consulta devolve 0 linhas | reprova; zero resultado é a classe do [[B-041]] |

#### Acceptance criteria

- `scripts/smoke.sh` **contains** `CREATE EXTENSION IF NOT EXISTS theodb_rs` e **não contains**
  `CREATE EXTENSION IF NOT EXISTS theodb ` (o umbrella)
- contra `postgres:18-bookworm` puro, sai **diferente de 0** e a saída **contains** o nome da extensão ausente
- contra `theodb:b036`, sai `0` e verifica: tipo `vector`, AM `theodb_hnsw`, um `CREATE INDEX` e uma consulta
  que **returns** ≥ 1 linha
- as 4 invocações de `ci.yml` (`:243,444,520,657`) resolvem

### T1.3 — `sql-surface.sh` e `cassert-smoke.sh` voltam, e o drift volta a comparar

#### Why this step

`schema_snapshot.sql` sobreviveu sem o oráculo. Os dois scripts não dependem do umbrella nem da cadeia —
medido lendo os dois.

#### TDD

RED — o gate de drift tem de reprovar uma mudança de superfície sem bump:

```bash
# adiciona um #[pg_extern] fictício, não bumpa default_version
bash scripts/sql-surface.sh HEAD > /tmp/h.txt   # a superfície muda
# o gate reprova
```

#### Concurrency tests

(none — single-threaded)

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| `git archive` de revisão sem `theodb_rs/src` | sai 0 com aviso (comportamento já existente, preservado) |
| Superfície idêntica | não reprova — o gate mede drift, não mudança de arquivo |

#### Acceptance criteria

- `scripts/sql-surface.sh HEAD` **returns** lista não-vazia de símbolos
- o gate **reprova** um `#[pg_extern]` novo sem bump de `default_version` — provado adicionando um e revertendo
- `cassert-smoke.sh` roda contra `theodb:b036` e sai `0`; as extensões e AMs que ele cita **existem**
- `schema-drift-gate.yml:87,88` e `cassert-sql-safety.yml:94` resolvem

### T1.4 — As invocações sem objeto saem, com registro

#### Why this step

Os `migrate-*` testam a cadeia de upgrade removida pelo B-031 com ADR. Restaurá-los é ressuscitar um oráculo
sem objeto; deixá-los é manter invocação morta.

#### TDD

Verificação estrutural:

```bash
grep -c "migrate-doc-check\|migrate-smoke" .github/workflows/ci.yml   # equals 0
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `grep -c "scripts/migrate" .github/workflows/ci.yml` `equals` 0
- o `CHANGELOG.md` **contains** a frase que nomeia a garantia perdida: o produto deixa de ter oráculo do
  caminho de atualização, e por qual decisão
- o job que restava vazio é **removido**, não deixado com `run: true`

### T1.5 — A guarda SSRF ganha teste próprio, e o disjuntor perde a dependência de rede

#### Why this step

Os três testes passam, e dois bullets do [[B-016]] seguem abertos. A guarda hoje é exercitada **por
acidente** — como efeito colateral de um teste que queria provar outra coisa.

#### TDD

RED:

```rust
#[pg_test(error = "refusing to call 127.0.0.1 — it resolves to a blocked internal address")]
fn ssrf_guard_refuses_loopback_by_itself() { /* prova a guarda RECUSANDO, sozinha */ }

#[pg_test]
fn breaker_state_machine_without_any_network() { /* Closed -> Open -> HalfOpen -> Closed */ }
```

#### Concurrency tests

(none — single-threaded) O disjuntor é `thread_local` por backend; o teste exercita a máquina de estados de um
backend só. Cobertura multi-backend seria item próprio.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Guarda afrouxada para o teste passar | **proibido** pelo DoD — abrir SSRF para ter teste verde |
| Disjuntor precisa de seam de injeção | é mudança de produto: registrar no plano antes de escrever |

#### Acceptance criteria

- existe teste cujo **único** objeto é a guarda SSRF, e ele passa
- a máquina de estados do disjuntor tem teste que **não** faz I/O de rede — `grep` por `reqwest`/`http` no
  corpo do teste `equals` 0
- a guarda não é alterada: `git diff` em `egress.rs` na região da guarda `equals` vazio
- suíte total **>= 478** e `0 failed`

### T1.6 — `cargo-udeps` roda onde o ambiente existe

#### Why this step

Quatro ciclos declararam `auditor_unavailable_cargo-udeps` como limitação de ambiente. Medido no B-035: dentro
do contêiner o audit passa limpo em **2 min 07 s**.

#### TDD

RED — hoje o detector devolve `auditor_unavailable`; depois tem de devolver achado ou lista vazia:

```python
def test_rust_detector_falls_back_to_the_pinned_container_when_host_lacks_pgrx():
    ...
```

#### Concurrency tests

(none — single-threaded) Um subprocesso por invocação.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Docker ausente | `auditor_unavailable` **com a razão certa** ("docker não disponível"), não a genérica |
| Imagem `theodb-toolchain` ausente | idem, nomeando a imagem |
| Timeout | o timeout do contêiner é maior que o do host (2 min medidos), e declarado |

#### Acceptance criteria

- `/code-quality` sobre este repo **não contains** `auditor_unavailable_cargo-udeps`
- o veredito sobe de `FAIL_SOFT` para `PASS`/`PASS_WITH_CAVEATS`, ou o cap remanescente é **outro** e nomeado
- teste unitário do detector cobre os três modos de falha acima

### T1.7 — O teste de performance sai da suíte funcional

#### Why this step

`rules/testing.md § 6` proíbe tempo em teste unitário sem isolamento. O bullet 1 do [[B-023]] já mediu que
`avx < scalar` **não reproduz** — é contenção.

#### TDD

Verificação estrutural + a medição que o DoD pede:

```bash
grep -c "fn cosine_simd_per_candidate_speedup" theodb_rs/src/vec.rs   # equals 0 em #[pg_test]
```

E a medição por **medianas de N repetições alternadas**, não uma amostra de cada.

#### Concurrency tests

(none — single-threaded) A medição é deliberadamente serial: medir speedup sob concorrência mediria a carga.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Mediana AVX pior que escalar em máquina isolada | é achado real: vira item com profiling, não é escondido |
| Variância acima do efeito | o harness **diz** isso em vez de reportar um número |

#### Acceptance criteria

- `cosine_simd_per_candidate_speedup` não é mais `#[pg_test]` — `grep` na suíte funcional `equals` 0
- existe medição por medianas de **>= 5** repetições alternadas, com variância declarada
- a suíte funcional continua com `0 failed` e o total **não diminui** por outro motivo que não a saída deste teste

## Failure scenarios

Consolidados por tarefa. O caminho tem I/O externo (Docker, PostgreSQL, `git archive`) e as duas classes que
já custaram ciclo neste projeto estão cobertas: **falha por ausência lida como falha real** (T1.1, T1.2) e
**gate que passa sem verificar** (T1.1 D2, provado por reprovação).

## Definition of done

- [ ] `check-workflow-paths.sh` reprova contra `8605677` e passa em `HEAD`, rodando no `actionlint.yml`
- [ ] `smoke.sh` reescrito, provado reprovando contra um Postgres sem a extensão
- [ ] `sql-surface.sh` e `cassert-smoke.sh` restaurados e verificados contra a superfície atual
- [ ] as 3 invocações `migrate-*` removidas, com a garantia perdida nomeada no CHANGELOG
- [ ] teste dedicado da guarda SSRF + disjuntor sem rede; a guarda **não** alterada
- [ ] `/code-quality` sem `auditor_unavailable_cargo-udeps`
- [ ] o teste SIMD fora da suíte funcional, com medição por medianas
- [ ] suíte Rust **>= 478 passed; 0 failed**
