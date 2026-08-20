---
slug: b059-omni-adapter
item: B-059
repo: theodb-bench
date: 2026-08-17
upstream_opportunity: .claude/knowledge-base/discoveries/opportunities/b059-omni-adapter-opportunity.md
---

# Plan — B-059 · o adapter do AlloyDB Omni

## Goal

Registrar `alloydbomni` como sistema medível pelo `theodb-bench`, com **três** propriedades que a medição do
DISCOVER provou necessárias e que o código atual não tem: uma tabela de opclass própria (os nomes do `scann` não
seguem a convenção do pgvector — M4), renderização de reloption **não-inteira** (`quantizer='sq8'` — M8), e
`LOAD 'alloydb_scann'` por sessão, cuja eficácia é **provada pelo portão do [[B-060]]** em vez de suposta (M5–M7).

O `capabilities()` declara só o que este código exercita, e a versão vai ao bundle **lida do servidor** — porque
a imagem medida é **PG 17.9** enquanto o TheoDB é PG 18 (M1), e uma corrida que esconda isso compara duas majors
sem dizer.

## Baseline Context

Medido no disco em 2026-08-17, `theodb-bench` em `84a4143` (`develop`, merge do B-060).

### Files that will be touched

| Arquivo | LoC | Papel hoje | O que muda |
|---|---|---|---|
| `src/adapters/postgres.py` | 640 | `PostgresAdapter` → `PgvectorAdapter` → `TheoDBAdapter` | `OPCLASSES` vira dado da classe; `index_ddl` ganha renderização por tipo |
| `src/adapters/alloydb.py` | — | **novo** | `AlloyDBOmniAdapter` |
| `src/registry.py` | 165 | `ADAPTERS` (4 entradas) + `BENCHMARKS` (2) | uma entrada `alloydbomni` |
| `tests/test_adapter_postgres.py` | 470 | duplos `_ServerStub` / `_adapter_with` do B-060 | ganha `_OmniStub` e os casos de T1/T2 |
| `src/adapters/base.py` | 520 | contrato `SystemAdapter` | **nada** — o B-060 já o fechou |
| `src/doctor.py` | 340 | `run_doctor` lê `ADAPTERS` | **nada** — a entrada nova é lida sozinha |

### Current callers / dependents

- `opclass()` (`postgres.py:531`) é o **único** consumidor de `OPCLASSES` (`:65`) — verificado por
  `grep -rn OPCLASSES src/ tests/`, não suposto. Mover a tabela para atributo de classe tem exatamente um ponto
  de leitura a ajustar.
- `index_ddl()` (`:547`) é chamado por `build_index()` (`:562`), chamado por `bench/vector.py` no build. A
  renderização de reloption é caminho único, sem segundo produtor.
- `set_search_parameters` / `effective_search_parameters` são consumidos por `bench/vector.py:347` e
  `:pós-347` (o `point.parameters.update` que o B-060 acrescentou) — o adapter novo herda esse caminho sem
  tocá-lo.
- `ADAPTERS` é lido por `get_adapter()` (`registry.py:101`), por `run_doctor()` (`doctor.py:326`) e pelo teste
  parametrizado `test_every_adapter_reports_effective_search_parameters`, que passa a cobrir 5 sistemas.

### Domain glossary

- **reloption** — opção do `WITH (...)` de `CREATE INDEX`, gravada em `pg_class.reloptions`.
- **opclass** (operator class) — o segundo token dentro dos parênteses do `CREATE INDEX`; define quais
  operadores o índice atende.
- **placeholder GUC** — variável de namespace não registrado. O PostgreSQL **aceita** o `SET` e não aplica nada;
  `current_setting` ecoa o valor escrito, `pg_settings` não o lista.
- **AM** (access method) — implementação de índice registrada em `pg_am`. `scann`, `ivf`, `hnsw` e `ivfflat`
  coexistem na instalação do Omni.
- **AH quantizer** — asymmetric hashing, o quantizador anisotrópico que o ADR-0035 aponta como razão do gap de
  QPS do ScaNN. No Omni é `scann.enable_ah_quantizer`, e M10 mediu `off`.
- **query layer** — o que o Omni é: planner + índice + colunar sobre armazenamento PostgreSQL padrão. Não tem o
  storage desagregado do AlloyDB gerenciado.

### Architecture boundaries affected

- **Fronteira de herança** (`architecture.md § 3`): `AlloyDBOmniAdapter` entra como folha sob `PgvectorAdapter`,
  irmã de `TheoDBAdapter`. Nenhuma camada nova; a hierarquia ganha largura, não profundidade.
