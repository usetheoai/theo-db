---
item: B-045
mode: evolve
date: 2026-08-13
verdict: pending
---

# B-045 — A ferramenta existe no histórico; o que falta é o dado por consulta, e ele não precisa de fork

## Corner 1 — Evidence

### O que já publicamos sem significância

Três artefatos, três alegações comparativas, zero testes:

| Artefato | Alegação | Base |
|---|---|---|
| `b035-theodb-vs-pgvector-pg18` | pgvector **+16,3% de QPS** a recall casado | 2 corridas concordantes (1,3%) |
| `b040`/`b044` | stemming **+5,6% de NDCG** | 1 corrida de cada lado |
| `b047-lexical-headtohead` | **paridade** com Elastic (+0,11% de NDCG) | 1 corrida por configuração |

O terceiro é o mais exposto: **afirmar paridade sem teste é a alegação mais frágil das três**, porque um
empate observado é indistinguível de uma diferença que a amostra não teve poder de detectar.

### A ferramenta removida — recuperada e lida

`git show 7cd157d^:benchmarks/theodb_bench/significance.py` — **93 linhas**, mais 112 de teste
(`test_significance.py`) e um consumidor (`m177_uds_significance.py`, 41 linhas).

O desenho é bom e a escolha estatística está justificada no próprio docstring:

- **Teste de permutação pareada** (Smucker, Allan & Carterette, CIKM 2007) como p principal — o teste que a
  literatura de IR recomenda. Wilcoxon e sinal foram **rejeitados com razão escrita**: descartam a magnitude
  por consulta e derrubam empates.
- **IC 95% por bootstrap pareado** (percentil) sobre a diferença média.
- **t pareado** como verificação cruzada concordante (Urbano, SIGIR 2013).
- numpy puro, sem DB e sem rede — testável offline. Semente fixa: `p` e IC reproduzem exatamente.
- Correção de Monte Carlo `(count+1)/(B+1)` — `p` nunca sai 0.

**Não é preciso escrever um teste estatístico.** É preciso recuperá-lo (degrau 2 da parsimony ladder — o que
já existe resolve).

### O gate real: o arnês não persiste dado por consulta

Medido no JSON de resultado das corridas do b047 — as chaves de métrica são **agregados**: `ndcg`, `recall`,
`mrr`, `qps`. As listas `st_*_list` existem e vêm **vazias** (são de casos streaming, não do FTS).

O dado por consulta **existe em memória e é descartado**. Em
`vectordb_bench/backend/runner/serial_runner.py:238-240`:

```python
recalls.append(calc_recall_fts(self.k, gt, results))
ndcgs.append(calc_ndcg_fts(self.k, gt, results))
mrrs.append(calc_mrr_fts(self.k, gt, results))
```

e o método devolve apenas `(avg_recall, avg_ndcg, avg_mrr, p99, p95)` (linha ~275). Os arrays morrem no
`return`.

**Persistí-los exigiria mudar a tupla de retorno, todos os chamadores e o dataclass `Metric`** — uma
alteração que ripplaria pelo `serial_runner` e pelo `task_runner`, os arquivos de núcleo que a Política de
Fork (D3) manda não tocar, porque é o que mantém o fork rebaseável e a saída dele real.

### O caminho que não exige fork: o arnês já expõe as três peças

| Peça | Onde | O que dá |
|---|---|---|
| Consultas + qrels | `FtsDatasetManager.queries_data`, `.gt_data`, `.recall_queries_data`, `.recall_gt_data` (`dataset.py`) | o mesmo conjunto que a corrida oficial usa |
| Métrica por consulta | `metric.calc_ndcg_fts`, `calc_recall_fts`, `calc_mrr_fts` | a **mesma função** que a corrida usa — métrica idêntica por construção, não por coincidência |
| Caminho de consulta | `VectorDB.search_documents` — a porta que TheoDB, Elastic e OpenSearch implementam | o mesmo código que o benchmark exercita |

Um avaliador que dirija o laço de consultas sobre a **porta** obtém arrays por consulta sem tocar em
`runner/` nem em `metric.py`. E ganha uma propriedade que persistir no arnês não daria de graça: **os dois
sistemas veem as mesmas consultas na mesma ordem por construção**, que é o pré-requisito de um teste pareado.

**A verificação que torna isso confiável:** a média dos nossos valores por consulta tem de bater com o
agregado que o arnês publicou. Se não bater, algo divergiu e descobrimos — em vez de publicar um `p` sobre
números que não são os da tabela.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| Fork do VectorDBBench | **nenhum** — o avaliador consome as abstrações, não altera o núcleo |
| `theo-db` | ferramenta nova em `benchmarks/`; nenhuma linha da extensão |
| Artefatos já publicados | os três ganham `p` e IC **por acréscimo**; se algum resultado não sobreviver, é acréscimo também |
| Alegações futuras | passam a exigir o teste — é o que o ADR-0061 declara faltar |
| Custo por comparação | um passe de consultas por sistema (6.980 consultas no MS MARCO, ~4 s a 1.800 QPS) — desprezível ao lado da corrida completa |

## Corner 4 — Verification

1. `significance.py` recuperado passa a própria suíte de 112 linhas, sem alteração.
2. A média dos valores por consulta **bate com o agregado publicado** pelo arnês, dentro do arredondamento
   de 4 casas que ele aplica — provado por asserção, não por inspeção.
3. Os três artefatos publicados ganham `p`, IC 95% e contagem de vitórias/derrotas/empates.
4. Um resultado que **não** sobreviva ao teste é publicado como não-significativo, por acréscimo.
5. O avaliador funciona igual para os três motores, porque depende só da porta `VectorDB`.

## Reclassificação

`suggested_mode: evolve` mantido. Não é defeito — é instrumento ausente. O que a descoberta mudou é o
**caminho**: o item dizia "recuperado do histórico ou reescrito, o que for menor", e a resposta é
**recuperar** o teste estatístico e **escrever** apenas o avaliador por consulta, que é a peça que nunca
existiu.
