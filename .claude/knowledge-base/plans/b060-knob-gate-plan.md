---
slug: b060-knob-gate
item: B-060
repo: theodb-bench
date: 2026-08-16
upstream: .claude/knowledge-base/discoveries/opportunities/b060-knob-gate-opportunity.md
---

# Plano — o arnês passa a exigir do knob de busca a mesma prova que já exige do índice

## Goal

Fazer o `theodb-bench` **recusar a medição** quando um parâmetro de busca pedido não está em vigor no servidor,
com a mesma disciplina que o `assert_index_used` já aplica ao plano — e registrar no bundle o valor **efetivo**
ao lado do pedido, para que um leitor futuro possa distinguir os dois.

O que este plano **não** faz: acrescentar motor novo (é o [[B-059]]) nem suite nova (é o [[B-061]]). Ele fecha
uma lacuna do framework que existe hoje para todos os adapters, inclusive os nossos.

## Baseline Context

### O estado medido

| Fato | Onde | Valor |
|---|---|---|
| verificação do plano | `src/adapters/postgres.py:325` | `assert_index_used` **existe** e levanta `AdapterError` |
| verificação do knob | `:279-280` e `:486-493` | **não existe** — o `SET` é emitido e nada é lido de volta |
| pedido vs vigente no *build* | `:483` | `BuildOutcome.parameters_in_force` **existe** |
| pedido vs vigente na *busca* | — | **não existe** |
| chamador único | `src/bench/vector.py:347` | `adapter.set_search_parameters(search)` |
| adapters afetados | `postgres`, `pgvector`, `theodb`, `fake` | 4 |
| suíte atual | — | **627 testes** |

### Files that will be touched

| Arquivo | Papel |
|---|---|
| `src/adapters/postgres.py` | `PostgresAdapter` ganha aplicar-e-verificar; `PgvectorAdapter` passa a **declarar** o mapa em vez de emitir `SET` solto |
| `src/adapters/base.py` | contrato: `effective_search_parameters()` |
| `src/adapters/fake.py` | participa do contrato (é o duplo que os testes do runner usam) |
| `tests/` | o teste que **reprova** um adapter que aceita e ignora |

Estado do git: `b6a5bfd` em `workspace` (repo `theodb-bench`).

### Current callers / dependents

- `src/bench/vector.py:347` é o **único** chamador de `set_search_parameters` — a superfície de mudança é estreita
- `FakeAdapter` é usado pelos testes do runner; se ele não participar do contrato, o gate fica sem cobertura no
  caminho que mais roda
- Nenhum bundle já publicado é invalidado: para `hnsw`/`ivfflat` o `SET` de fato aplicou, porque são namespaces
  registrados pela extensão. **O que muda é a garantia, não os números**

### Architecture boundaries affected

- `src/adapters/` é a fronteira com o sistema medido — é onde validação de fronteira pertence
  (`rules/architecture.md § 2`)
- `rules/error-handling.md § 2`: "retorne erros explícitos em vez de valores mágicos". Guardar o pedido e
  publicá-lo como vigente é precisamente o valor mágico
- **Os 11 schemas são versionados.** Acrescentar o efetivo ao bundle é campo novo → decisão declarada na D3

### Domain glossary

- **knob de busca** — parâmetro que muda o ponto de operação da consulta sem rebuild (`ef_search`, `probes`,
  `num_leaves_to_search`). É o eixo do trade-off recall × QPS.
- **efetivo** — o valor que o servidor de fato tem em vigor, lido de volta dele. Distinto do **pedido**.
- **placeholder GUC** — namespace não registrado que o PostgreSQL aceita sem aplicar. É o mecanismo do defeito.

## Prior Art

- **`assert_index_used`** (`postgres.py:325`) — o padrão que este plano espelha, com a razão no docstring.
- **`BuildOutcome.parameters_in_force`** (`:483`) — a prova de que o projeto já sabe distinguir pedido de
  vigente; só não o faz na busca.
