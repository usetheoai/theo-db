---
slug: b046-qps-parity
item: B-046
date: 2026-08-13
upstream: .claude/knowledge-base/discoveries/opportunities/b046-qps-parity-opportunity.md
---

# Plano — decompor o déficit de 16,3% de QPS antes de tentar fechá-lo

## Goal

Responder, **por experimento**, quanto do déficit de QPS do TheoDB contra o pgvector a recall casado vem de
**qualidade de grafo** e quanto vem de **eficiência de varredura** — e publicar a decomposição no artefato do
b035, atualizando-o em vez de duplicá-lo.

O item **não promete fechar o déficit**. Promete decompô-lo e, se a decomposição apontar um caminho barato,
segui-lo dentro deste ciclo; se apontar um caminho caro, abrir o item seguinte com a evidência na mão. Prometer
paridade antes de saber onde está o custo é exatamente o palpite que o `why_now` do B-046 nomeia.

## Baseline Context

### O estado medido

| Fato | Fonte | Valor |
|---|---|---|
| recall @ `ef_search=64` | `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` | TheoDB 0,9600 · pgvector 0,9835 |
| recall casado | idem | TheoDB 0,9829 (`ef=128`) · pgvector 0,9835 (`ef=64`) |
| QPS a recall casado | idem | TheoDB 3.086,1 · pgvector 3.590,6 (**+16,3%**) |
| reprodutibilidade | duas corridas independentes | 1,3% QPS · 0,06% recall |
| caso | `Performance1536D50K` | 50.000 × 1536d, COSINE, k=10 |

### Files that will be touched

| Arquivo | LoC | Papel |
|---|---|---|
| `clients/theodb/config.py` (fork) | ~300 | `TheoDBHNSWConfig._refuse_unhonourable_build_params` (`:191-213`), `index_param()` (`:227-236`) |
| `clients/theodb/theodb.py` (fork) | ~450 | `render_create_index` (pura), `_assert_is_theodb` |
| `tests/test_theodb.py` (fork) | 21 testes | onde a nova cobertura entra |
| `benchmarks/vectordbbench/run-graph-sweep.sh` | novo | a varredura de recall |
| `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` | 190 | **atualizado**, nunca duplicado (DoD) |

Estado do git: `21490aa` em `workspace`. Fork em `4a15939`.

### Current callers / dependents

- `benchmarks/vectordbbench/run.sh` invoca o cliente do fork; `run-graph-sweep.sh` (novo) fará o mesmo
- `benchmarks/vectordbbench/docker-compose.yml` fixa as imagens dos dois motores (já repontado a `theodb:b036`)
- **Nada em `theodb_rs/` chama este código** — o cliente do arnês não é dependência de produto, e é por isso
  que o blast radius do motor neste item é zero
- `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` é o consumidor do resultado

### Architecture boundaries affected

- **Fora deste repositório:** `clients/theodb/{config,theodb}.py` vivem no fork do VectorDBBench. Nenhum gate
  deste repo audita aquele Python (`rules/cycle-code-quality.md` cobre Rust aqui)
- **Dentro:** só `benchmarks/` e `wiki/benchmarks/`. Nenhuma fronteira de `rules/architecture.md` é cruzada —
  o motor não muda neste item
- **`rules/public-copy.md` § 4** governa o que pode ser dito do resultado: comparação de performance exige
  artefato reproduzível linkado no mesmo parágrafo

### Domain glossary

- **qualidade de grafo** — quão bem o grafo HNSW conecta vizinhos verdadeiros; observável como recall a
  `ef_search` fixo. Controlada no build por `m` e `ef_construction`.
- **eficiência de varredura** — quanto trabalho a consulta gasta por unidade de recall obtido; observável como
  páginas tocadas por consulta a recall casado.
- **recall casado** — o par de `ef_search` (um por motor) em que os dois entregam o mesmo recall. É o único
  ponto em que comparar QPS significa alguma coisa (lição do b035, terceira variação).

## Prior Art

- **[[B-035]]** construiu o cliente e o arnês, e mediu o déficit. O guard que este plano modifica é dele.
- **[[B-036]]** tornou `m`/`ef_construction` reloptions honradas — é o que torna o experimento possível.
- **[[B-045]]** deu ao projeto o teste pareado, exigido pelo DoD antes de qualquer afirmação de ganho.
- **M57** (`build.rs:15-21`) mediu `ef_construction` 64→200 **piorando** o recall a 100k–500k, e `m` 16→32
  idem (0,952). **Esta é a razão de o item ser uma varredura e não um ajuste**: a direção do knob não é
  conhecida, é medida.
