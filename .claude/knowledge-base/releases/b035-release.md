---
slug: b035-vectordbbench-client
items: [B-035]
date: 2026-08-12
base: 88479fe
head: 0b49c87
verdict: PR_OPEN_AWAITING_APPROVAL
---

# Release — o arnês de benchmark volta, e traz um resultado que inverte a leitura ingênua

## Veredito: `PR_OPEN_AWAITING_APPROVAL`

Nenhum gate reprovou. O merge espera aprovação humana — gate **LOCKED** do `cycle-release`, e Regra 4.

## Por que não há corte de versão novo

O `cycle-release` manda **não disparar** quando já existe PR de release aberto. Há dois: **#227**
(`develop → main`) e **#228** (`workspace → develop`). O B-035 entra na `[0.160.0]`, já cortada e coberta
pelo #228 — que agora leva cinco itens: B-030, B-031, B-033, B-034 e B-035.

## O que foi entregue

**No fork `usetheoai/VectorDBBench@theodb`** (3 commits, criado neste ciclo):

| | |
|---|---|
| Diff sobre o upstream | **3 arquivos tocados, +19 linhas**, mais 1 diretório de cliente e 1 arquivo de teste |
| Núcleo do arnês | **intocado** — `runner/`, `dataset.py`, `metric.py`, `models.py` sem uma linha alterada |
| Dependências novas | **zero** — reusa `psycopg`, `psycopg-binary`, `pgvector`, que o extra `pgvector` já declara |
| Instalação | `pip install "vectordb-bench[theodb] @ git+https://github.com/usetheoai/VectorDBBench@theodb"`, verificado em venv limpa fora de qualquer checkout |

**Neste repositório:** `benchmarks/vectordbbench/` (compose + runner + init), o artefato em
`wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md`, os brutos em `benchmarks/vectordbbench/results/` e a
entrada em `wiki/log.md`.

## Estado verificado

| Gate | Resultado |
|---|---|
| Testes do cliente | **21/21** |
| Teste de resolução do upstream (parametrizado sobre todo o enum `DB`) | **44/44** |
| `make lint` do upstream (`black --check` + `ruff check`) | **All checks passed** |
| `/code-quality` | `FAIL_SOFT` — **0 achados HARD**; os dois caps são de ambiente, num ciclo que não altera Rust |
| `/review` | **`READY_TO_MERGE`**, 7/7 |
| Corrida real | duas, em máquina de referência, com recall medido |

## O resultado

Droplet `g-16vcpu-64gb` (o `16c64g` que é o rótulo de referência do próprio upstream), **IP 164.90.141.31**,
efêmero e destruído ao fim junto com a chave SSH dedicada.

| Motor | `ef_search` | recall@10 | QPS | build do índice |
|---|---|---|---|---|
| pgvector 0.8.6 | 64 | 0,9835 | **3.590,6** | **53,9 s** |
| TheoDB | 64 | 0,9600 | 4.536,8 | 144,8 s |
| **TheoDB** | **128** | **0,9829** | **3.086,1** | — |

**A recall casado o pgvector faz +16,3% de QPS e constrói o índice 2,7× mais rápido.**

A leitura ingênua da primeira linha diria TheoDB +26%. Ali o TheoDB entrega recall 0,96 contra 0,9835: é
mais rápido porque procura menos. **A configuração "igual dos dois lados" era a armadilha, não a comparação
justa** — e é o achado mais reutilizável deste ciclo.

## O que este ciclo produziu além do código

**Três defeitos do próprio cliente, todos encontrados rodando contra o produto e nenhum pelos testes:**

1. O cliente **aceitava um pgvector** e publicaria os números sob o rótulo TheoDB. Toda sondagem que ele
   fazia é satisfeita pelo pgvector.
2. Registrava os adaptadores **antes** de criar a extensão — o mesmo defeito que quebrou o cliente upstream
   num host limpo, invisível aqui porque o `template1` da imagem já traz as extensões.
3. O runner **não falhava** quando a corrida falhava: a CLI sai com 0 sobre um caso que falhou e imprime uma
   linha de resumo com recall 0,0 que parece resultado.

**Um teste que estava errado antes de estar certo.** A primeira versão do teste de regressão do item 2
removia só a extensão `vector` — mas o **tipo** pertence ao `theodb_rs` e sobrevivia, então o cenário nunca
se materializava e o teste passava contra o código não corrigido.

**Um `DiskFull` que não era disco:** `/dev/shm` a 64 MB dentro do contêiner, com 134 GB livres no host.

**A causa raiz do cap do `cargo-udeps`, que dispara há três ciclos:** 1.226 arquivos em `theodb_rs/target/`
pertencem ao `root`, resíduo de builds em contêiner. O conserto é `chown -R paulo:paulo theodb_rs/target` e
é do dono da máquina.

## Followups

- **B-036 / B-037 / B-038** — as três lacunas de compatibilidade que a descoberta mediu; a primeira é o que
  impede varrer o eixo de qualidade de grafo em qualquer benchmark.
- **B-029** — o CI segue vermelho; nenhum destes números tem esteira que o valide de forma independente.
- **PR upstream** — fora de escopo por decisão registrada no item: a imagem do produto nunca foi publicada
  (10/10 falhas do publish), e um revisor do upstream precisaria compilar do fonte.
- **Significância pareada** — o arnês não tem. Qualquer alegação comparativa precisa dela por cima.

## O que NÃO foi feito

Nenhuma tag criada. Nenhum release publicado. `develop` e `main` intocados. O droplet foi destruído
(verificado: a listagem por tag volta vazia) e a chave SSH efêmera removida.