- **[[B-034]]** (theo-db) — `SET hnsw.ef_search` aceito em silêncio sem efeito. Nossa própria medição da classe.
- **Avaliação independente de AlloyDB, 2026-08-15** — `scann.num_leaves_to_search` sem efeito sem `LOAD`:
  recall 0,15 e uma corrida de 10M vetores perdida.

## Drawbacks & Risks

| # | Risco | Prob. | Mitigação |
|---|---|---|---|
| R1 | O gate reprova corridas legítimas por diferença de formatação (`"64"` vs `64`, `on` vs `true`) | **alta** — `current_setting` devolve texto | A comparação normaliza pelo tipo declarado no mapa, e o teste cobre inteiro, booleano e enum |
| R2 | Um GUC legitimamente clampado pelo servidor é lido como divergência | média — o próprio `PgvectorAdapter` já clampa `probes` | O mapa declara o valor **que foi enviado** (pós-clamp), não o pedido bruto; e o bundle carrega os dois |
| R3 | Acrescentar campo ao bundle quebra os 11 schemas versionados | média | D3 decide: o efetivo entra em campo novo com bump, ou fica fora do bundle e só no gate. **Não** se acrescenta campo sem bump |
| R4 | `FakeAdapter` participar do contrato torna os testes do runner mais rígidos | baixa | Ele é o duplo; se o contrato não vale nele, o gate não tem cobertura onde mais roda |
| R5 | O gate vira mais um que sempre passa | média | Provado **reprovando** um duplo que aceita e ignora — a prova por reprovação que o [[B-029]] estabeleceu |

## Unresolved Questions

- Q1 — `current_setting('hnsw.ef_search', true)` devolve `NULL` ou string vazia para namespace não registrado? A
  T1.1 **mede** contra um PostgreSQL sem a extensão, em vez de supor: os dois casos têm de ser tratados.
- Q2 — Há GUC de busca cujo valor efetivo legitimamente difere do enviado, além do clamp de `probes`? A T1.2
  varre os adapters existentes e conta, em vez de assumir que é só um.
- Q3 — O efetivo entra no bundle (bump de schema) ou fica só no gate? A D3 decide, e a decisão é declarada.

## ADRs

### D1 — Template Method: a base aplica e verifica; a subclasse só declara o mapa

**Decisão.** `PostgresAdapter` passa a possuir o algoritmo — *aplicar → ler de volta → recusar se divergir →
registrar* — e as subclasses fornecem apenas o mapeamento `nome lógico → (GUC, valor enviado, tipo)`.

```
PostgresAdapter.set_search_parameters(params)      # final, não sobrescrito
    ├── mapping = self._search_guc_mapping(params) # hook: a subclasse declara
    ├── for each: SET <guc> = <value>
    ├── assert_search_parameters_applied(mapping)  # lê de volta, recusa se divergir
    └── self._effective_search_parameters = ...
```

**Alternativas consideradas.**

- *Cada subclasse verifica a sua.* Rejeitada: duplica a verificação em cada motor novo, e é exatamente assim que
  um deles esquece. O `PgvectorAdapter` hoje duplica os `SET`; o Omni duplicaria de novo.
- *Um decorator sobre `set_search_parameters`.* Rejeitada por KISS: acrescenta indireção para resolver o que
  herança já resolve, e o método tem um único chamador.
- *Verificar dentro de `execute`, por consulta.* Rejeitada por custo: seria uma leitura extra por consulta num
  caminho medido em QPS. A verificação pertence ao ponto em que o parâmetro muda.

**Por que Template Method e não Strategy:** o que varia entre motores é **dado** (qual GUC, qual literal), não
**comportamento**. Strategy seria uma classe por motor para devolver um dicionário — abstração sem variação de
algoritmo, e o degrau 1 da parsimony ladder a recusa.

### D2 — O que a divergência significa é decidido pelo tipo, não por comparação de string

**Decisão.** O mapa declara o tipo (`int`, `bool`, `enum`), e a comparação normaliza por ele. `current_setting`
sempre devolve texto: `64` volta `"64"`, `on` pode voltar `"on"`.

**Razão.** R1 é o risco mais provável do plano. Um gate que reprova por `"64" != 64` é pior que nenhum gate,
porque ensina a desligá-lo.

