---
type: Invariant
title: Ler um arquivo exige o bit x em TODO diretório do caminho — e o erro acusa o arquivo, não o diretório
description: Um TSV de 70 GB em 644 ficou inalcançável porque o pai era /root em 700. O erro é "Permission denied" sobre o ARQUIVO, e quem lê o `ls -l` do arquivo conclui que a permissão está certa.
resource: benchmarks/m169_rebuild_heap.sh
tags: [plataforma, permissao, filesystem, benchmark, erro-enganoso]
timestamp: 2026-07-31T00:00:00Z
---

# Ler um arquivo exige o bit `x` em **todo** diretório do caminho

## O caso medido (2026-07-31)

A recarga do gêmeo `hits_heap` a 100M abortou com:

```
/root/theo-db/benchmarks/.cache/hits_sample.tsv: Permission denied
```

O `ls -l` do arquivo dizia o contrário:

```
/root                                   root:root      700
/root/theo-db/.../hits_sample.tsv       pgtest:pgtest  644     ← legível por todos
```

O arquivo era **world-readable**. O bloqueio estava em `/root`, modo `700`: para **abrir** um caminho, o
processo precisa do bit `x` em **cada** diretório atravessado. Sem `x` no pai, o arquivo é inalcançável por
mais permissivo que ele seja.

## Por que isto engana especificamente

O kernel reporta o erro **no caminho pedido**, que é o do arquivo. Quem depura olha `ls -l <arquivo>`, vê `644`,
e conclui que permissão não é a causa — passando a procurar SELinux, AppArmor, mount `noexec`, bug do cliente.
A informação que resolve está um nível acima e não aparece em lugar nenhum da mensagem.

`namei -l <caminho>` imprime a permissão de cada componente e mata a dúvida em uma linha.

## O agravante em `\copy` vs `COPY`

As duas formas leem por processos **diferentes**, e trocá-las muda quem precisa da permissão:

| | quem abre o arquivo |
|---|---|
| `COPY … FROM '/path'` | o **servidor** PostgreSQL (usuário do postmaster) |
| `\copy … FROM '/path'` | o **cliente** psql (usuário do shell) |

Uma carga que funcionou como `root` e falha como `pgtest` não é regressão do produto: é o mesmo comando com
outro leitor. Por isso a prova tem de ser feita **como o usuário que vai ler**, não pelo operador:

```bash
sudo -u "$PGOSUSER" head -c1 "$TSV" >/dev/null   # 1 byte basta e é O(1)
```

`test -r` rodado por outro usuário não vale — ele responde sobre quem pergunta, não sobre quem vai ler.

## A regra

1. Arquivo que será lido por um serviço vive **fora** de `$HOME` de qualquer usuário — `/root` é `700` por
   padrão e nada lá dentro é alcançável por outro processo.
2. Antes de qualquer passo destrutivo que dependa da leitura, **leia 1 byte como o usuário que vai ler**
   (ver [guard antes de materializar o pendente](../failure-modes/destruir-antes-de-provar-a-precondicao.md)).
3. Ao depurar `Permission denied`, rode `namei -l` **antes** de olhar o `ls -l` do arquivo.
4. Mover entre diretórios do **mesmo filesystem** é `rename(2)` — O(1). Realocar 70 GB para fora de `/root`
   custou zero I/O (`df` inalterado). Não use "é grande demais para mover" como argumento sem checar o `df`.

## Relacionados

- [failure-mode/destruir-antes-de-provar-a-precondicao](../failure-modes/destruir-antes-de-provar-a-precondicao.md)
- [failure-mode/erro-generico-torna-o-bug-irreproduzivel](../failure-modes/erro-generico-torna-o-bug-irreproduzivel.md) — a mensagem que aponta para o lugar errado
- [failure-mode/o-sintoma-nomeia-a-fase-errada](../failure-modes/o-sintoma-nomeia-a-fase-errada.md)
