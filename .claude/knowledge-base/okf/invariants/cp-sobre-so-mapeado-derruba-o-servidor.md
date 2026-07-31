---
type: Invariant
title: `cp` sobre um `.so` MAPEADO troca as páginas sob os processos vivos — e o postmaster não re-executa, então ele entra em loop de crash
description: Medido 2026-07-31: um `cp` do .so novo sobre o instalado matou o vectorizer worker com signal 11 e derrubou o cluster. `restart` não resolve — só `stop` + `start`, porque o postmaster guarda o mapeamento corrompido e cada filho o herda.
resource: .claude/knowledge-base/okf/log.md
tags: [plataforma, so, mmap, postgres, deploy, crash]
timestamp: 2026-07-31T00:00:00Z
---

# `cp` sobre um `.so` mapeado derruba o servidor

## O caso medido (2026-07-31)

Depois de construir o `.so` novo, instalei com o comando óbvio:

```bash
cp target/release/libtheodb_rs.so /opt/pg18/lib/postgresql/theodb_rs.so
```

Segundos depois:

```
LOG:  background worker "theodb vectorizer worker" (PID 139467) was terminated by signal 11: Segmentation fault
LOG:  terminating any other active server processes
LOG:  all server processes terminated; reinitializing
```

`cp` **trunca e reescreve no MESMO inode**. Todo processo que tinha o arquivo `mmap`ado — sob
`shared_preload_libraries`, isso é o postmaster e cada filho — passou a ver páginas de conteúdo novo em endereços
que o linker resolveu para o layout antigo. O primeiro a tocar uma delas morreu com SIGSEGV, e o postmaster
derrubou o cluster inteiro, como faz para qualquer crash de backend.

## O segundo fato, que é o que impede a recuperação óbvia

Depois do crash o cluster entrou em **loop**: cada novo processo morria igual, mesmo com o arquivo em disco
íntegro (md5 conferido).

A razão é que **o postmaster não re-executa**. Ele é o mesmo processo do arranque, com o mesmo mapeamento
corrompido, e `pg_ctl restart` mantém... nada — `restart` para e sobe um postmaster novo, então em tese
resolveria; o que NÃO resolve é o postmaster tentar se recuperar sozinho (`reinitializing`), porque aí ele forka
filhos que herdam a corrupção dele.

O que funcionou:

```bash
pg_ctl -m immediate stop      # derruba o postmaster corrompido
pgrep -f "postgres -D <data>" # confirmar ZERO processos antes de subir
pg_ctl -w start               # processo novo, mmap novo, arquivo íntegro
```

## A forma correta de instalar

```bash
install -m 755 novo.so /opt/pg18/lib/postgresql/theodb_rs.so.new   # arquivo NOVO, inode novo
mv -f /opt/pg18/lib/postgresql/theodb_rs.so.new \
      /opt/pg18/lib/postgresql/theodb_rs.so                        # rename ATÔMICO
```

`rename(2)` troca a entrada de diretório, não o conteúdo: quem já tem o inode antigo mapeado continua com ele
válido até sair. Depois disso o restart é ordenado, não emergencial.

Vale para qualquer biblioteca carregada: `.so` de extensão, `LD_PRELOAD`, plugin de linker.

## Relacionado, e por que o custo foi pequeno

O gêmeo de 100M havia acabado de passar por `ALTER TABLE … SET LOGGED` (1561 s que pareciam cerimônia). O crash
recovery **trunca toda tabela UNLOGGED** — ver [unlogged-truncado-por-recovery](unlogged-truncado-por-recovery.md).
Se eu tivesse pulado aquele passo, este `cp` teria apagado 30 min de `COPY` mais 26 min de rewrite. A decisão de
persistência se pagou no cenário exato para o qual foi escrita — só que o gatilho fui eu, não um OOM.

## Relacionados

- [invariant/so-obsoleto-sob-shared-preload](so-obsoleto-sob-shared-preload.md) — o vizinho: trocar o `.so` **não** afeta backends vivos até o restart, e por isso os testes rodam contra o binário ANTIGO
- [invariant/unlogged-truncado-por-recovery](unlogged-truncado-por-recovery.md)
- [failure-mode/destruir-antes-de-provar-a-precondicao](../failure-modes/destruir-antes-de-provar-a-precondicao.md)
