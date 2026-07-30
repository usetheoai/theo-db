---
type: Index
title: Invariantes de plataforma
description: Índice dos conceitos do tipo `Invariant` deste bundle.
tags: [okf, indice]
timestamp: 2026-07-30T00:00:00Z
---

# Invariantes de plataforma

Verdades sobre PostgreSQL, pgrx, Rust, Arrow, git e a infraestrutura deste projeto que foram aprendidas **pelo
caminho caro**. Não são preferências; são propriedades do sistema que, ignoradas, produzem falha.

Leia antes de mexer em storage, FFI, recovery, build ou branch compartilhado.

| Conceito | O que é |
|---|---|
| [BackgroundWorker::transaction faz PushActiveSnapshot por todo o closure](bgworker-transaction-segura-snapshot.md) | Uma chamada HTTP dentro do closure segura backend_xmin pelo tempo inteiro da chamada, atrasando autovacuum. |
| [CHUNK_GROUP_ROWS = 10.000 é a unidade de decode, skip e memória do colunar](chunk-group-e-a-unidade-de-tudo.md) | Todo termo O(N) no colunar tem uma versão O(chunk-group); quando um caminho não tem, é defeito de escala esperando a escala. |
| [CustomScan com scanrelid=0 e Aggref no targetlist quebra sob subquery pullup](customscan-scanrelid-zero-e-aggref-pullup.md) | O pullup inlina o Aggref num nó superior e o planner falha com 'cache lookup failed for attribute N of relation 0' — crasha até o EXPLAIN. |
| [sum(Int64) do DataFusion faz add_wrapping — casar para Decimal128 antes de somar](datafusion-sum-int64-faz-wrapping.md) | Para saída numeric exata, o caminho é sum(cast(col AS Decimal128(38,0))) sobre i128; sum(Int64) silenciosamente dá a volta. |
| [durable_rename emite 4 fsyncs em ordem estrita e o do diretório-pai é o load-bearing](durable-rename-fsync-do-diretorio-pai.md) | Sem o fsync do diretório-pai o rename se perde; e durable_rename NÃO faz PANIC — repassa o elevel do caller. |
| [git switch e restore — nunca checkout, revert, reset --hard ou force-push](git-switch-nao-checkout.md) | Comandos ambíguos ou destrutivos são proibidos por regra do projeto; os substitutos preservam a capacidade de recuperar. |
| [Peers AGPL são estudo, nunca fonte de código](licenca-agpl-e-study-only.md) | A distribuição é Apache-2.0 com gate fail-closed contra AGPL; técnica se aprende, código se reimplementa do zero. |
| [A e2e-runner é o runner do CI — medir nela satura o pipeline e contamina o número](nao-usar-a-box-do-ci.md) | 165.227.121.20 hospeda o runner do GitHub Actions e k3s; usá-la para benchmark degrada o CI de todo o time e produz deriva. |
| [nohup ... & dentro de ssh não sobrevive ao fechamento do canal](nohup-em-ssh-nao-sobrevive.md) | Você acha que lançou o processo e não lançou. Exige script + setsid, com verificação de PID depois. |
| [pgrx 0.19 desenrola: um ERROR do PostgreSQL vira panic_any e as frames Rust desenrolam](panic-atraves-da-fronteira-c.md) | O pg_guard no bloco extern C-unwind embrulha cada função bindgen; check_for_interrupts! desenrola limpo, ao contrário do que uma revisão alegou. |
| [pgrx não gera o script de upgrade — e uma pg_extern nova não alcança catálogo existente](pgrx-nao-gera-script-de-upgrade.md) | Adicionar uma função depois que default_version foi congelada faz o símbolo existir no .so e não no catálogo; o erro só aparece em instalação pré-existente. |
| [Spi::get_one marca a transação como mutável](pgrx-spi-nao-e-read-only.md) | Apesar do nome, get_one impede operações que exigem transação read-only; Spi::connect + c.select é o caminho que não marca. |
| [Trocar o .so não afeta backends enquanto o postmaster não reinicia](so-obsoleto-sob-shared-preload.md) | Sob shared_preload_libraries o postmaster mapeia o .so no arranque; substituir o arquivo deixa /proc/PID/maps marcando '(deleted)' e os testes rodam contra o binário ANTIGO. |
| [O TableAmRoutine tem de ser alocado em TopMemoryContext](tableam-routine-em-topmemorycontext.md) | Alocar a routine handler num contexto de menor duração produz ponteiro pendente e segfault quando o contexto é resetado. |
| [UNLOGGED é truncada por crash recovery — sempre, sem aviso](unlogged-truncado-por-recovery.md) | Uma tabela UNLOGGED perde 100% do conteúdo quando o cluster reinicializa após crash. Se ela é a fonte de um A/B, o oráculo passa a comparar contra vazio. |
| [Um background worker roda em backend próprio e não enxerga SET de sessão](worker-nao-ve-set-de-sessao.md) | Configurar GUCs com SET numa sessão psql não afeta o worker; ele precisa de ALTER SYSTEM, e shared_preload_libraries exige restart, não reload. |
