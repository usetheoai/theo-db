---
slug: b059-omni-adapter
item: B-059
repo: theodb-bench
date: 2026-08-17
base: 84a4143
head: 744b2d9
verdict: pending
measured_on: droplet theo-b059-bench · 138.197.22.192 · s-8vcpu-16gb · nyc3
---

# Review — B-059 · o adapter que expôs um buraco no portão que o antecedeu

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte | **667 passed; 1 skipped; 0 failed** (era 637 no merge do B-060 — 30 novos) |
| 2 | `mypy --strict` | **limpo**, 37 arquivos |
| 3 | `ruff check src/ tests/` | **All checks passed** (com o ruff **do venv do projeto**, 0.16.2 — ver R-5) |
| 4 | `ruff format --check` | limpo |
| 5 | Prova por reprovação | 3 conjuntos, todos verificados removendo o conserto — § Cross-validation |
| 6 | Segredos commitados | **0** |
| 7 | Idioma do repo | inglês em código, docstrings, CHANGELOG e commits — per `CLAUDE.md` do `theodb-bench` |
| 8 | `CHANGELOG.md` atualizado | sim — uma seção por categoria, consolidada (ver R-6) |
| 9 | Schemas versionados | **nenhum bump** |

## Cross-validation

| # | Afirmação | Como foi verificada | Resultado |
|---|---|---|---|
| C1 | opclasses do `scann` são `cosine`/`dot_product`/`l2` | `pg_opclass ⋈ pg_am` no servidor real | confirmado; nenhum casa com `vector_*_ops` |
| C2 | `quantizer='sq8'` é aceito e é string | `CREATE INDEX … WITH (num_leaves=10, quantizer='sq8')` + `pg_class.reloptions` | `num_leaves=10 · quantizer=sq8` gravado |
| C3 | o `LOAD` é necessário e o portão o prova | removido o `LOAD` → **2 testes reprovam**, um deles pelo portão em `postgres.py` | provado por reprovação |
| C4 | versão vem do servidor | `select version()` → `PostgreSQL 17.9`; bundle carrega `17.9 / alloydb_scann 0.1.4` | lida, não inferida |
| C5 | `capabilities()` não alega plataforma | teste assere ausência de `disaggregated_storage`, `managed_failover`, `read_pool` | 4 ausências asseridas |
| C6 | `alloydbomni` visível ao CLI | `theodb-bench list` no droplet | reportado com a descrição medida |
| C7 | caminho `scann` ponta a ponta | 7 passos contra o servidor real: extensão, 5 000 linhas, build 0,07 s, portão, busca 10 ids, plano nomeia o índice, versão | 7 de 7 |
| C8 | knob não-mapeado é recusado | a mesma corrida que publicou 3 pontos idênticos → **`INVALID`** | provado contra servidor real |
| C9 | capability fora do vocabulário é pega sem servidor | removido `vector_scann` de `CAPABILITIES` → `test_every_declared_capability_is_a_known_one[alloydbomni]` reprova | provado por reprovação |

## Achados

### R-1 — CRÍTICO · O portão do B-060 tinha um buraco que só um segundo motor revelaria, e a primeira corrida caiu nele

A corrida `alloydbomni` do `vector/synthetic/sweep` produziu bundle **`VALID`** com estas linhas:

```
alloydbomni  hnsw m=16 ef_search=16    qps=455.0  recall=0.8234
alloydbomni  hnsw m=16 ef_search=64    qps=462.6  recall=0.8218
alloydbomni  hnsw m=16 ef_search=256   qps=554.7  recall=0.8208
```

Recall **plano** num intervalo de 16× de `ef_search`, com QPS *subindo*. O pgvector, no mesmo comando e na mesma
máquina, fez a curva correta: `0,6148 → 0,9000 → 0,9908`. Recall é determinístico dado `ef_search`; plano
significa que o parâmetro não fez nada.

Medido com oráculo próprio, 100 consultas, mesmo índice, mesmo corpus:

| | portão diz em vigor | recall@10 ef=16 | recall@10 ef=256 |
|---|---|---|---|
| pgvector | `{'hnsw.ef_search': …}` | 0,5570 | **0,9970** |
| alloydbomni | **`{}`** | 0,7820 | **0,7820** |

Duas causas compostas: o fork do pgvector que o Omni empacota **não registra nenhum GUC `hnsw.*`** (zero linhas
em `pg_settings`), e o `_search_guc_mapping` do meu adapter mapeia `num_leaves_to_search`, não `ef_search`.
**Um parâmetro pedido que não entra no mapeamento nunca era verificado** — o portão recebia `{}`, não tinha o que
checar, e passava por vacuidade.

