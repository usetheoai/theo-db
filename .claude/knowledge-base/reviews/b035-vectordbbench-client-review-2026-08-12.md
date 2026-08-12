---
slug: b035-vectordbbench-client
items: [B-035]
date: 2026-08-12
base: 88479fe
head: a51c5e3
verdict: READY_TO_MERGE
---

# Review — o cliente que recusa medir o que não pode honrar, e a corrida que inverteu a leitura

## Gates duros do `cycle-review`

| # | Gate | Resultado |
|---|---|---|
| 1 | Testes verdes | **21/21** no fork (`pytest tests/test_theodb.py`), **44/44** no teste de resolução parametrizado do upstream |
| 2 | Lint do upstream (`make lint`) | `black --check` + `ruff check` — **All checks passed** |
| 3 | Segredos commitados | **0** |
| 4 | Commit direto em `main` | não — `workspace` |
| 5 | Trailer de coautoria | **0** |
| 6 | `CHANGELOG.md` atualizado | sim |

`/code-quality`: **`FAIL_SOFT`**, Rust auditado, **0 achados HARD**, 1 SOFT_CAP + 2 SOFT_FLOOR + 1 INFO.

## Cross-validation — 7 de 7

| # | Afirmação do Goal | Como foi verificada | Resultado |
|---|---|---|---|
| G1 | Cliente em fork de diff mínimo, registro resolve | `test_theodb_is_registered_and_resolves`, `test_theodb_cli_command_is_exposed`, `git diff --stat` | ok — **3 arquivos upstream tocados, +19 linhas**; zero em `runner/`, `dataset.py`, `metric.py`, `models.py` |
| G2 | Executa contra o TheoDB real | `test_live_client_loads_indexes_and_searches` (recall real, não contagem de linhas) + `test_live_search_uses_the_index` (`Index Scan` no plano) | ok |
| G3 | Parâmetro de build não honrado falha alto | 5 testes, incluindo asserção sobre a mensagem conter parâmetro, valor pedido, valor honrado e `B-036` | ok |
| G4 | Métricas mapeiam para opclass e operador | parametrizado sobre L2/COSINE/IP + recusa de `HAMMING` | ok |
| G5 | Instalável por terceiro | venv limpa fora de qualquer checkout; `vectordbbench theodbhnsw --help` exit 0 | ok |
| G6 | Corrida real com recall, mesmo PG18 | droplet `g-16vcpu-64gb`, gate de versão verificou `180004` dos dois lados antes de medir | ok |
| G7 | O artefato publica o que NÃO cobre | seção explícita em `wiki/benchmarks/b035-theodb-vs-pgvector-pg18.md` | ok |

## O resultado, e por que ele importa mais que o código

| Motor | `ef_search` | recall@10 | QPS | build |
|---|---|---|---|---|
| pgvector 0.8.6 | 64 | 0,9835 | **3.590,6** | **53,9 s** |
| TheoDB | 64 | 0,9600 | 4.536,8 | 144,8 s |
| **TheoDB** | **128** | **0,9829** | **3.086,1** | — |

**A recall casado o pgvector faz +16,3% de QPS e constrói o índice 2,7× mais rápido.**

A primeira linha lida sozinha diria TheoDB +26%. É verdade e é enganoso: ali o TheoDB entrega recall 0,96
contra 0,9835 — é mais rápido porque procura menos. **A configuração "igual dos dois lados" era a armadilha**,
não a comparação justa.

Reprodutibilidade: duas corridas independentes em `ef_search=64` deram 4.598,0 e 4.536,8 QPS (**1,3%**),
recall 0,9594 e 0,9600.

## Achados

### R-1 — ALTO · O cliente aceitava um pgvector e publicaria os números como TheoDB

Encontrado apontando o cliente para o contêiner do pgvector: **conectou, criou a extensão, criou a tabela e
teria rodado o caso inteiro.** Toda sondagem que o cliente fazia é satisfeita pelo pgvector — tem o tipo
`vector`, um access method `hnsw` e `vector_l2_ops`.

Medição rotulada errada é pior que medição falhada: nada a jusante distingue de um resultado real.

Corrigido com checagem de identidade contra `theodb_hnsw` em `pg_am` — o AM own-code que o alias `hnsw`
deliberadamente sombreia e que o pgvector não tem (medido: 1 no TheoDB, 0 no pgvector). Teste de regressão
usa o pgvector como controle negativo.

**Os 18 testes unitários não pegaram isto.** A verificação contra o produto pegou.

### R-2 — MÉDIO · `register_vector` antes do `CREATE EXTENSION` — o mesmo defeito do upstream

A corrida do pgvector falhou no droplet com `vector type not found in the database`. Causa: o cliente
upstream chama `register_vector` (`pgvector.py:93`) **antes** de `CREATE EXTENSION` (`pgvector.py:61`), e
`register_vector` procura o tipo no catálogo.

