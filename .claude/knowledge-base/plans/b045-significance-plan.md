---
slug: b045-significance
items: [B-045]
date: 2026-08-13
branch: workspace
---

# Significância pareada sobre as abstrações do arnês, sem forkar o núcleo

## Goal

Devolver ao projeto a capacidade de dizer se uma diferença medida **sobrevive ao acaso**: recuperar o teste
de permutação pareada do histórico, escrever o avaliador por consulta que nunca existiu para o VectorDBBench,
e aplicar os dois aos três artefatos já publicados — de modo que (a) o dado por consulta seja obtido
**reusando** dataset, métrica e porta de cliente do arnês, sem alterar o núcleo dele, (b) a média por consulta
seja **verificada contra o agregado publicado**, e (c) todo resultado que não sobreviva ao teste seja
publicado como não-significativo, por acréscimo.

## Baseline Context

### Files that will be touched

| Arquivo | Estado | LoC | Papel |
|---|---|---|---|
| `benchmarks/significance/significance.py` | **recuperado** de `7cd157d^` | 93 | permutação pareada + bootstrap + t. Pura computação |
| `benchmarks/significance/test_significance.py` | **recuperado**, parcial | ~60 | os 6 testes que exercitam `paired_significance` direto |
| `benchmarks/significance/per_query.py` | **novo** | ~120 | `PerQueryEvaluator` — dirige o laço de consultas sobre a porta `VectorDB` |
| `benchmarks/significance/compare.py` | **novo** | ~130 | alinha por `qid`, roda as comparações par a par, persiste os arrays |
| `benchmarks/significance/test_per_query.py` | **novo** | ~90 | testes do avaliador com um cliente falso |
| `wiki/benchmarks/b047-lexical-headtohead.md` | atualização | — | ganha `p`, IC e vitórias/derrotas |
| `wiki/benchmarks/b040-theodb-fts-msmarco.md` | atualização | — | idem, para o A/B do stemming |
| `CHANGELOG.md` | edição | — | `[Unreleased]` |

**Zero alteração no fork.** É a decisão central e está na D1.

### Current callers / dependents

As três peças do arnês das quais o avaliador depende — e **só** essas:

| Símbolo | Onde | Contrato |
|---|---|---|
| `VectorDB.search_documents(query, k)` | `clients/api.py:302` | devolve IDs ranqueados; TheoDB, Elastic e OpenSearch implementam |
| `metric.calc_ndcg_fts(k, gt, got)` | `metric.py:149` | NDCG **por consulta** — a mesma função que a corrida usa |
| `metric.calc_recall_fts`, `calc_mrr_fts` | `metric.py:131`, idem | recall e MRR por consulta |
| `FtsDatasetManager.recall_queries_data` / `.recall_gt_data` | `dataset.py` | consultas e qrels válidos, o mesmo conjunto da corrida |

O dado por consulta que o arnês descarta está em `runner/serial_runner.py:238-240`; **não** o recuperamos de
lá — ver D1.

### Domain glossary

| Termo | Significado aqui |
|---|---|
| **teste de permutação pareada** | sob H0 cada diferença por consulta é igualmente provável `+d` ou `−d`; `p` = fração de reamostragens com média ≥ a observada |
| **bootstrap pareado** | reamostra as diferenças com reposição; percentis 2,5 e 97,5 dão o IC 95% |
| **`d_z` de Cohen** | tamanho de efeito pareado: `mean(d) / sd(d)` |
| **qrel** | julgamento de relevância por consulta; é o que permite NDCG e MRR |
| **alinhamento por `qid`** | parear a consulta certa entre sistemas. Sem isso o teste compara consultas diferentes e o `p` não significa nada |
| **`(count+1)/(B+1)`** | correção de Monte Carlo — a atribuição observada é uma das permutações, então `p` nunca é 0 |

### Architecture boundaries affected

```
compare.py  ──alinha por qid──►  significance.py   (numpy puro; sem DB, sem rede)
     │
     └──►  PerQueryEvaluator  ──►  VectorDB.search_documents   (porta do arnês)
                              ──►  metric.calc_*_fts           (métrica do arnês)
                              ──►  FtsDatasetManager           (dataset do arnês)
```