- **Fronteira DIP** (`architecture.md § 2`): `registry.py` continua sendo o único lugar que conhece classes
  concretas; o core importa `SystemAdapter` e recebe a instância por fábrica. O item **não** altera essa
  direção — acrescenta uma fábrica.
- **Fronteira de sessão**: a conexão é **única e persistente** (`self._connection`, criada em `:151`, fechada em
  `:181`). Um `LOAD` em `wait_ready` vale para a sessão inteira; se ela cair, o portão do B-060 é o que denuncia
  (F6).
- **Fronteira de confiança**: identificadores vindos de definição de benchmark passam por `_identifier()`
  (`:620`) e literais por `_literal()` (`:635`). A renderização de reloption string **usa `_literal`** em vez de
  interpolar — dado de benchmark é dado, e dado não escreve SQL.

## Prior Art

- **`PgvectorAdapter`** — a herança certa: o tipo `vector` e os operadores `<=>`/`<->`/`<#>` do Omni vêm de um
  **fork do pgvector 0.8.2** (M2), então `column_type`, `_to_column` e `distance_expression` já servem.
- **`.claude/knowledge-base/reviews/b060-knob-gate-review-2026-08-16.md` § R-6** — previu este item nominalmente:
  *"o risco R1 continua real para um motor futuro cujo GUC seja booleano ou enum — o ScaNN traz
  `quantizer='sq8'`"*. O que era diferido virou exigido.
- **Template Method do B-060** (`_search_guc_mapping`) — mesma forma reaproveitada: a base é dona do algoritmo,
  a subclasse declara o dado.
- **`rules/parsimony-ladder.md`** — o degrau 5 é o que decide `OPCLASSES` como atributo de classe em vez de
  método-fábrica: é dado, e dado como atributo é o idioma.

## Coverage Matrix

| # | Afirmação do Goal | Task |
|---|---|---|
| G1 | tabela de opclass própria por adapter | T2 |
| G2 | reloption não-inteira renderizada; inválida levanta `AdapterError` | T1 |
| G3 | `LOAD 'alloydb_scann'` emitido, e provado pelo portão | T3, T5 |
| G4 | `capabilities()` honesto — query layer, sem storage desagregado | T3 |
| G5 | versão lida do servidor, não da tag | T3 |
| G6 | `alloydbomni` registrado e visível ao `doctor` | T4 |
| G7 | corrida `theodb × alloydbomni × pgvector` produz bundle válido | T6 |

100% — nenhuma afirmação sem task.

## ADRs

Nenhuma decisão de arquitetura nova: o item **consome** a forma que o ADR do B-060 estabeleceu. A escolha de
herdar `PgvectorAdapter` em vez de `PostgresAdapter` é derivada de M2 (o `vector` do Omni é fork do pgvector),
não uma decisão em aberto.

---

## Phase 1 — a base aprende dois tipos de dado

### T1.1 — `index_ddl` renderiza reloption por tipo, e recusa o que não sabe

**Why this step.** M8 mediu `quantizer='sq8'` como aceito pelo Omni, e `postgres.py:555` faz
`f"{key} = {int(value)}"`. `int('sq8')` levanta `ValueError` cru — sem `ErrorContext`, sem fase, sem sistema.
É o degrau 3 da hierarquia de `error-handling.md` (falhar claro) quebrado por um `int()`.

#### TDD

```
RED   test_a_string_reloption_is_rendered_quoted
        adapter.index_ddl(spec, IndexSpec(kind="scann", parameters={"quantizer": "sq8"}))
        assert "quantizer = 'sq8'" in ddl      # falha hoje com ValueError

RED   test_an_unrenderable_reloption_raises_adapter_error_not_value_error
        with pytest.raises(AdapterError) as exc:
            adapter.index_ddl(spec, IndexSpec(kind="hnsw", parameters={"m": [1, 2]}))
        assert exc.value.context.phase is Phase.INDEX_BUILD

RED   test_an_int_reloption_is_still_rendered_bare
        assert "m = 16" in ddl                 # regressão: não quebrar o caminho existente
```

**Acceptance.** `int`/`bool` sem aspas; `str` com aspas e escape via `_literal` (que já existe, `:635`);
qualquer outro tipo → `AdapterError` com `phase=Phase.INDEX_BUILD` e o nome da opção na mensagem.

### T1.2 — a tabela de opclass passa a ser declarada pela subclasse

**Why this step.** M4: `scann` usa `cosine` / `dot_product` / `l2`. A `OPCLASSES` de módulo (`:65`) só conhece a
convenção do pgvector, e `opclass()` (`:533`) lê o módulo. Herdar essa tabela emitiria
`USING scann (emb vector_cosine_ops)`.