**Alternativas consideradas.**

- *Comparar sempre como string.* Rejeitada: `SET x = 1` pode ler de volta `"on"` num booleano.
- *Comparar sempre como inteiro.* Rejeitada: não cobre booleano nem enum, e o ScaNN traz `quantizer='sq8'`.
- *`pg_settings` em vez de `current_setting`.* **Considerada, medida na T1.1, e passou a ser a ÚNICA correta** —
  ver a correção abaixo. O raciocínio original desta linha estava invertido: eu supunha que `pg_settings` *não*
  listar placeholder o desqualificaria, quando é precisamente o que o qualifica.

> **CORREÇÃO POR ACRÉSCIMO — 2026-08-16, T1.1 EXECUTADA. A D2 estava construída sobre o instrumento errado.**
>
> A D2 propunha ler o efetivo com `current_setting`. **Medido, ele não serve** — e o modo de falha é o pior
> possível para um gate:
>
> ```
> postgres:18-bookworm puro
>   SET nao.existe = 999;                             → SET      (sucede)
>   SELECT current_setting('nao.existe', true);        → 999      (ecoa o que eu escrevi)
>   SELECT count(*) FROM pg_settings WHERE name='nao.existe';  → 0
> ```
>
> Um gate sobre `current_setting` pediria 200, leria 200, e mediria o default: **falso-negativo perfeito**.
> `pg_settings` é a autoridade, porque lista apenas GUC **registrado**.
>
> **E há um terceiro fato, medido no nosso próprio produto, que é o mais valioso dos três:**
>
> ```
> theodb:b036, sessão nova
>   SELECT count(*) FROM pg_settings WHERE name LIKE 'theodb%';   → 0
>   LOAD 'theodb_rs';
>   SELECT count(*) FROM pg_settings WHERE name LIKE 'theodb%';   → 38
>   SET theodb_hnsw.ef_search = 200;
>   → setting=200  source=session
> ```
>
> Os 38 GUCs só existem **depois** de a biblioteca carregar na sessão. Antes disso, `SET theodb_hnsw.ef_search`
> é placeholder e não faz nada — **a mesma condição que faz o `scann.num_leaves_to_search` do AlloyDB falhar em
> silêncio existe no TheoDB**. A avaliação independente encontrou no concorrente o que nós temos aqui.
>
> O gate passa a ser: **(a)** o GUC está em `pg_settings` (registrado, logo a biblioteca carregou);
> **(b)** `setting` casa o valor enviado; **(c)** `source` é `session`, não `default` — porque um GUC registrado
> cujo `source` seja `default` significa que o `SET` não pegou.
>
> Isto também **substitui** a alternativa "`pg_settings` para diagnóstico" da D2: ele deixou de ser opção
> preferível e passou a ser o único instrumento correto.

### D3 — O efetivo entra no bundle **com** bump de schema, ou não entra

**Decisão.** Se o valor efetivo for para o bundle, o schema correspondente é bumpado no mesmo commit. Se o bump
for julgado caro demais neste item, o efetivo fica **só no gate** e isso é registrado — nunca um campo novo em
schema versionado sem bump.

**Razão.** Os 11 schemas versionados são a promessa de reprodutibilidade do arnês. Acrescentar campo em silêncio
quebra a leitura de um bundle antigo por um validador novo, ou vice-versa.

**Alternativas consideradas.**

- *Acrescentar como campo opcional sem bump.* Rejeitada: "opcional" é como um schema versionado deixa de ser
  contrato.
- *Só no log, não no bundle.* Rejeitada como destino final — o bundle é o artefato; log não é contrato. Aceita
  como **estado intermediário declarado** se o bump não couber aqui.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa |
|---|---|---|
| C1 | Um parâmetro pedido e não vigente **recusa** a medição | T1.1, T1.3 |
| C2 | O efetivo é lido do servidor, não inferido | T1.1 |
| C3 | A verificação vale para **todo** adapter, não só um | T1.2, T1.4 |
| C4 | O gate é provado **reprovando** | T1.3 |
| C5 | O bundle distingue pedido de efetivo, ou a ausência é declarada | T1.5 (D3) |