- **ADR-0030** fixou o critério de recall do projeto como *paridade com o pgvector*, não um absoluto.

## Drawbacks & Risks

| # | Risco | Prob. | Mitigação |
|---|---|---|---|
| R1 | A varredura não acha nenhum ponto que suba o recall a `ef=64` | **média** — o M57 já mediu não-monotonicidade | É resultado, não fracasso: fecha a metade "qualidade de grafo" como *não é aqui* e a decomposição fica mais forte, não mais fraca |
| R2 | Remover o guard reintroduz a medição de parâmetro não aplicado | média | O guard não é removido: passa a **perguntar ao servidor**, e o teste cobre as duas metades (aceita no b036+, recusa no anterior) |
| R3 | O experimento local não reproduz o do droplet | baixa para recall/páginas, **alta para QPS** | Só recall e páginas são medidos localmente; QPS é declarado como pendente de droplet e não entra em tabela nenhuma até lá |
| R4 | 50.000 × 1536d não cabe confortavelmente no host | média — 307 MB de vetores brutos + grafo | Medir a pegada antes; se não couber com folga, cair para `Performance768D1M`? **não** — mudar o caso muda a pergunta. Cair para menos pontos de varredura, nunca para outro caso |
| R5 | A varredura vira uma caça a hiperparâmetro que sempre acha algo | média | O DoD exige teste pareado antes de qualquer afirmação de ganho, e a varredura declara os pontos ANTES de rodar |

## Unresolved Questions

- Q1 — O `ef_construction` alto degrada a 50K×1536d como degradou a 100k–500k no M57? Não sei, e é
  literalmente o que a varredura mede. Registrado como pergunta, não como suposição.
- Q2 — Páginas tocadas é bom proxy de "custo por candidato" do pgvector? É o melhor disponível sem forkar o
  pgvector, e o artefato dirá isso com essas palavras.
- Q3 — Se a causa for única, [[B-042]] fecha como duplicata deste ou o contrário? Decisão do owner quando a
  evidência existir; o plano não a antecipa.

## ADRs

### D1 — O guard pergunta ao servidor, e não é removido

**Decisão.** `_refuse_unhonourable_build_params` deixa de comparar contra a constante `THEODB_HNSW_M` e passa
a decidir por uma **sonda contra o servidor**: numa transação revertida, cria uma tabela temporária mínima e um
índice com as opções pedidas. Se o PostgreSQL aceitar, o parâmetro é honrado; se recusar com
`unrecognized parameter`, o guard levanta como hoje, citando a versão do servidor.

**Alternativas consideradas.**

- *Remover o guard e deixar o `CREATE INDEX` falhar.* Rejeitada: a falha viria **depois da carga do dataset**,
  que é a parte cara. A regra do arnês é falhar antes da carga, e o B-035 pagou para aprendê-la.
- *Ler a versão da extensão e comparar com "≥ b036".* Rejeitada: acopla o cliente a um número de versão que
  ninguém garante, e mede o rótulo em vez da capacidade — a mesma classe do b047 (rótulo igual, máquina
  diferente). A sonda mede a capacidade.
- *Manter a constante e adicionar um `--force`.* Rejeitada: uma flag que desliga a verificação é a forma mais
  rápida de publicar um parâmetro não aplicado.

**Custo aceito.** Uma transação a mais por corrida, antes da carga. É desprezível ao lado de carregar 50.000
vetores de 1536d.

### D2 — A decomposição roda no host; o droplet só produz o número final

**Decisão.** Recall e páginas-por-consulta são medidos localmente. QPS **não** é medido localmente e nenhum
número de QPS local entra em artefato.

**Razão.** Recall é função do grafo e da consulta; páginas tocadas é função do layout e do caminho. Nenhum dos
dois depende do relógio. QPS depende. Medir a decomposição na nuvem seria pagar por um número que não muda com
a máquina — e medir QPS aqui seria publicar a máquina errada, que é a classe de erro do b040/b044.

**Alternativa considerada.** *Tudo no droplet, por uniformidade.* Rejeitada por custo sem ganho de validade.

### D3 — Os pontos da varredura são declarados antes de rodar

**Decisão.** A grade é fixada no plano: `m ∈ {16, 24, 32}` × `ef_construction ∈ {64, 128, 200, 400}`, com
`ef_search=64` fixo. Doze pontos, e o mesmo corpus/consultas do b035.

