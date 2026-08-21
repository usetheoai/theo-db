---
item: B-050
upstream: zilliztech/VectorDBBench
arquivo: vectordb_bench/backend/clients/oss_opensearch/oss_opensearch.py
estado: PREPARADO — aguarda decisão do owner sobre publicar
data: 2026-08-20
---

# PR upstream preparado, não enviado

O `cycle-backlog` deste projeto autoriza abrir PRs **neste** ecossistema. Um PR em
`zilliztech/VectorDBBench` é outra coisa: é público, permanente, atribuído ao owner e interage com
uma comunidade de terceiro. Fica preparado aqui e sai quando ele disser.

## O defeito, verificado no upstream em 2026-08-20

Busca de código no `zilliztech/VectorDBBench` confirma que ele **continua presente**:

```python
# oss_opensearch.py:21
REPLICA_HEALTH_TIMEOUT: Final[str] = "30m"

# oss_opensearch.py:810-816
def _wait_till_green(self):
    response = self.client.cluster.health(
        index=self.index_name,
        wait_for_status="green",
        timeout=REPLICA_HEALTH_TIMEOUT,     # <- string
    )
```

`_wait_till_green` é chamado por `_update_replicas` (`:808`), que `optimize()` chama
**incondicionalmente nos dois ramos** — o de FTS (`:775`) e o vetorial (`:784`). Não é caminho lento:
é caminho morto para toda corrida de OpenSearch.

Erro observado, antes de qualquer requisição sair:

```
ValueError: Timeout value connect was 30m, but it must be an int, float or None
```

## A sutileza que o patch precisa respeitar

A mensagem vem do **transporte** (`urllib3`), não da API. Ou seja: naquela versão do `opensearch-py`
o `timeout` **não** era repassado como parâmetro de query da cluster-health API — foi interceptado
como timeout de conexão.

Isso importa porque o `timeout` da cluster-health API **legitimamente** aceita duração (`"30m"`): é
quanto o CLUSTER espera pelo status verde. Trocar por um número muda a semântica de "o cluster
espera 30 min" para "o cliente HTTP espera 1800 s".

E o `opensearch-py` está **sem pin** no `pyproject.toml` do upstream
(`opensearch = ["opensearch-py", "boto3", "requests-aws4auth"]`), então o comportamento varia por
instalação — o que faz deste um bug que alguns veem e outros não, e é a informação mais útil que o
PR pode carregar.

**Consequência para o patch:** a correção de uma linha que usamos no fork (`"30m"` → `1800`)
funciona e foi medida, mas o PR upstream honesto deve **nomear a ambiguidade** e propor a forma que
preserva a intenção — a duração como parâmetro da API e um timeout numérico de transporte —
deixando a escolha final ao mantenedor, que sabe qual versão eles suportam.

## Evidência que o corpo do PR deve trazer

- a mensagem de erro exata, acima;
- que o caminho é **incondicional** a partir de `optimize()`, com as linhas;
- que o `opensearch-py` está sem pin, então o bug é dependente de versão;
- que, com o timeout numérico, a corrida completa — NDCG 0,7344 sobre 6.980 consultas.

## O que o PR NÃO deve mencionar

TheoDB, o nosso cliente, o nosso fork. É conserto do cliente **deles**; misturar transformaria uma
correção óbvia numa discussão de escopo. É o segundo bullet do DoD do B-050, e ele está certo.

## O outro PR, e por que não é este

O cliente FTS do `theodb` é o PR maior, e depende de o revisor conseguir subir um TheoDB. A imagem
agora **é** publicável (`ghcr.io/usetheoai/theo-db:latest`, verificado por `docker pull` anônimo em
2026-08-20, [[B-082]]), o que remove a condição que o [[B-035]] citou para adiá-lo. Continua fora
deste item por escopo — mas a razão que o bloqueava deixou de valer, e isso merece um item próprio.