O conserto reusa o raciocínio que o próprio arnês já aplicava um nível acima. `VectorBenchmark.sweep_for` recusa
varrer busca exata com a razão escrita: *"sweeping it would produce duplicate points under different labels"*.
É literalmente o mesmo defeito, um degrau abaixo. Cada adapter declara `SEARCH_PARAMETERS`, e o pedido que
nomeia outra coisa reprova a corrida.

**Prova:** o mesmo comando que publicou as três linhas fictícias agora reporta **`INVALID`**.

Vale nomear o que isto diz do B-060: o portão que ele entregou estava **certo e incompleto**, e nenhum teste com
duplo o teria revelado — precisou de um motor cujo vocabulário de knobs difere. É o argumento mais forte deste
ciclo a favor de rodar contra servidor real, e a ressalva mais séria que o review do B-060 registrou
("o gate não foi exercitado contra um servidor real") cobrando o preço.

### R-2 — CRÍTICO · O `assert_index_used` que o B-060 citou como exemplar é código morto e está quebrado

Descoberto ao tentar usá-lo no C7. Quatro fatos, todos por execução ou grep:

1. **Nenhum chamador.** `grep -rn assert_index_used src/ tests/` devolve a definição e dois comentários. Zero
   invocações. `src/bench/vector.py` não verifica plano de forma alguma.
2. **Quebrado se chamado.** `PgvectorAdapter` sobrescreve `_query_sql` repetindo a expressão de distância no
   `ORDER BY` — 2 placeholders — e sobrescreve `execute` para ligar a sonda duas vezes, com o comentário
   explicando por quê. `assert_index_used`, herdado, liga **uma**:
   `psycopg.ProgrammingError: the query has 2 placeholders but 1 parameters were passed`.
3. **`SET enable_seqscan = off` nunca é emitido.** A string existe uma vez no repositório: no docstring.
4. Logo o invariante I5 anunciado em `postgres.py:10` — *"The index is forced **and** verified"* — é **falso nas
   duas metades**.

**O que a medição impede de exagerar:** nenhum número publicado é retratado. No tamanho exato da suíte registrada
(10 000 × 64), `EXPLAIN` confirma `Index Scan` com o nome do índice no plano nos três — `pgvector hnsw`,
`alloydbomni hnsw`, `alloydbomni scann`. O buraco é **latente**. O que o torna real: 200 linhas produziram
`Seq Scan` com índice presente, e o [[B-018]] já registra um JOIN onde nem `enable_seqscan = off` alcança.

Registrado como [[B-063]]. A oportunidade do B-060 foi **corrigida por acréscimo** — ela abriu chamando este
método de *"o padrão certo, que já existe"* e de *"disciplina exata"*, e construiu o portão por analogia a ele.
A analogia era boa; o exemplar estava morto. Neste ciclo o docstring parou de afirmar o que o código não faz;
consertar o mecanismo é o B-063.

### R-3 — ALTO · Um teste de contrato que enumerava o que dizia cobrir universalmente

`test_every_adapter_reports_effective_search_parameters` estava
`@pytest.mark.parametrize("name", ["postgres", "pgvector", "theodb", "fake"])`. Medido: **4 passed** com o
`alloydbomni` já registrado e descoberto. Um teste cuja docstring diz *"a new engine cannot be added without
answering what is in force"* excluía todo adapter acrescentado depois de ele ser escrito — e reportava verde por
isso. Passou a derivar de `ADAPTERS`; cobre 5 e cobrirá o sexto sem edição.

Escrevi, junto, um "guarda do guarda" que era tautológico (`set(sorted(X)) == set(X)`, sempre verdade) e o
removi em vez de deixá-lo verde: com a parametrização derivada a deriva é estruturalmente impossível, e o
degrau 1 da parsimony ladder responde "não".

### R-4 — ALTO · Uma capability fora do vocabulário central, encontrada no lugar errado

`AlloyDBOmniAdapter.capabilities()` devolvia `vector_scann`, e `CAPABILITIES` (`base.py:41`) não o listava.
O teste unitário que assere o **conteúdo** do dict passou; `build_index` reprovou no **primeiro build real** com
`unknown capability 'vector_scann'`. Asserir o dict prova o que o adapter *diz*; só a checagem contra o
vocabulário prova que a corrida pode usá-lo.

`vector_scann` entrou no vocabulário — distinto de `vector_hnsw` de propósito, porque o Omni traz **os dois** e
confundi-los mediria o fork sob o nome do motor. E entrou um teste parametrizado sobre `ADAPTERS` que reprova
quando a capability sai do vocabulário (verificado removendo-a).

