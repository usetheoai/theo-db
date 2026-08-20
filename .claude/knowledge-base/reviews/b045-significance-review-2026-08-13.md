---
slug: b045-significance
items: [B-045]
date: 2026-08-13
base: 1a02e66
head: 82b620e
verdict: READY_TO_MERGE
---

# Review — o `p` que faltava, e a paridade que sobreviveu a ele

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Suíte da ferramenta | **22 passed, 0 failed** — sem banco, sem rede |
| 2 | Suíte Rust | inalterada (469) — este ciclo **não toca** a extensão |
| 3 | Segredos commitados | **0** |
| 4 | Commit direto em `main` | não — `workspace` |
| 5 | Trailer de coautoria | **0** |
| 6 | `CHANGELOG.md` atualizado | sim |
| 7 | Bundle OKF | **304 conceitos, 0 erros, 0 warnings, 0 órfãos** |

`/code-quality`: `FAIL_SOFT`, **0 achados HARD**. Os dois caps são os de ambiente conhecidos, num ciclo que
não altera uma linha de Rust.

## Cross-validation — 4 de 4

| # | Afirmação do Goal | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | O teste volta e passa a própria suíte | `diff` contra `git show 7cd157d^:…` **vazio**; 6 testes originais passam | ok — **byte a byte** |
| G2 | Dado por consulta reusando as abstrações, sem tocar o núcleo | `git diff` do fork neste ciclo: **0 arquivos**; dois testes estruturais garantem que o avaliador não nomeia motor nem paraleliza | ok |
| G3 | A média por consulta bate com o agregado publicado | `run_lexical_significance.py` verificou os **três** motores com tolerância `5e-4` antes de emitir qualquer `p` | ok — nenhuma divergência |
| G4 | Os artefatos ganham `p`, IC e contagem | `b047` atualizado com a tabela completa; `b040` atualizado dizendo **por que não** ganha | ok |

## O resultado

Permutação pareada, 100.000 reamostragens, semente fixa, **n = 6.980 consultas**:

| comparação | diff médio (NDCG) | IC 95% | p (perm) | p (t) | V/D/E | `d_z` |
|---|---|---|---|---|---|---|
| TheoDB vs Elasticsearch | +0,00066 | [−0,0011, +0,0025] | **0,477** | 0,475 | 233 / 263 / 6.484 | 0,009 |
| TheoDB vs OpenSearch | +0,00068 | [−0,0011, +0,0025] | **0,466** | 0,463 | 235 / 268 / 6.477 | 0,009 |
| Elasticsearch vs OpenSearch | +0,00002 | [−0,0002, +0,0002] | 0,912 | 0,843 | 9 / 10 / 6.961 | 0,002 |

**A paridade do b047 sobreviveu ao teste**, e sobreviveu do jeito forte: IC estreito centrado em zero, com
6.484 das 6.980 consultas empatando **exatamente**. Não é média que esconde variação.

## Achados

### R-1 — ALTO · O guard do B-041 disparou de verdade, e evitou um NDCG 0 publicado

Ao apontar o avaliador para a coleção errada (`vdb_bench_index` em vez de `theodb_collection`, que é o nome
que a carga do arnês criou), o cliente **recusou buscar**:

```
RuntimeError: lexical index 57249680128590 for collection 'vdb_bench_index' was never built:
bm25_search would return an empty result that is indistinguishable from 'nothing matched'.
```

Sem esse guard — construído no B-044 e registrado como [[B-041]] — a busca teria devolvido vazio para as
**6.980 consultas**, o NDCG do TheoDB sairia 0, e o `p` diria que o Elasticsearch é dramaticamente superior.
O gate de agregado teria pegado depois; o guard pegou antes, com a mensagem certa.

**É a primeira vez neste ciclo que uma defesa construída num item anterior salva o item seguinte.**

### R-2 — MÉDIO · A verificação contra o agregado é o que torna o `p` confiável

`run_lexical_significance.py` compara a média por consulta com o número que a corrida publicou, para os três
motores, antes de emitir qualquer `p`. Os valores reproduziram: TheoDB 0,7351 (publicado 0,7351), Elastic
0,7344 (0,7343), OpenSearch 0,7344 (0,7344).

Sem essa checagem, um `p` correto sobre números que não são os da tabela seria pior que nenhum `p` — teria a
aparência de rigor e a substância de outra medição. A tolerância é `5e-4` (metade da última casa do
`round(..., 4)` do `serial_runner`), **constante e não ajustável por motor**.

### R-3 — MÉDIO · A ferramenta distingue os dois significados de um `p` alto

`verdict()` separa explicitamente:

- `p` alto com **IC estreito** em torno de zero → evidência de equivalência
- `p` alto com **IC largo** → falta de poder, e **não** evidência de equivalência

Tratá-los como a mesma coisa é exatamente como se afirma paridade sem tê-la medido. Aqui o caso é o primeiro
(largura 0,0036 em NDCG com n=6.980), e o artefato diz isso em vez de dizer só "não significativo".

### R-4 — MÉDIO · Dois erros meus, ambos pegos por mecanismo e não por revisão

1. **`q.qid`** — o campo de `FtsQuery` é `query_id`. Falhou no import do dado, alto e imediato.
2. **Nome de coleção errado** — pego pelo guard do R-1.

Ambos falharam de forma barulhenta. Nenhum dos dois teria sido pego por leitura de código, e é isso que os
torna dignos de nota: o desenho defensivo fez o trabalho que a atenção não faria.

### R-5 — BAIXO · Um critério de aceite meu reprovava a própria documentação

O T1.1 exigia `grep -c "run_m53_hybrid_beir" … equals 0`, mas o comentário que **explica** a remoção dos 4
testes contém o nome. Satisfazer a letra significaria apagar a explicação.

Corrigido por acréscimo no plano: o que o critério quis dizer é "nenhuma **dependência**", e dependência é
`import`. O `grep` correto dá 0.

### R-6 — INFORMATIVO · O que deliberadamente NÃO ganhou teste

- **Os 4,3× de QPS.** QPS não tem valor por consulta; o pareado não se aplica. O caminho é N corridas
  repetidas, e é item próprio.
- **O +5,6% do stemming.** As duas corridas do A/B usaram imagens diferentes e os arrays por consulta do
  lado *sem* stemming não foram preservados. O artefato do b040 **diz isso**, em vez de deixar o leitor supor
  que o número foi testado junto.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou.
- **Um corpus, um `k`, um idioma.** MS MARCO 100K, k=10, inglês.
- **A escolha do teste não foi contestada.** Permutação pareada é o que a literatura de IR recomenda e a
  justificativa está no código recuperado; quem discordar tem os arrays persistidos para recomputar.
- **O avaliador não foi exercitado com um quarto motor** — o desenho permite, mas isso é afirmação de
  estrutura, não medição.
- **O CI segue vermelho** (B-029).

## Veredito

**`READY_TO_MERGE`.**

4 de 4 afirmações verificadas; 22 testes verdes; 0 achados HARD; o resultado medido contra os três motores
reais com a média verificada antes de qualquer `p`.

**Ressalvas:** review do próprio implementador; e o item fecha a lacuna **para métricas de qualidade**, não
para velocidade — a diferença de QPS, que é a maior que temos, continua sem teste, e o artefato diz isso.