A direção de dependência é toda para **abstrações do arnês**, nunca para o núcleo dele. `significance.py` não
conhece o arnês; `PerQueryEvaluator` não conhece nenhum motor; `compare.py` não conhece nenhum dos dois por
dentro. Um motor novo entra implementando a porta — nada aqui muda (OCP).

## Prior Art

| Fonte | O que ensina | Onde |
|---|---|---|
| Oportunidade do B-045 | as medições que sustentam este plano | `.claude/knowledge-base/discoveries/opportunities/b045-significance-opportunity.md` |
| `significance.py` removido | o teste, a justificativa da escolha e a correção de Monte Carlo | `git show 7cd157d^:benchmarks/theodb_bench/significance.py` |
| `_paired_sig` do consumidor antigo | duas exigências que carrego sem carregar o código: **alinhar por qid** e **persistir os arrays** para recomputo por terceiro | `git show 7cd157d^:benchmarks/theodb_bench/test_significance.py:61,99` |
| Smucker, Allan & Carterette (CIKM 2007) | por que permutação pareada, e por que não Wilcoxon nem sinal | citado no docstring recuperado |
| ADR-0061 | o limite declarado que este item fecha | `wiki/decisions/0061-benchmark-oficial-por-pilar.md` |
| `b047-lexical-headtohead` | a alegação mais frágil que temos: **paridade** sem teste | `wiki/benchmarks/b047-lexical-headtohead.md` |

## Coverage Matrix

| # | Afirmação do Goal | Tarefa(s) |
|---|---|---|
| G1 | O teste estatístico volta e passa a própria suíte | T1.1 |
| G2 | O dado por consulta é obtido reusando as abstrações, sem tocar o núcleo do fork | T1.2 |
| G3 | A média por consulta bate com o agregado publicado | T1.3 |
| G4 | Os artefatos publicados ganham `p`, IC e vitórias/derrotas — inclusive quando o resultado não sobrevive | T1.4 |

Cobertura: **4/4**.

## ADRs

### D1 — O dado por consulta é reproduzido pela porta, não extraído do núcleo do arnês

**Decisão.** `PerQueryEvaluator` dirige o próprio laço de consultas sobre `VectorDB.search_documents`, e
computa a métrica com as funções do arnês.

**Alternativas consideradas.**

1. *Patchear `serial_runner.py` para persistir os arrays.* O dado já existe lá (linhas 238-240). **Rejeitada:**
   o método devolve `(avg_recall, avg_ndcg, avg_mrr, p99, p95)`, então persistir exigiria mudar a tupla, todos
   os chamadores e o dataclass `Metric` — uma alteração que ripplaria por `runner/` e `task_runner.py`, os
   arquivos que a Política de Fork (D3) manda não tocar. Um fork que altera o núcleo deixa de ser rebaseável,
   e a saída prevista pela D3 ("morre quando o upstream aceitar") vira ficção.
2. *Reimplementar a métrica do zero.* **Rejeitada:** dois cálculos de NDCG divergem em detalhes (corte em `k`,
   tratamento de empate, DCG ideal) e o `p` sairia sobre números que não são os da tabela publicada. Importar
   `calc_ndcg_fts` torna a métrica idêntica **por construção**, não por conferência.

**Consequência aceita:** um passe extra de consultas por sistema. Medido em escala: 6.980 consultas a ~1.800
QPS ≈ 4 s no motor mais lento dos três — desprezível ao lado da corrida completa de ~15 min.

**Consequência que é vantagem:** dirigir o laço garante **as mesmas consultas na mesma ordem** para todos os
sistemas, que é o pré-requisito do teste pareado. Persistir no arnês não daria isso de graça.

### D2 — Alinhar por `qid`, nunca por posição

**Decisão.** `compare.py` alinha os arrays pelo identificador de consulta antes de parear.

**Alternativas consideradas.**