### R-5 — MÉDIO · Rodei o gate errado por 20 minutos

Usei `python3 -m ruff` (global, **0.11.12**) em vez de `.venv/bin/ruff` (**0.16.2**, que é o do projeto e o do
CI). O global reportava 9 achados, incluindo `UP038` e `S603` em arquivos que eu não toquei — regras que a versão
do projeto trata de outra forma. Corrigido; o gate real reporta 4 achados, todos meus, todos `RUF012`.

É a mesma classe de erro que `cargo check` sem `pg_test`: **usar um gate mais barato que o que importa**. Vale o
registro porque o sintoma foi enganoso — "9 erros, a maioria pré-existentes" convida a descartar o ruído em vez
de desconfiar da ferramenta.

Os 4 `RUF012` estavam certos: `OPCLASSES` e `SEARCH_PARAMETERS` são atributos mutáveis de classe, e `ClassVar` é
exatamente o que a decisão de lê-los da classe significa. A anotação passou a **expressar** o desenho que o
teste `test_setting_opclasses_on_an_instance_does_not_change_the_lookup` fixa.

### R-6 — MÉDIO · O CHANGELOG ganhou seções duplicadas, e eu as consolidei

O `cat >>` inseriu `### Fixed` **depois** da definição de link `[Unreleased]:`, e já existiam `### Changed` e
`### Fixed` no `[Unreleased]`. Ficaram quatro headers para duas categorias. Consolidado em uma seção por
categoria, per `Keep a Changelog`.

### R-7 — BAIXO · Duas seams que inventei e removi antes de commitar

- `_session_prelude()` — inventada para facilitar o teste do `LOAD`. Ler `PostgresAdapter.wait_ready` mostrou que
  ele só chama `_execute("SELECT 1")`, então o duplo já cobre a cadeia inteira e o override do `wait_ready` é
  **uma linha**. Inventar seam de produção para resolver problema de teste é o degrau 1 respondido errado.
- `_create_extension_statement()` — escrita e sem chamador. Removida no mesmo turno. Dado o R-2, escrever um
  segundo método sem chamador neste arquivo seria particularmente irônico.

### R-8 — INFORMATIVO · Dois fatos medidos que pertencem a itens seguintes

- **`scann.enable_ah_quantizer = off` por default.** O AH é o mecanismo que o `ADR-0035` aponta como razão do gap
  de 25-44×, e no Omni é opt-in. Uma corrida `theodb × scann` no default mediria o ScaNN **sem o que o torna
  ScaNN**. O adapter aceita o knob; quem mede é o [[B-057]].
- **`google_columnar_engine` vem instalado E pré-carregado** (`shared_preload_libraries`). O [[B-058]] precisa
  **desligar** para medir "Omni off", não deixar de instalar.

### R-9 — INFORMATIVO · O runner classifica recusa do adapter como `sut_alive`

A corrida recusada reportou `Run is INVALID: sut_alive`. O sistema está vivo; o **pedido** era inválido. A
recusa funciona e bloqueia o número, que é o que importa; a taxonomia de falha do runner é imprecisa aqui e está
fora do escopo deste item.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **A corrida das três vias ainda não fechou.** `pgvector` produziu bundle `VALID`; o eixo `theodb` depende de um
  build de imagem que ainda compilava ao escrever isto. Sem ele, o critério final do DoD do B-059 está **aberto**,
  e este review não pode declará-lo cumprido.
- **Nenhuma corrida indexada válida do Omni existe**, e a razão é honesta: nenhuma suíte registrada varre
  `num_leaves_to_search`. O caminho `scann` está provado ponta a ponta pelo C7 (build, portão, busca, plano
  nomeia o índice), mas isso é uma verificação de integração, não um bundle. Registrar uma suíte com parâmetros
  que ninguém mediu é o que a Q1 do plano recusou; medi-los é o B-057.
- **`assert_index_used` continua morto.** Este ciclo parou a afirmação falsa; o mecanismo é o B-063.
- **`num_leaves` não é derivado da cardinalidade** (Q1 do plano) — passável, não derivado.
- **O AM `ivf` próprio do Omni não é coberto** (Q2 do plano).

## Veredito

**Pendente** — 9 de 9 afirmações verificadas por execução, 667 testes, mypy strict e ruff limpos, e três
consertos provados por reprovação. Mas o **último critério do DoD** (corrida das três vias com bundle válido)
não fechou: o eixo `theodb` aguarda o build da imagem. O veredito não pode ser `READY_TO_MERGE` antes disso —
declará-lo agora seria exatamente o `cobertura-alegada-sem-execucao` que este repositório rastreia.
