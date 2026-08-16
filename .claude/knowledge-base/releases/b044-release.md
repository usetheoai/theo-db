---
slug: b044-stemming
items: [B-044]
date: 2026-08-13
base: 96fb342
head: dd5254d
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — o pilar lexical stemiza, e ficou mais rápido fazendo isso

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

Nenhum gate reprovou. O merge espera aprovação humana — gate **LOCKED** do `cycle-release`, e Regra 4.

## Por que não há corte de versão novo

`cycle-release` manda não disparar com PR de release aberto. Há dois — **#227** e **#228** — e o B-044 entra
na `[0.160.0]`, que passa a cobrir **sete** itens: B-030, B-031, B-033, B-034, B-035, B-040 e B-044.

## O que foi entregue

| | |
|---|---|
| `theodb_rs/src/lexical/analyzer.rs` | novo — o analisador `theodb_en` e o registro |
| `theodb_rs/src/lexical/engine.rs` | schema nomeia a cadeia; registro nos dois pontos de abertura; erro de parse deixa de ser engolido |
| Dependências novas | **zero** — `tantivy 0.26` já traz `Stemmer`, `StopWordFilter` e `Language` |
| Superfície SQL | **inalterada** — `bm25_build`/`bm25_search` mantêm assinatura |
| Migração | **nenhuma** — e não por omissão: o desenho a torna desnecessária |

## Estado verificado

| Gate | Resultado |
|---|---|
| Suíte | **469 passed, 0 failed** (era 457) |
| `/code-quality` | `FAIL_SOFT` — 0 achados HARD, nada novo no lexical |
| `cargo-udeps` (contêiner pinado, contra este código) | **All deps seem to have been used** |
| Bundle OKF | 303 conceitos, 0 erros, 0 warnings |
| `/review` | **`READY_TO_MERGE`**, 4/4 |
| Produto | stemming verificado na imagem construída, antes da corrida |

## O resultado

A/B controlado, **mesma máquina** (`g-16vcpu-64gb`, Xeon 8358, IP 159.65.249.69, destruído), mesmo caso,
mesmo dataset em cache — só a imagem muda:

| | NDCG@10 | recall@10 | MRR | QPS | p99 | build |
|---|---|---|---|---|---|---|
| sem stemming | 0,6962 | 0,8025 | 0,6670 | 1.722,6 | 3,9 ms | 2,19 s |
| **com stemming** | **0,7351** | **0,8464** | **0,7034** | **1.910,3** | **3,5 ms** | 3,21 s |
| delta | **+5,6%** | **+5,5%** | **+5,5%** | **+10,9%** | **−10,3%** | +46,9% |

**Qualidade sobe nos três eixos e o throughput sobe junto** — remover stopwords encurta as listas de postings
mais do que o stemmer as alonga. O único custo é o build: +1,03 s sobre 100.000 documentos.

## O que este ciclo produziu além do código

**Uma conclusão minha desfeita antes de virar alegação pública.** Medi a corrida com stemming num Xeon 8168 e
comparei com a corrida sem, feita antes num Xeon **8358**. O delta aparente era **−31,8% de QPS**. Refeito na
mesma máquina: **+10,9%**. O sinal inverteu.

Métricas de qualidade não mudaram entre as duas leituras — são função determinística do índice e das
consultas. QPS e latência mudaram inteiramente. **É o erro do b035 num eixo novo:** lá o parâmetro era igual
e o ponto de operação não; aqui o rótulo era igual e a máquina não. O ADR-0061 foi estendido para cobrir
antes-e-depois do mesmo motor, não só concorrentes.

**Uma sonda que não confia no rótulo.** Cada corrida do A/B imprime `stemming ativo: 0|1`, medido por
`bm25_build` + consulta flexionada. Sem ela, uma troca de imagem que falhasse em silêncio produziria duas
corridas idênticas rotuladas A e B, e o delta zero seria lido como "não faz diferença".

**Uma migração eliminada em vez de tratada.** O plano previa "a invalidação de índices existentes está
tratada". Não há o que tratar: o Tantivy serializa o nome do tokenizer no schema, então índice antigo diz
`"default"` para sempre. Registrar sob nome próprio transforma o problema em não-problema — e redefinir
`"default"` teria mudado a semântica de busca de toda instalação em silêncio.

## Followups

- **[[B-045]]** — sem significância pareada, o +5,6% é observado, não demonstrado. Limita este resultado
  como limita todos.
- **[[B-047]]** — rodar Elastic e OpenSearch na mesma máquina: agora o handicap de stemming não existe mais,
  e a comparação passa a ser legível.
- **[[B-041]]** — `bm25_search` sobre índice inexistente ainda devolve vazio em silêncio (este ciclo
  consertou o erro de *parse*, não esse).
- Operadores de consulta (frase, booleanos, exclusão, prefixo) continuam ausentes — fora do escopo por
  decisão registrada.
- **B-029** — CI vermelho.

## O que NÃO foi feito

Nenhuma tag criada. Nenhum release publicado. `develop` e `main` intocados. Os dois droplets efêmeros deste
ciclo foram destruídos com suas chaves SSH (verificado: listagem por tag vazia).