**Meu cliente tinha o mesmo defeito, copiado junto com a estrutura.** Ficava invisível porque o `template1`
da imagem do TheoDB carrega `theodb_rs` e `vector`. Corrigido: extensão primeiro, com `CASCADE` (o shim
`vector` requer `theodb_rs`).

Não é cosmético: passar dessa linha é o que permite a checagem de identidade dar a mensagem projetada em vez
de um `vector type not found` opaco.

**O teste de regressão estava errado antes de estar certo.** Minha primeira versão removia só a extensão
`vector` — mas o **tipo** pertence ao `theodb_rs`, então sobrevivia, o cenário nunca se materializava e o
teste passava contra o código não corrigido. Medi, corrigi o teste, e só então ele ficou vermelho.

### R-3 — MÉDIO · O runner não falhava quando a corrida falhava

`vectordbbench` sai com **código 0** mesmo quando o caso falha, e imprime uma linha de resumo com recall 0,0
que parece resultado. Meu `run.sh` usava `if ! vectordbbench …` — inoperante.

Corrigido: sucesso é verificado no **log** (`grep "failed to run"`), não no código de saída. Mais um pré-voo
TCP que falha em 1 segundo em vez de depois da carga do dataset.

O gate provou-se na prática: quando o pgvector falhou, o runner recusou publicar meia comparação.

### R-4 — BAIXO · Dois erros de operação meus, ambos custaram tempo

1. **Contêiner "healthy" sem porta publicada.** Um `docker compose up` parcial reaproveitou um contêiner com
   a rede quebrada; `docker ps` dizia `Up (healthy)` e `docker port` não devolvia nada. Motivou o pré-voo TCP.
2. **`| tee` num processo destacado.** Quando o shell pai morreu, o `tee` morreu junto e o filho travou em
   `pipe_read` — corrida viva, sem escrever nada, parecendo travada. Trocado por redirecionamento direto.

### R-5 — INFORMATIVO · `DiskFull` que não era disco

A primeira corrida morreu com `could not resize shared memory segment: No space left on device` — com
**134 GB livres no host**. Era `/dev/shm` a 64 MB (padrão do Docker) contra `maintenance_work_mem=1GB`.
Corrigido com `shm_size: 2gb`, igual nos dois motores.

Vale registrar porque a mensagem aponta para o lugar errado, e limpar disco não teria resolvido nada.

### R-6 — INFORMATIVO · O cap do `cargo-udeps` tem causa raiz, enfim

Dispara há três ciclos. A causa: **1.226 arquivos em `theodb_rs/target/` pertencem ao `root`**, resíduo de
builds em contêiner que montaram o diretório do host. O `cargo-udeps` roda como `paulo` e não consegue
sobrescrever (`failed to write .../fingerprint/zstd-…`).

`sudo` sem senha não está disponível, então não forcei.

**Correção por acréscimo, escrita depois de tentar o contorno:** o diagnóstico acima estava *incompleto*, e a
tentativa de contornar com `CARGO_TARGET_DIR` próprio revelou a causa mais funda —

```
Error: /home/paulo/.pgrx/config.toml not found.  Have you run `cargo pgrx init` yet?
```

**O host nunca instalou o pgrx.** O build deste crate acontece dentro do `theodb-toolchain`, e é lá que
`cargo pgrx init` foi executado. Então `cargo-udeps` no host não falha por permissão — falha porque o
ambiente de build não existe ali, e nenhum `chown` conserta isso.

Os 1.226 arquivos root no `target/` são reais e valem limpar, mas são o **segundo** obstáculo, não o
primeiro. O caminho correto é rodar o auditor dentro do contêiner pinado — que é o padrão já estabelecido no
projeto para `clippy`/`fmt`.

## O que este review NÃO cobriu

- **Nenhum agente independente.** Mesmo agente que implementou. Três dos seis achados vieram de rodar contra
  o produto, não de revisar código.
- **Sem significância pareada.** Duas amostras concordantes não são um teste. Qualquer alegação comparativa
  precisa dela por cima.
- **Um só ponto do pgvector.** Varri `ef_search` apenas do lado do TheoDB.
- **Escala 50K.** Menor caso padrão do arnês; boa parte do custo aí é cliente Python.
- **Os testes do cliente rodam no fork, não neste repositório.** `/code-quality` audita Rust; o Python do
  cliente não passa por nenhum gate deste repo.
- **O CI segue vermelho** (B-029).

## Veredito

**`READY_TO_MERGE`.**

7 de 7 afirmações verificadas; nenhum gate duro disparou; 0 achados HARD no `/code-quality`. O
`FAIL_SOFT` vem de dois caps de ambiente (`cargo-udeps` sem permissão de escrita, crates locais não
verificáveis em crates.io) num ciclo que **não altera uma linha de Rust**.

**Ressalvas:** review do próprio implementador; três defeitos do cliente só apareceram rodando contra o
produto; e o número publicado — pgvector +16% a recall casado — é uma observação de duas amostras, não um
resultado com significância.
