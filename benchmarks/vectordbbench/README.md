# VectorDBBench — TheoDB × pgvector

Arnês de comparação multi-sistema, do zero. O cliente `theodb` vive no fork
[`usetheoai/VectorDBBench@theodb`](https://github.com/usetheoai/VectorDBBench/tree/theodb);
aqui ficam apenas as duas coisas que são decisão **nossa** sobre a **nossa** medição: a
igualdade de versão do PostgreSQL entre os dois lados, e o comando exato que produziu o
artefato publicado.

## Por que o arnês é este

`zilliztech/VectorDBBench` — MIT (passa o gate D1), mantido (último push 2026-08-11), com
clientes para `pgvector`, `pgvectorscale`, `pgdiskann`, `vectorchord` e `alloydb`. O que o
qualifica acima das alternativas avaliadas é medir **recall**: um arnês que reporta só
latência e vazão permite que um índice fique rápido errando, e a tabela não mostra.

## Reproduzir

```bash
# 1. os dois bancos, na mesma versão de PostgreSQL
docker compose -f benchmarks/vectordbbench/docker-compose.yml up -d

# 2. o arnês, com o cliente TheoDB, em ambiente limpo
uv venv --python 3.11 /tmp/vdbb && . /tmp/vdbb/bin/activate
uv pip install "vectordb-bench[theodb] @ git+https://github.com/usetheoai/VectorDBBench@theodb"

# 3. a corrida (baixa ~300 MB de dataset na primeira vez)
CASE=Performance1536D50K K=10 EF_SEARCH=64 ./benchmarks/vectordbbench/run.sh
```

`run.sh` **recusa rodar** se `server_version_num` divergir entre os dois contêineres. Não é
um aviso: o compose do upstream fixa `pgvector/pgvector:pg16` e o TheoDB é PG18-only —
comparar assim mediria a diferença entre duas versões do PostgreSQL e a atribuiria ao
índice.

Os dois contêineres sobem com `shared_buffers` e `maintenance_work_mem` idênticos, pela
mesma razão: um banco com mais cache que o outro mede o cache.

## O que este arnês NÃO faz

- **Não tem teste de significância estatística.** O `theodb_bench` removido tinha
  (randomização pareada de Smucker/Allan/Carterette). Uma diferença de QPS entre duas
  corridas aqui é uma observação, não um resultado significativo — e qualquer alegação
  comparativa precisa da significância por cima.
- **Não varre `m` nem `ef_construction`.** O TheoDB os fixa em 16 e 64
  (`theodb_rs/src/am/build.rs:22-23`), e o cliente **recusa** qualquer outro valor em vez de
  aceitá-lo e ignorá-lo. Item de backlog: B-036.
- **Não cobre `ivfflat`** (o alias do AM não existe — B-037) nem **`halfvec`/`sparsevec`**
  (os tipos não existem — B-038).
- **Não cobre busca com filtro.** O cliente declara só `NonFilter`; a superfície de filtro
  do TheoDB existe mas não foi medida aqui.

## Cliente no fork, arnês aqui

Manter o cliente Python neste repositório significaria duas cópias do mesmo código, e o
"diff mínimo" que a Política de Fork (D3) exige viraria ficção — o fork conteria uma
cópia. O fork tem uma saída declarada: morre quando o PR upstream for aceito.