1. *Assumir a mesma ordem.* Mais simples, e **verdadeiro hoje** porque o mesmo avaliador dirige todos os
   sistemas. **Rejeitada:** a suposição é invisível e frágil — uma execução em paralelo, um sistema que pule
   uma consulta por erro, ou um artefato lido de outra corrida quebram o pareamento **sem erro**, e o `p` sai
   sobre consultas diferentes. É a classe de defeito que este ciclo inteiro existe para combater.
2. *Ordenar antes de comparar.* Equivalente a alinhar, mas perde a detecção: um sistema com consulta faltando
   passaria despercebido. Alinhar por chave **falha alto** nesse caso.

### D3 — Persistir os arrays por consulta no artefato

**Decisão.** O relatório carrega `qids` e os valores por consulta de cada sistema.

**Alternativas consideradas.**

1. *Publicar só `p` e IC.* **Rejeitada:** um `p` sem o dado é uma afirmação que ninguém pode verificar. Com
   os arrays, um terceiro recomputa o teste — inclusive com outro teste estatístico, se discordar da escolha.
2. *Publicar o dado bruto de todas as métricas.* YAGNI. NDCG é a métrica de qualidade que as alegações usam;
   recall e MRR entram porque saem do mesmo passe, sem custo.

### D4 — Não recuperar o consumidor `run_m53_hybrid_beir`

**Decisão.** Recuperamos `significance.py` e os 6 testes que o exercitam direto. Os outros 4 testes do
arquivo dependem de `run_m53_hybrid_beir._paired_sig` e **não** são recuperados.

**Alternativas consideradas.**

1. *Recuperar o consumidor inteiro.* **Rejeitada:** ele é engessado em três sistemas nomeados
   (`hybrid`/`vector`/`fts`) e num artefato BEIR que não existe mais. `compare.py` precisa ser genérico sobre
   N sistemas, porque o b047 compara três motores e o próximo pode comparar cinco.
2. *Recuperar os 4 testes adaptando-os.* **Rejeitada:** testes adaptados de um consumidor que não existe
   testam o que eu escrevi hoje, não o que foi validado antes. Escrevo testes novos para `compare.py`.

**O que carrego do módulo descartado**, porque são exigências e não código: alinhar por `qid` (D2) e
persistir os arrays (D3).

## Tasks

### T1.1 — O teste estatístico volta e passa a própria suíte

#### Why this step

É a peça que já foi validada e não precisa ser reinventada — degrau 2 da parsimony ladder. Recuperá-la com
os testes originais prova que a recuperação foi fiel, não uma reescrita disfarçada.

#### TDD

RED — recuperar os arquivos e rodar a suíte antes de qualquer adaptação:

```bash
git show 7cd157d^:benchmarks/theodb_bench/significance.py    > benchmarks/significance/significance.py
git show 7cd157d^:benchmarks/theodb_bench/test_significance.py > /tmp/orig_tests.py
pytest benchmarks/significance/test_significance.py -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- `pytest benchmarks/significance/test_significance.py` sai com `exit code 0` e reporta **6 passed**
- `git diff --no-index` entre o arquivo recuperado e `git show 7cd157d^:...` é **vazio** — a função não foi
  reescrita, `equals` byte a byte
- os 4 testes que dependem de `run_m53_hybrid_beir` estão **removidos com comentário** dizendo por quê —
  `grep -c "run_m53_hybrid_beir" benchmarks/significance/test_significance.py` `equals` 0

### T1.2 — O avaliador por consulta, sobre a porta

#### Why this step

É a peça que nunca existiu para o VectorDBBench, e é o que torna o teste aplicável. Sem ela, `significance.py`
é uma função sem dado.

#### TDD

RED — com um cliente falso que implementa a porta, sem banco:

```python
class FakeClient:
    """Implementa só o que o avaliador usa: a porta VectorDB.search_documents."""
    def __init__(self, answers): self.answers = answers
    def search_documents(self, query, k=10, **kw): return self.answers[query][:k]

