---
item: B-060
repo: theodb-bench
mode: review
date: 2026-08-16
verdict: pending
---

# B-060 — o arnês prova que o índice foi usado, e aceita a palavra do servidor sobre o knob

## Corner 1 — Evidence

### O padrão certo, que já existe

`theodb-bench/src/adapters/postgres.py:325` tem `assert_index_used`, e o docstring diz por que ele existe:

> *"Forcing without verifying proves nothing: the planner may ignore the hint, and the run would report a
> sequential scan under an index's name."*

Ele roda `EXPLAIN (FORMAT JSON)`, procura o nome do índice no plano, e **levanta `AdapterError`** se não achar.
Disciplina exata.

### O mesmo padrão ausente no eixo vizinho

`PgvectorAdapter.set_search_parameters` (`:486-493`), na íntegra:

```python
def set_search_parameters(self, parameters: dict[str, Any]) -> None:
    super().set_search_parameters(parameters)
    for name, value in parameters.items():
        if name == "ef_search":
            self._execute(f"SET hnsw.ef_search = {int(value)}")
        elif name == "probes":
            lists = ivfflat_lists(self._row_count)
            self._execute(f"SET ivfflat.probes = {clamp_probes(int(value), lists)}")
```

Emite o `SET` e retorna. **Nada lê o valor de volta.** E a base (`:279-280`) é apenas
`self._search_parameters = dict(parameters)` — guarda o que foi *pedido*, nunca o que está *em vigor*.

### O mecanismo que torna isso perigoso, e ele não é hipotético

No PostgreSQL, `SET namespace.option = valor` para um namespace **não registrado** **não falha** — é tratado
como placeholder customizado. O `SET` sucede, e a busca roda no default do motor.

Duas medições independentes provam que a classe morde:

| Onde | O que foi medido |
|---|---|
| **Nosso produto**, [[B-034]] | `SET hnsw.ef_search = N` era **aceito em silêncio e não fazia nada** — meia compatibilidade pior que nenhuma, porque o usuário acredita que ajustou |
| **AlloyDB**, avaliação independente 2026-08-15 | `SET scann.num_leaves_to_search = N` **não tem efeito** sem `LOAD 'alloydb_scann'` antes, e não avisa. O avaliador pediu busca profunda, recebeu **recall 0,15**, e perdeu uma corrida de **10 milhões de vetores** sem saber por quê |

Para o pgvector a lacuna é benigna hoje: `hnsw` e `ivfflat` são namespaces registrados pela extensão, então o
`SET` de fato aplica. **A lacuna é benigna por sorte da configuração, não por desenho** — e deixa de ser no
instante em que um motor exige um passo extra (`LOAD`), ou em que a extensão não está carregada, ou em que o
nome muda de versão.

### O que o bundle registra hoje

`BuildOutcome` tem `parameters_in_force` (`:483`) — o build **já** distingue pedido de vigente. A busca não tem
equivalente: `set_search_parameters` guarda o pedido e o bundle publica esse pedido como se fosse o ponto de
operação medido.

**É a assimetria que decide o item:** o mesmo arnês que se recusa a chamar seqscan de índice aceita chamar
`ef=64` de `ef=200`.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `src/adapters/postgres.py` | `PostgresAdapter` ganha a verificação; `PgvectorAdapter` declara o mapa em vez de emitir `SET` solto |
| `src/adapters/fake.py` | `FakeAdapter` (`:284`) tem a mesma forma; precisa participar do contrato ou declará-lo inaplicável |
| `src/adapters/base.py` (`:484`) | a assinatura do contrato pode ganhar o retorno do efetivo |
| `src/bench/vector.py:347` | único chamador; não muda de forma |
| Bundles já publicados | **nenhum é invalidado** — para pgvector/theodb o `SET` de fato aplicou (namespaces registrados). O que muda é a garantia, não os números |
| Schemas versionados (11) | se o efetivo entrar no bundle, é campo novo → **bump de schema**, e isso é decisão a declarar |
| [[B-059]] (adapter do Omni) | **depende disto**: sem o portão, a primeira corrida contra o ScaNN pode publicar um default raso como vitória nossa |

## Corner 4 — Verification

1. Um adapter que aceita o parâmetro e o ignora é **recusado** — provado com um duplo que faz exatamente isso,
   não só com o caminho feliz.
2. O valor **efetivo** é lido do servidor (`current_setting`), não inferido.
3. O bundle carrega pedido **e** efetivo, ou o gate recusa antes de medir.
4. Os 627 testes existentes seguem verdes.
5. Vale para **todo** adapter, não só o do Omni.

## Reclassificação

`suggested_mode: review` mantido — o achado é de leitura de código, e a evidência de que a classe morde vem de
duas medições anteriores (uma nossa, uma independente), não de execução nova neste item.