## Tasks

### T1.1 — Medir como o PostgreSQL responde ao placeholder, antes de escrever o gate

#### Why this step

O gate inteiro depende de uma resposta que eu não tenho: `current_setting('hnsw.ef_search', true)` contra um
servidor **sem** a extensão devolve `NULL`, string vazia, ou levanta? E `pg_settings` lista placeholder? Q1 e Q2
são perguntas, não suposições, e escrever o gate antes de medi-las é adivinhar o mecanismo que ele existe para
pegar.

#### TDD

Medição contra dois contêineres — `postgres:18-bookworm` puro (sem extensão) e `theodb:b036`:

```sql
SET hnsw.ef_search = 200;                          -- sucede? erra?
SELECT current_setting('hnsw.ef_search', true);    -- devolve o quê?
SELECT count(*) FROM pg_settings WHERE name = 'hnsw.ef_search';
```

#### Concurrency tests

(none — single-threaded) Leitura de catálogo numa sessão.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| `current_setting` levanta em vez de devolver NULL | o gate trata a exceção como divergência, não a propaga crua |
| O servidor aceita e clampa | é divergência **legítima**: o mapa declara o enviado, e o efetivo vai ao lado |

#### Acceptance criteria

- o review **contains** a saída literal das três consultas nos dois contêineres — `grep -c "current_setting -> \[999\]"` no review `equals` 1
- a escolha entre `current_setting` e `pg_settings` é justificada **pelo que foi medido**, não por preferência
- o docstring do gate **contains** as strings `pg_settings` e `placeholder` — `grep -c` no arquivo do gate `equals` 1 para cada

### T1.2 — Contar quantos knobs existem e quantos clampam, antes de generalizar

#### Why this step

Q2. A D2 assume que o clamp de `probes` é o único caso em que o efetivo difere legitimamente do pedido. Assumir
é como o gate nasce com falso positivo.

#### TDD

```bash
grep -nE "SET [a-z_]+\.[a-z_]+" src/adapters/*.py    # todos os knobs emitidos hoje
grep -nE "clamp|min\(|max\(" src/adapters/postgres.py # onde o valor é transformado
```

#### Concurrency tests

(none — single-threaded) Leitura estática.

#### Acceptance criteria

- `grep -cE "SET [a-z_]+\.[a-z_]+" src/adapters/*.py` **returns** um número, e o review lista esse mesmo número de knobs com etiqueta `igual` ou `transformado`
- `pytest -k clamp` **returns** `0 failed` com `>= 1` teste coletado, provando que o gate aceita o valor transformado
- o número de knobs etiquetados **equals** o número que o `grep` devolveu — se divergir, sobrou um sem etiqueta

### T1.3 — O gate, provado REPROVANDO

#### Why this step

É o item. E a prova tem de ser por reprovação: um gate exercitado só no caminho feliz é o `assert_index_used`
sem o `EXPLAIN`.

#### TDD

RED — um duplo que aceita o `SET` e ignora:

```python
def test_gate_refuses_an_adapter_that_accepts_the_knob_and_ignores_it():
    """A silent no-op SET is the exact mechanism B-034 measured in TheoDB and the
    independent AlloyDB evaluation measured in ScaNN. The gate must refuse, not report."""
    adapter = _IgnoringAdapter()          # SET succeeds; current_setting returns the default
    with pytest.raises(AdapterError, match="requested .* effective"):
        adapter.set_search_parameters({"ef_search": 200})

def test_gate_accepts_when_the_server_actually_applied_it():
    adapter = _HonestAdapter()
    adapter.set_search_parameters({"ef_search": 200})
    assert adapter.effective_search_parameters() == {"ef_search": 200}

def test_gate_accepts_a_documented_clamp():
    """probes is clamped to the list count; the effective value legitimately differs."""
    ...
```

#### Concurrency tests

(none — single-threaded) O adapter é usado por um cliente por vez; o runner paraleliza consultas, não
configuração.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| GUC não existe no servidor | recusa nomeando o GUC e o valor pedido |
| Efetivo difere por clamp declarado | **aceita**, e registra os dois |
| Conexão cai durante a leitura de volta | `SystemUnavailableError`, não divergência silenciosa |