**Razão.** Declarar depois é como uma varredura vira caça ao ponto conveniente. Doze pontos cobrem as duas
direções que o M57 explorou (subir `m`, subir `efc`) mais o cruzamento que ele não explorou.

**Alternativas consideradas.**

- *Busca adaptativa (subir `efc` enquanto o recall subir).* Rejeitada: o M57 mediu que o recall **não** é
  monotônico em `efc`, então uma busca gulosa pararia no primeiro platô e chamaria isso de ótimo.
- *Um ponto só, `efc=200`, que é o que o pgvector recomenda.* Rejeitada: mediria uma hipótese, não decomporia
  nada — e o M57 já mediu esse ponto exato piorando o recall.
- *Grade declarada depois de ver os primeiros resultados.* Rejeitada explicitamente: é o mecanismo pelo qual
  uma varredura vira caça ao ponto conveniente, e o resultado seria irreprodutível por construção.

**Custo aceito.** Doze builds de índice sobre 50.000 × 1536d. Se a medição de pegada (T1.0) mostrar que não
cabe, o plano corta pontos da grade — **nunca troca de caso**, porque trocar o caso troca a pergunta.

## Coverage Matrix

| # | Afirmação do Goal | Tarefa que a cobre |
|---|---|---|
| C1 | O experimento é possível — o arnês aceita `m`/`ef_construction` | T1.1, T1.2 |
| C2 | O guard continua protegendo contra servidor que não suporta | T1.1 |
| C3 | A metade "qualidade de grafo" recebe um número | T1.3, T1.4 |
| C4 | A metade "eficiência de varredura" recebe um número | T1.5 |
| C5 | O artefato do b035 é atualizado, não duplicado | T1.6 |

## Tasks

### T1.0 — Medir a pegada antes de declarar a grade viável

#### Why this step

R4 diz que 50.000 × 1536d pode não caber com folga em 15 GB de RAM e 129 GB livres. Declarar doze builds e
descobrir isso no sétimo desperdiça horas. A medição é um build só.

#### TDD

Não é código: é medição. Carregar o caso, construir **um** índice com o par default, e registrar tempo de
build, tamanho do índice e pico de memória. Se um build custar mais de ~20 min, a grade cai para 6 pontos
(`m ∈ {16, 32}` × `efc ∈ {64, 200, 400}`) e a redução é **registrada com o número que a motivou**.

#### Concurrency tests

(none — single-threaded) A medição de pegada é uma corrida sequencial; nada aqui é concorrente.

#### Acceptance criteria

- tamanho do índice e tempo de build de um ponto `equals` registrados no log da corrida
- a grade final está escrita no plano **antes** do primeiro ponto rodar: `grep -c '^| m=' run-graph-sweep.sh` `equals` 12 ou 6, e o commit que a declara é anterior ao commit dos resultados (`git log --reverse`)

> **T1.0 — EXECUTADA em 2026-08-13. A grade de 12 pontos fica.**
>
> | Grandeza | Medido |
> |---|---|
> | inserção de 50.000 × 1536d | **17,8 s** |
> | construção do índice (`optimize`) | **200,7 s** |
> | carga total | 218,5 s |
> | heap | 391 MB |
> | índice HNSW | **401 MB** |
> | disco livre no host | 128 GB |
>
> Um ponto custa ~3,7 min de carga; doze pontos cabem em ~1 h. O corte para 6 pontos que a
> T1.0 autorizava **não é necessário** e não foi feito.
>
> **Um achado que mudou o runner, e que só apareceu por rodar:** o estágio
> `--search-concurrent` varre 8 níveis de concorrência (1, 5, 10, 20, 30, 40, 60, 80) × 30 s
> ≈ **4 min por ponto**, e produz QPS — que a D2 proíbe publicar a partir do host. Pior: o
> JSON de resultado só é escrito no fim da tarefa **inteira**, então interromper esse estágio
> perdeu o recall que a corrida já havia medido. Quatro minutos por ponto para produzir um
> número inutilizável, com risco de perder o utilizável. O runner passou a usar
> `--search-serial` sozinho.

### T1.1 — O guard passa a perguntar ao servidor

#### Why this step

É o bloqueio medido no Corner 1. E as duas metades importam: aceitar contra b036+ **e** recusar contra o
anterior. Testar só a primeira reintroduz o defeito que o guard existe para impedir.

#### TDD

RED (no fork, `tests/test_theodb.py`):