def test_evaluator_returns_one_score_per_query():
    ev = PerQueryEvaluator(k=10)
    r = ev.evaluate(FakeClient({"q1": ["d1","d2"], "q2": ["d3"]}),
                    queries=[("q1","texto 1"), ("q2","texto 2")],
                    qrels=[{"d1":1}, {"d9":1}])
    assert r.qids == ["q1","q2"]
    assert len(r.ndcg) == 2
    assert r.ndcg[0] > 0 and r.ndcg[1] == 0     # q2 não achou nada relevante

def test_evaluator_refuses_mismatched_lengths():
    with pytest.raises(ValueError, match="queries e qrels"):
        PerQueryEvaluator(k=10).evaluate(FakeClient({}), queries=[("q1","t")], qrels=[])

def test_evaluator_uses_the_harness_metric_function():
    # o valor tem de bater com calc_ndcg_fts chamada diretamente — métrica idêntica por construção
    from vectordb_bench.metric import calc_ndcg_fts
    assert r.ndcg[0] == calc_ndcg_fts(10, {"d1":1}, ["d1","d2"])
```

#### Concurrency tests

(none — single-threaded). O avaliador é sequencial de propósito: a ordem das consultas é o pareamento, e
paralelizar aqui trocaria uma garantia por um ganho de segundos.

#### Acceptance criteria

- `pytest benchmarks/significance/test_per_query.py` sai 0
- o array devolvido `equals` o resultado de `calc_ndcg_fts` chamada diretamente — asserção explícita, porque
  é o que garante que o `p` é sobre os números da tabela
- comprimentos divergentes entre consultas e qrels levantam `ValueError` com mensagem `contains` "queries e
  qrels"
- o avaliador não importa nenhum motor: `grep -cE "theodb|elastic|opensearch" per_query.py` `equals` 0

### T1.3 — A média por consulta bate com o agregado publicado

#### Why this step

É a verificação que torna todo o resto confiável. Se a média dos nossos valores não reproduz o número que o
arnês publicou, estamos calculando outra coisa — e um `p` sobre outra coisa é pior que nenhum `p`.

#### TDD

RED — contra os motores reais, comparando com o JSON já coletado:

```python
@needs_engines
def test_per_query_mean_reproduces_the_published_aggregate():
    pub = load_published_metrics("results-lexical/json", db="TheoDB", label="theodb-b044")
    got = PerQueryEvaluator(k=10).evaluate(theodb_client, queries, qrels)
    assert abs(mean(got.ndcg) - pub["ndcg"]) <= 5e-4    # o arnês arredonda em 4 casas
    assert abs(mean(got.recall) - pub["recall"]) <= 5e-4
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- a diferença entre a média por consulta e o agregado publicado é `<= 5e-4` para NDCG e recall — o
  arredondamento de 4 casas que o `serial_runner` aplica
- a verificação roda para **os três** motores: o teste é parametrizado sobre `('TheoDB','ElasticCloud','OSSOpenSearch')` e `pytest -k reproduces` reporta `3 passed`, não 1
- se algum divergir, o `pytest` sai com `exit code` diferente de 0 e nenhum artefato é escrito; a tolerância `5e-4` é constante no código — `grep -c '5e-4' test_per_query.py` `equals` 1, provando que não há valor ajustado por motor

### T1.4 — Os artefatos publicados ganham p, IC e contagem

#### Why this step

É o produto. E o caso mais importante é o que pode dar errado: a alegação de **paridade** do b047 é a mais
frágil que temos, porque um empate observado é indistinguível de uma diferença que a amostra não detectou.

#### TDD

RED — verificação sobre os artefatos:

