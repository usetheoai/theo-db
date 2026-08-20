---
slug: b040-fts-client
items: [B-040]
date: 2026-08-13
base: cd78ab9
head: 86e1ad9
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — o pilar lexical tem instrumento, e o primeiro número público

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

Nenhum gate reprovou. O merge espera aprovação humana — gate **LOCKED** do `cycle-release`, e Regra 4.

## Por que não há corte de versão novo

`cycle-release` manda não disparar com PR de release aberto. Há dois — **#227** (`develop → main`) e **#228**
(`workspace → develop`) —, e o B-040 entra na `[0.160.0]` já cortada, que passa a cobrir **seis** itens:
B-030, B-031, B-033, B-034, B-035 e B-040.

## O que foi entregue

**No fork `usetheoai/VectorDBBench@theodb`:**

| | |
|---|---|
| Diff **deste ciclo** | **2 arquivos** (`theodb.py`, `config.py`), +254/−14. **Zero** linhas no registro |
| Núcleo do arnês | intocado |
| Dependências novas | **zero** |
| Posição | **primeiro cliente PostgreSQL com FTS** no VectorDBBench — os outros cinco são Elastic, OpenSearch, Milvus, Turbopuffer e Vespa |

**Neste repositório:** `benchmarks/vectordbbench/run-fts.sh`, o artefato
`wiki/benchmarks/b040-theodb-fts-msmarco.md`, os brutos em `benchmarks/vectordbbench/results/` e a entrada
em `wiki/log.md`.

## Estado verificado

| Gate | Resultado |
|---|---|
| Testes do cliente | **77/77** |
| `make lint` do upstream | All checks passed |
| Bundle OKF | 302 conceitos, 0 erros, 0 warnings |
| `/code-quality` | `FAIL_SOFT` — **0 achados HARD**; os dois caps são de ambiente ([[B-039]]) |
| `/review` | **`READY_TO_MERGE`**, 6/6 |
| Corrida real | uma, em máquina de referência, com as três métricas de qualidade |

## O resultado

Droplet `g-16vcpu-64gb` (Xeon 8358), **IP 167.172.229.34**, destruído ao fim com a chave SSH efêmera.

| Métrica | Valor |
|---|---|
| **NDCG@10** | **0,6962** |
| **recall@10** | **0,8025** |
| **MRR** | **0,667** |
| QPS (pico, 60 clientes) | 1.616,4 |
| p99 serial | 4,8 ms |
| Carga de 100.000 documentos | 25,6 s |

**O artefato abre declarando o handicap**, não a tabela: o TheoDB não faz stemming, não tem operadores de
consulta e não expõe `k1`/`b`. Elasticsearch e OpenSearch — os motores da mesma tabela pública —
stemmizam por padrão. Um NDCG lido sem isso atribui ao ranqueamento uma diferença que é de
pré-processamento.

## O que este ciclo produziu além do código

**Dois limites do motor, medidos, que o cliente adapta em vez de esconder:**

1. `bm25_build` exige chave **`BIGINT`**; os ids do arnês são strings opacas. Chave substituta gerada pelo
   banco, `JOIN` de volta preservando a ordem do motor. O cliente nunca reordena.
2. `bm25_search` sobre índice nunca construído devolve **zero linhas sem erro** — indistinguível de "nada
   casou". Virou [[B-041]], porque o contorno é do cliente e a superfície pública segue exposta.

**Uma nota do backlog derrubada por medição.** O [[B-004]] dizia que "a superfície não expõe busca
multi-termo" — falso como escrito, e teria feito este item parecer inviável. Multi-termo funciona com scores
acumulados corretos; o que falta são operadores e stemming. Corrigido lá por acréscimo.

**Um critério de aceite que reprovou o próprio artefato.** O T1.6 exigia `stemming` antes de `NDCG` no texto;
na primeira verificação falhou, porque `NDCG` aparecia na descrição do frontmatter. Corrigido — e o critério
fez exatamente o trabalho para o qual foi escrito.

**Uma pergunta em aberto do plano, respondida:** o dataset FTS vem do `ir_datasets` e custa **3,9 GB**;
a corrida inteira levou 887 s com o download incluído.

## Followups

- **Rodar os outros motores no mesmo arnês e na mesma máquina.** É o que falta para haver comparação — hoje
  o artefato tem número nosso e nenhum alheio, deliberadamente.
- **[[B-041]]** — `bm25_search` silencioso sobre índice inexistente.
- **[[B-004]]** — segue aberto: dois corpora ainda não são uma curva.
- **B-029** — CI vermelho; nenhum destes números tem esteira independente.
- **Significância pareada** — o arnês não tem; qualquer alegação comparativa precisa dela por cima.

## O que NÃO foi feito

Nenhuma tag criada. Nenhum release publicado. `develop` e `main` intocados. Droplet destruído (verificado: a
listagem por tag volta vazia) e chave SSH efêmera removida.