#### TDD

```
RED   test_an_adapter_declares_its_own_opclasses
        class _Stub(PgvectorAdapter):
            OPCLASSES = {"scann": {"cosine": "cosine"}}
        assert _Stub(...).opclass("scann", "cosine") == "cosine"

RED   test_an_unknown_metric_still_names_what_is_available
        with pytest.raises(UnsupportedCapabilityError) as exc:
            adapter.opclass("scann", "hamming")
        assert "cosine" in str(exc.value)      # a mensagem lista o que EXISTE
```

**Acceptance.** `OPCLASSES` é atributo de classe em `PgvectorAdapter` com o conteúdo de hoje; `opclass()` lê
`type(self).OPCLASSES`; o módulo mantém o nome apontando para a mesma tabela (consumidor único, mas o rename
gratuito é ruído).

---

## Phase 2 — o adapter

### T2.1 — `AlloyDBOmniAdapter`

**Why this step.** É o entregável. Herda `PgvectorAdapter` porque M2 provou que o `vector` do Omni é fork do
pgvector 0.8.2 — o tipo, os operadores e o formato de entrada já servem, e reescrevê-los seria reinventar
(Regra 9).

#### TDD

```
RED   test_wait_ready_creates_the_extension_and_loads_the_library
        # o LOAD é o que M5-M7 provaram necessário; sem ele o SET é placeholder
        assert "CREATE EXTENSION IF NOT EXISTS \"alloydb_scann\" CASCADE" in server.statements
        assert "LOAD 'alloydb_scann'" in server.statements

RED   test_capabilities_declares_only_what_this_code_exercises
        caps = adapter.capabilities()
        assert caps["vector_scann"] is True
        # o Omni é query layer: NÃO tem storage desagregado / read pool / failover
        assert "disaggregated_storage" not in caps
        assert "managed_failover" not in caps

RED   test_the_version_comes_from_the_server_not_from_the_image_tag
        server.rows[("select version()",)] = ("PostgreSQL 17.9 on x86_64...",)
        assert "17.9" in adapter.export_config()["version"]
        assert "latest" not in adapter.export_config()["version"]

RED   test_the_search_knob_is_the_scann_guc
        adapter.set_search_parameters({"num_leaves_to_search": 500})
        assert adapter.effective_search_parameters() == {"scann.num_leaves_to_search": "500"}
```

**Acceptance.** `system_id = "alloydbomni"`, `extension = "alloydb_scann"`,
`OPCLASSES = {"scann": {"l2": "l2", "ip": "dot_product", "cosine": "cosine"}}`,
`_search_guc_mapping` mapeando `num_leaves_to_search` → `scann.num_leaves_to_search`. `capabilities()` traz
`vector_scann` e **não** traz nada de plataforma.

### T2.2 — o `LOAD` é provado pela sua ausência

**Why this step.** M5–M7 é a única medição deste item que **outro** mecanismo já verifica: o portão do B-060 lê
`pg_settings`. Um teste que remova o `LOAD` e exija a recusa é o que liga os dois itens — e é a diferença entre
"emiti um LOAD" e "o knob entrou em vigor".

#### TDD

```
RED   test_without_the_load_the_gate_refuses_the_run
        # servidor-duplo que só registra scann.* DEPOIS de ver LOAD — a forma medida em M5/M6
        server = _OmniStub(registers_gucs_only_after_load=True)
        adapter = _adapter_with(server, load_disabled=True)
        with pytest.raises(AdapterError, match="não está em vigor|not in force"):
            adapter.set_search_parameters({"num_leaves_to_search": 500})
```

**Acceptance.** O duplo reproduz M5/M6 (ausente de `pg_settings` antes do `LOAD`, presente depois), e o gate do
B-060 recusa sem que este item escreva verificação nova.

### T2.3 — registro e `doctor`

#### TDD

```
RED   test_alloydbomni_is_registered
        assert "alloydbomni" in ADAPTERS
        assert ADAPTERS["alloydbomni"].requires == ("psycopg",)

RED   test_every_registered_adapter_reports_effective_search_parameters
        # o teste parametrizado do B-060 passa a cobrir 5 adapters em vez de 4 — sem edição
```

---

## Phase 3 — a corrida

### T3.1 — `theodb × alloydbomni × pgvector` no droplet

**Why this step.** G7, e o único critério que nenhum duplo pode satisfazer. Roda em `138.197.22.192`
(droplet efêmero próprio), nunca na máquina do owner.