```python
def test_artifacts_carry_significance():
    for f in ("b047-lexical-headtohead.md", "b040-theodb-fts-msmarco.md"):
        md = read(f"wiki/benchmarks/{f}")
        assert "p =" in md or "p_permutation" in md
        assert "IC 95%" in md
        assert "vitórias" in md
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance criteria

- os artefatos `contains` `p`, `IC 95%` e vitórias/derrotas/empates para cada comparação afirmada
- o artefato do b047 diz explicitamente se a **paridade** é sustentada: um `p` alto com IC estreito em torno
  de zero é evidência de equivalência; um `p` alto com IC largo é **falta de poder**, e são coisas diferentes
- os arrays por consulta são persistidos em `benchmarks/significance/per-query/` para recomputo por terceiro
- se algum resultado publicado **não** sobreviver, a correção entra por acréscimo — `grep -c "não-significativo"`
  `>= 1` quando for o caso

## Failure scenarios

O avaliador faz I/O (consulta os motores por rede) e lê artefatos do disco.

| Cenário | Comportamento exigido | Onde |
|---|---|---|
| Motor cai no meio do passe | o erro propaga com a consulta e o índice; **não** se completa o array com zeros | T1.2 |
| Consulta sem qrel | é dado, não erro: NDCG 0 legítimo, contado como consulta válida | T1.2 |
| `qid` presente num sistema e ausente noutro | `compare.py` **falha alto** nomeando o `qid` — pareamento quebrado é resultado inválido | T1.3 |
| Média não reproduz o agregado | a corrida falha e nada é publicado | T1.3 |
| Artefato publicado sem o `p` correspondente | o teste de T1.4 falha | T1.4 |
| `scipy` ausente | o t-test cai na aproximação normal e **registra o método** — comportamento já do código recuperado | T1.1 |

## Concurrency tests

O avaliador é **deliberadamente sequencial** — a ordem das consultas é o pareamento. A única concorrência
relevante é a do motor sob consulta, e ela não é objeto deste item.

| Verificação | Como |
|---|---|
| O avaliador não paraleliza | asserção de que não há `ThreadPool`/`multiprocessing` no módulo — `grep -c` `equals` 0 |
| O teste estatístico é determinístico | duas execuções com a mesma semente dão `p` e IC idênticos — `assert_equal` |

## Dependencies

Nenhuma dependência nova obrigatória.

| Pacote | Papel | Licença | Nota |
|---|---|---|---|
| `numpy` | permutação e bootstrap | BSD-3 | já é dependência do arnês |
| `scipy` | p-valor exato do t | BSD-3 | **opcional** — o código recuperado cai na aproximação normal e registra qual usou |
| `vectordb-bench` | dataset, métrica, porta de cliente | MIT | já instalado; é do que dependemos |

## Drawbacks & Risks

| # | Risco | Probabilidade | Mitigação |
|---|---|---|---|
| R1 | **A paridade do b047 pode não se sustentar** — o teste pode revelar que o Elastic é significativamente melhor | média | É o resultado, e vai publicado. Foi por isso que o item foi aberto |
| R2 | **O +5,6% do stemming pode não sobreviver** | baixa | Publicado como não-significativo, por acréscimo, se for o caso |
| R3 | Um passe extra de consultas por sistema | certa | ~4 s por motor; desprezível |
| R4 | A média por consulta pode não reproduzir o agregado por detalhe de arredondamento | média | T1.3 é exatamente essa verificação, com tolerância declarada e não ajustável |
| R5 | O `p` do QPS não é computável desta forma — QPS não tem valor por consulta | certa | **Declarado**: este item dá significância às métricas de **qualidade**. Para QPS, o caminho é N corridas repetidas, e isso é item separado |
| R6 | Escolha de teste pode ser contestada | baixa | Os arrays são persistidos (D3): quem discordar recomputa com outro teste |

## Unresolved Questions

- Q1 — **A paridade do b047 sobrevive?** Não sei, e é o ponto. Um `p` alto com IC estreito em torno de zero
  sustenta equivalência; um `p` alto com IC largo é falta de poder. Resolver por execução em T1.4, e dizer
  qual dos dois é.
- Q2 — **Quantas consultas o MS MARCO Small dá de fato?** O log diz 6.980 com qrels. Confirmar em T1.3, porque
  `n` entra no poder do teste.
- Q3 — **Vale dar significância ao QPS por N corridas repetidas?** Fora do escopo aqui (R5). Se a resposta
  do b047 depender disso, vira item próprio.
