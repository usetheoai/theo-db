---
type: Invariant
title: Trocar o .so não afeta backends enquanto o postmaster não reinicia
description: Sob shared_preload_libraries o postmaster mapeia o .so no arranque; substituir o arquivo deixa /proc/PID/maps marcando '(deleted)' e os testes rodam contra o binário ANTIGO.
tags: [pgrx, postgres, build, falso-verde]
timestamp: 2026-07-30T00:00:00Z
---

# Trocar o `.so` não afeta backends enquanto o postmaster não reinicia

## O invariante

Com a extensão em `shared_preload_libraries`, o **postmaster** carrega o `.so` no arranque e cada backend herda o
mapeamento. `cargo pgrx install` substitui o arquivo em disco, mas o processo segue com o inode antigo:

```
$ grep theodb /proc/<pid>/maps
... /path/theodb_rs.so (deleted)
```

O `(deleted)` é a assinatura.

## O falso-verde que ele produz

O pior caso não é falhar — é **passar**. Um oráculo que valida um fix roda contra o código sem o fix e reporta
sucesso.

## Regra derivada

- Depois de `cargo pgrx install`, **reiniciar o postmaster** antes de qualquer verificação.
- O restart precisa **reinjetar** as variáveis de trace (ex.: `THEODB_ADMIT_TRACE`) e apontar o `PGDATA` certo —
  elas se perdem no restart.
- Confirmar por `md5sum` do `.so` **e** por `pg_postmaster_start_time()` posterior ao build.

## Relacionados

- [technique/proveniencia-em-todo-artefato](../techniques/proveniencia-em-todo-artefato.md)