**Acceptance.** Bundle válido contra os schemas versionados; `system.json` de cada sistema carregando a versão
**lida do servidor**; `points[].parameters` carregando `scann.num_leaves_to_search` efetivo. O relatório declara
que a comparação **cruza uma major** (PG 17.9 vs PG 18) e que `scann.enable_ah_quantizer` estava `off` ou `on`,
explicitamente — sem isso o número não diz o que mediu.

---

## Concurrency tests

`(none — single-threaded)`. O adapter usa uma conexão única e persistente (`postgres.py:132,151`), o runner é
sequencial, e este item não introduz thread, lock ou estado compartilhado. O `scann.num_search_threads = 2` que
M10 mediu é paralelismo **interno do motor**, do outro lado do socket — não do código deste repo.

## Failure scenarios

I/O externo presente (driver de banco), portanto obrigatório.

| # | Falha | Comportamento exigido |
|---|---|---|
| F1 | `alloydb_scann` indisponível (imagem errada) | `CREATE EXTENSION` falha → `AdapterError` com `phase=BOOTSTRAP`, citando a extensão. **Nunca** seguir sem índice e medir seqscan |
| F2 | `LOAD` falha (biblioteca ausente do `$libdir`) | erro propagado com contexto; o portão do B-060 recusaria depois de qualquer forma — mas falhar em `wait_ready` é mais cedo e mais claro |
| F3 | servidor não responde `select version()` | `export_config` **não** cai para a tag da imagem; omite o campo ou levanta. Inferir a versão é o defeito que M1 documentou |
| F4 | `num_leaves_to_search` aceito e não vigente | portão do B-060 recusa antes de medir (T2.2) |
| F5 | reloption que o Omni recusa (M9 mediu a recusa) | `AdapterError` com o texto do servidor, não `ValueError` |
| F6 | conexão cai entre `wait_ready` e a busca | o `LOAD` é perdido; o portão detecta na próxima `set_search_parameters`. **Limite honesto:** não há reconexão automática hoje, e este item não a adiciona |

## Drawbacks & Risks

| # | Risco | Mitigação |
|---|---|---|
| R1 | **A corrida cruza uma major** (PG 17.9 × PG 18) e é comparação desigual | Não esconder: o bundle carrega a versão lida do servidor e o relatório declara a divergência. Alternativa considerada e rejeitada: rodar o TheoDB em PG 17 — mudaria o produto para caber no concorrente |
| R2 | `scann.enable_ah_quantizer = off` por default (M10) — medir assim mede o ScaNN sem o que o torna ScaNN | Fora do escopo **deste** item (que entrega o adapter), mas registrado no relatório e no [[B-057]], que é quem mede |
| R3 | O `vector` do Omni é **fork** do pgvector, não upstream — usá-lo como o "pgvector" da corrida compararia fork com fork | A corrida usa o contêiner pgvector separado para o eixo pgvector; a coincidência de tipos serve só à herança |
| R4 | Herdar `PgvectorAdapter` acopla o Omni a mudanças no adapter do pgvector | É o acoplamento correto enquanto o tipo `vector` for compartilhado (M2). Se divergir, extrair uma base comum — não antecipar (YAGNI) |
| R5 | `OPCLASSES` como atributo de classe é herdado por engano por uma subclasse futura | O teste T1.2 fixa que a leitura é `type(self)`, e cada adapter que muda de AM declara a sua |
| R6 | Sem cobertura de integração real: os testes usam duplos | Mesma ressalva registrada no review do B-060. A T3.1 é a compensação — mas ela é uma corrida, não um teste automatizado |

## Unresolved Questions

- Q1 — **qual `num_leaves` usar no build?** M8 aceitou `10` numa tabela de 200 linhas. O valor certo escala com a
  cardinalidade (como o `lists` do ivfflat, que o arnês já deriva em `:553`). Este item **não** o deriva — entrega
  o parâmetro passável e deixa a derivação para quem medir ([[B-057]]). Pendência consciente, não omissão: derivar
  agora seria fixar uma heurística sem nenhuma medição que a sustente.
- Q2 — **o `ivf` do Omni é um AM distinto** (M4) e não é coberto aqui. Nada mede IVF do Omni hoje; adicioná-lo
  agora é o degrau 1 da parsimony ladder respondido com "não".
- Q3 — **`LOAD` sobrevive a reconexão?** Não há reconexão automática no adapter hoje, então a pergunta não tem
  consequência prática neste item (F6 registra o limite). Se a reconexão vier, o `LOAD` precisa migrar para um
  hook de pós-conexão em vez de viver em `wait_ready`.