```python
def test_build_params_accepted_when_the_server_supports_them(monkeypatch):
    # sonda responde "aceita" -> m=32 passa
    ...
    cfg = TheoDBHNSWConfig(metric_type=MetricType.COSINE, m=32, ef_construction=200)
    assert cfg.index_param()["options"] == {"m": 32, "ef_construction": 200}

def test_build_params_still_refused_when_the_server_rejects_them(monkeypatch):
    # sonda responde 'unrecognized parameter "m"' -> levanta, citando a versão do servidor
    with pytest.raises(UnsupportedBuildParameterError) as e:
        TheoDBHNSWConfig(metric_type=MetricType.COSINE, m=32)
    assert "unrecognized parameter" in str(e.value)

def test_the_probe_runs_before_the_load_not_after():
    # o guard é consultado na construção da config, não no create_index
    ...
```

GREEN — sonda em transação revertida; o resultado é memoizado por conexão (uma sonda por corrida, não por
consulta).

#### Concurrency tests

(none — single-threaded) O cliente do arnês configura em thread única, e a sonda roda antes de qualquer
worker existir. A fase de consulta, que **é** paralela no arnês, não toca este código — o resultado da sonda
já está memoizado quando os workers sobem.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Servidor inalcançável na hora da sonda | levanta com a mensagem de conexão, **não** assume "suporta" |
| Servidor aceita `m` mas não `ef_construction` | recusa nomeando **qual** dos dois, não os dois |
| Sonda deixa lixo no banco | proibido — transação revertida, verificado por teste que conta relações antes/depois |

#### Acceptance criteria

- `TheoDBHNSWConfig(m=32, ef_construction=200)` contra b036+ produz `options == {"m": 32, "ef_construction": 200}`
- contra um servidor sem as reloptions, **levanta** `UnsupportedBuildParameterError` citando a mensagem real do
  PostgreSQL, não uma constante do cliente
- `pytest tests/test_theodb.py -q` reporta `>= 24` testes e `0 failed` (hoje: `21 passed`)

> **Correção por acréscimo à T1.1, feita na implementação (2026-08-13).** O plano dizia que a
> recusa continuaria acontecendo na **construção da config**. Não pode: `TheoDBHNSWConfig` é
> construída pela CLI **antes de existir qualquer conexão**, e uma sonda sem conexão não é uma
> sonda. A recusa mudou de lugar — sai da config e passa para `TheoDB.__init__`, logo após
> `_assert_is_theodb()`, que é onde a conexão já existe **e ainda é antes da carga**. A
> propriedade que o plano queria preservar (falhar antes do caro) está preservada; o que estava
> errado era o endereço.

### T1.2 — O `WITH` chega ao `CREATE INDEX`, verificado no catálogo

#### Why this step

Emitir a opção e o servidor ignorá-la é o defeito do B-034 numa camada nova. A verificação é no
`pg_class.reloptions`, não no SQL gerado.

#### TDD

RED — teste de integração contra o contêiner `theodb:b036`:

```python
def test_create_index_carries_the_options_into_the_catalog(theodb_container):
    client.create_index(m=32, ef_construction=200)
    assert reloptions_of("theodb_collection_idx") == ["m=32", "ef_construction=200"]
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `pg_class.reloptions` do índice criado pelo cliente `equals` `{m=32,ef_construction=200}`
- o teste falha se `index_param()` voltar a devolver `options: {}`

### T1.3 — A varredura de qualidade de grafo

#### Why this step

É o experimento que decide onde está o custo. `ef_search` fixo em 64 isola a variável: qualquer mudança de
recall vem do grafo.

#### TDD

Não é unidade — é corrida. `benchmarks/vectordbbench/run-graph-sweep.sh` roda a grade da D3 e emite uma tabela
recall × (`m`, `efc`). O gate do runner é o do `run.sh`: sucesso lido no **JSON de resultado**, nunca no código
de saída (o `vectordbbench` sai 0 mesmo quando o caso falha).

#### Concurrency tests

(none — single-threaded) A varredura roda um ponto da grade por vez, em série, de propósito: dois builds de
índice concorrentes na mesma máquina disputariam `maintenance_work_mem` e CPU, e o tempo de build medido em
T1.0 deixaria de significar o que diz.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Um ponto da grade falha o build | registra o ponto como falho **e continua**; a tabela mostra o buraco em vez de omitir a linha |
| Todos os pontos dão o mesmo recall | é sinal de que o `WITH` não chegou — o runner **falha alto** em vez de publicar uma tabela plana |

#### Acceptance criteria

- tabela com **um recall medido por ponto** da grade, a `ef_search=64`
- o melhor ponto da grade é comparado com `0.9835` (pgvector @ `ef_search=64`); a diferença `melhor_recall - 0.9600` **equals** o número publicado para a metade "qualidade de grafo"
- se `max(recall_da_grade) < 0.9606` (= `0.9600` + os `0.06%` de reprodutibilidade medidos no b035), o artefato **contains** a frase `não é qualidade de grafo` — resultado registrado, não fracasso

### T1.4 — O ponto vencedor é confirmado, não assumido

#### Why this step

Uma grade de doze pontos tem doze chances de um deles parecer melhor por ruído. A reprodutibilidade medida no
b035 é 0,06% em recall; qualquer diferença dessa ordem não é diferença.

#### TDD

Re-rodar o melhor ponto e o default, na mesma sessão, e comparar. Se a diferença entre as duas execuções do
mesmo ponto for da ordem da diferença entre os pontos, o resultado é **inconclusivo** e é assim que é
registrado.

#### Concurrency tests

(none — single-threaded) Repetição sequencial, pelo mesmo motivo da T1.3.

#### Acceptance criteria

- o melhor ponto é re-executado `>=` 2 vezes
- a diferença entre pontos é `>` 3× a diferença entre execuções do mesmo ponto, ou o resultado é declarado
  inconclusivo

### T1.5 — A metade "eficiência de varredura"

#### Why this step

Se a T1.3 não fechar o recall, o custo está na varredura, e essa metade precisa de um número próprio.

#### TDD

Corrida: nos dois motores, no **ponto de recall casado**, medir `EXPLAIN (ANALYZE, BUFFERS)` sobre as mesmas N
consultas e reportar a mediana de `shared hit`. Do lado do TheoDB, reportar também `candidates_seen` via
`theodb.explain_scan`.

#### Concurrency tests

(none — single-threaded) `EXPLAIN (ANALYZE, BUFFERS)` é medido com **um** cliente por vez em cada motor. Medir
sob concorrência mudaria o cache compartilhado durante a própria medição, e a contagem de páginas deixaria de
ser propriedade do caminho para virar propriedade da carga.

#### Failure scenarios

| Cenário | Comportamento exigido |
|---|---|
| Cache quente num lado e frio no outro | aquecer os dois com o mesmo número de consultas antes de medir, e dizer que se fez isso |
| `shared read` > 0 (leu do disco) | reportar separado de `shared hit`; misturar os dois compara I/O com cache |

#### Acceptance criteria

- mediana de `shared hit` por consulta, os dois motores, a recall casado, `>=` 100 consultas
- `candidates_seen` do TheoDB reportado ao lado, **declarado como sem equivalente no pgvector**
- o artefato **contains** a razão `shared_hit / candidates_seen` do TheoDB como número, e **contains** a frase `não medível sem forkar o pgvector` para o denominador do outro lado

### T1.6 — O artefato é atualizado, não duplicado

#### Why this step

Exigência literal do DoD, e a razão é a do acervo: um segundo arquivo para a mesma medição faz o leitor
encontrar o antigo.

#### TDD

`grep` estrutural: existe exatamente **um** arquivo em `wiki/benchmarks/` sobre a comparação TheoDB×pgvector no
`Performance1536D50K`.

#### Concurrency tests

(none — single-threaded) Edição de arquivo.

#### Acceptance criteria

- `ls wiki/benchmarks/ | grep -c "theodb-vs-pgvector"` `equals` 1
- o arquivo ganha uma seção com a decomposição e **contains** ainda os três números originais (`3.086,1`, `3.590,6`, `0,9829`) — verificado por `grep`, não por leitura
- o conceito OKF correspondente é atualizado e `okf-validate` sai 0

## Failure scenarios

Consolidados por tarefa acima. O caminho tem I/O externo (dois contêineres PostgreSQL + download de dataset),
e as duas classes que já custaram corrida neste projeto estão cobertas: **sucesso lido no JSON e não no exit
code** (T1.3) e **cache assimétrico** (T1.5).

## Definition of done

- [ ] o cliente aceita `m`/`ef_construction` contra b036+ **e** recusa contra servidor sem suporte — as duas
      metades testadas
- [ ] `pg_class.reloptions` prova que o `WITH` chegou
- [ ] tabela recall × (`m`, `efc`) a `ef_search=64` no caso real, com o melhor ponto confirmado por repetição
- [ ] um número para cada metade da decomposição, cada um com o instrumento que o produziu
- [ ] nenhuma afirmação de ganho sem teste pareado
- [ ] `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` atualizado, arquivo único
- [ ] QPS **não** medido no host, e a pendência do droplet declarada no artefato