#### Acceptance criteria

- `pytest -k refuses_an_adapter_that_accepts_the_knob_and_ignores_it` **returns** `1 passed`, e o mesmo teste contra o `HEAD` de hoje **returns** `1 failed` — verificado com `git stash`
- a mensagem do `AdapterError` **contains** as três substrings: o nome do parâmetro, `requested=` e `effective=`
- os 627 testes existentes seguem verdes, e o total **>= 630**

### T1.4 — Todo adapter participa, ou declara por escrito que não se aplica

#### Why this step

C3. Um contrato que vale para três dos quatro adapters é um contrato que o quarto quebra em silêncio — e o
quarto (`fake`) é o que mais roda, porque os testes do runner o usam.

#### TDD

```python
@pytest.mark.parametrize("name", sorted(ADAPTERS))
def test_every_adapter_reports_effective_search_parameters(name):
    """A new engine cannot be added without answering what is in force."""
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- teste parametrizado sobre `ADAPTERS` — acrescentar motor novo **sem** o efetivo reprova
- `FakeAdapter` participa de verdade (devolve o que aplicou), não com um stub que devolve o pedido

### T1.5 — O bundle distingue pedido de efetivo, ou a ausência é declarada

#### Why this step

D3. O bundle é o artefato; se ele publica o pedido como se fosse o medido, o gate protege a corrida e não o
leitor.

#### TDD

Verificação estrutural sobre o schema e o bundle gerado:

```bash
theodb-bench validate <bundle>   # exit 0 com o campo novo
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- **ou** o bundle carrega o efetivo e o schema correspondente foi bumpado no mesmo commit — `grep` da versão
  nova no schema `equals` 1
- **ou** o efetivo fica só no gate, e o `CHANGELOG.md` diz isso com essas palavras, nomeando o item que o levará
  ao bundle
- `theodb-bench validate` sai `0` num bundle novo, e continua saindo `0` num bundle antigo

## Failure scenarios

O caminho tem I/O externo (PostgreSQL via psycopg). As duas classes que já custaram neste ecossistema estão
cobertas: **o gate que sempre passa** (T1.3, provado por reprovação) e **o falso positivo que ensina a desligar
o gate** (T1.1/T1.2, medindo o placeholder e contando os clamps antes de generalizar). Os cenários de nível do
plano:

| Cenário | Comportamento exigido |
|---|---|
| A biblioteca do motor **não** carregou na sessão | `pg_settings` não lista o GUC → o gate **recusa** nomeando o GUC. É o caso medido na T1.1 (0 GUCs antes do `LOAD`, 38 depois) e é a armadilha do ScaNN |
| GUC registrado, mas `source = default` | o `SET` não pegou → recusa. Um GUC presente com `source` default significa que ninguém o alterou nesta sessão |
| GUC registrado e `setting` divergente do enviado | recusa, **exceto** quando o mapa declara a transformação (o clamp de `probes`) |
| Conexão cai entre o `SET` e a leitura | `SystemUnavailableError`, nunca divergência silenciosa — a distinção entre "não pegou" e "não pude verificar" é o que o `NOT_VALIDATED` do `cycle-acceptance` protege |
| `pg_settings` inacessível (permissão) | recusa declarando que **não pôde verificar** — não é o mesmo que verificado-e-divergente |

## Definition of done

- [ ] o comportamento do placeholder GUC está **medido** e registrado, não suposto
- [ ] os knobs existentes estão contados e classificados (enviado-igual ou transformado)
- [ ] o gate **reprova** um adapter que aceita e ignora, provado por teste
- [ ] a mensagem nomeia parâmetro, pedido e efetivo
- [ ] todo adapter em `ADAPTERS` responde o efetivo — teste parametrizado
- [ ] o bundle distingue os dois **com** bump de schema, ou a ausência está declarada no CHANGELOG
- [ ] suíte `>= 630` testes, `0 failed`
- [ ] `CHANGELOG.md` atualizado (em inglês, per `CLAUDE.md` do repo)
