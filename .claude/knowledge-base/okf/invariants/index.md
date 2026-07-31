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
| [O teto de escala costuma ser o BUILD, não a query — o ambuild picava ~4× o dataset base](build-pica-4x-o-dataset-base.md) | Dimensionar a box pelo tamanho do índice é dimensionar pelo número errado: 30M OOMou a 64,7 GB num box de 62 GB usáveis enquanto o índice final tinha 15 GB. |
| [CHUNK_GROUP_ROWS = 10.000 é a unidade de decode, skip e memória do colunar](chunk-group-e-a-unidade-de-tudo.md) | Todo termo O(N) no colunar tem uma versão O(chunk-group); quando um caminho não tem, é defeito de escala esperando a escala. |
| [O PostgreSQL dobra expressões constantes no planejamento — `CASE ... ELSE 1/0` erra SEMPRE](constant-folding-avalia-o-ramo-nao-tomado.md) | Um gate escrito como CASE com divisão por zero constante dispara mesmo quando o ramo não é tomado, e o sintoma se lê como "o fix não funcionou". |
| [`cp` sobre um `.so` MAPEADO troca as páginas sob os processos vivos — e o postmaster não re-executa, então ele entra em loop de crash](cp-sobre-so-mapeado-derruba-o-servidor.md) | Medido 2026-07-31: um `cp` do .so novo sobre o instalado matou o vectorizer worker com signal 11 e derrubou o cluster. `restart` não resolve — só `stop` + `start`. |
| [CustomScan com scanrelid=0 e Aggref no targetlist quebra sob subquery pullup](customscan-scanrelid-zero-e-aggref-pullup.md) | O pullup inlina o Aggref num nó superior e o planner falha com 'cache lookup failed for attribute N of relation 0' — crasha até o EXPLAIN. |
| [sum(Int64) do DataFusion faz add_wrapping — casar para Decimal128 antes de somar](datafusion-sum-int64-faz-wrapping.md) | Para saída numeric exata, o caminho é sum(cast(col AS Decimal128(38,0))) sobre i128; sum(Int64) silenciosamente dá a volta. |
| [`pg_total_relation_size` pega lock de relação — sob DDL exclusivo ele BLOQUEIA, e o monitor silencioso parece "nada acontecendo"](monitorar-tamanho-de-relacao-bloqueia-sob-ddl.md) | Medido 2026-07-31: a query de acompanhamento de um ALTER TABLE ... SET LOGGED ficou 163 s em Lock/relation. A saída vazia lê como ausência de atividade, que é o oposto da verdade. |
| [`pgrep -f <padrão>` casa com a linha de comando do PRÓPRIO watcher — o laço de espera nunca termina](pgrep-f-casa-com-o-proprio-watcher.md) | `until ! pgrep -f "cargo build"; do sleep 20; done` roda para sempre, porque o shell que executa o laço tem "cargo build" no próprio argv. O build terminou em 2m11s e o watcher continuou reportando RODANDO. |
| [Ler um arquivo exige o bit x em TODO diretório do caminho — e o erro acusa o arquivo, não o diretório](ler-arquivo-exige-x-em-todo-o-caminho.md) | Um TSV de 70 GB em 644 ficou inalcançável porque o pai era /root em 700. O erro é "Permission denied" sobre o ARQUIVO, e quem lê o `ls -l` do arquivo conclui que a permissão está certa. |
| [Dois parsers da mesma string discordam — e a divergência vira a vulnerabilidade](dois-parsers-da-mesma-string-discordam.md) | endpoint_host validava a URL segundo a RFC; o cliente HTTP não implementa userinfo e caía para a porta 80, então http://169.254.169.254:x@api.openai.com ia para o metadata service. |
| [durable_rename emite 4 fsyncs em ordem estrita e o do diretório-pai é o load-bearing](durable-rename-fsync-do-diretorio-pai.md) | Sem o fsync do diretório-pai o rename se perde; e durable_rename NÃO faz PANIC — repassa o elevel do caller. |
| [git switch e restore — nunca checkout, revert, reset --hard ou force-push](git-switch-nao-checkout.md) | Comandos ambíguos ou destrutivos são proibidos por regra do projeto; os substitutos preservam a capacidade de recuperar. |
| [Quando a granularidade do relógio é maior que a distância entre dois eventos, o teste fica flaky](granularidade-do-relogio-menor-que-o-evento.md) | O smoke de PITR capturava o alvo no MESMO segundo do stop do backup; pgbackrest --type=time compara com estritamente-menor, então o restore falhava de forma intermitente. |
| [Peers AGPL são estudo, nunca fonte de código](licenca-agpl-e-study-only.md) | A distribuição é Apache-2.0 com gate fail-closed contra AGPL; técnica se aprende, código se reimplementa do zero. |
| [maintenance_work_mem não limita o pico de RSS quando o trabalho é feito em Rust](maintenance-work-mem-nao-capa-rss-de-rust.md) | O malloc do Rust acontece FORA dos memory contexts do PostgreSQL, então o knob não capa nada — o working set precisa ser medido, não presumido. |
| [A e2e-runner é o runner do CI — medir nela satura o pipeline e contamina o número](nao-usar-a-box-do-ci.md) | 165.227.121.20 hospeda o runner do GitHub Actions e k3s; usá-la para benchmark degrada o CI de todo o time e produz deriva. |
| [nohup ... & dentro de ssh não sobrevive ao fechamento do canal](nohup-em-ssh-nao-sobrevive.md) | Você acha que lançou o processo e não lançou. Exige script + setsid, com verificação de PID depois. |
| [pgrx 0.19 desenrola: um ERROR do PostgreSQL vira panic_any e as frames Rust desenrolam](panic-atraves-da-fronteira-c.md) | O pg_guard no bloco extern C-unwind embrulha cada função bindgen; check_for_interrupts! desenrola limpo, ao contrário do que uma revisão alegou. |
| [PG18 renomeou TupleDesc->attrs para compact_attrs, e o código antigo COMPILA lendo a struct errada](pg18-compact-attrs-rename-silencioso.md) | Os dois arrays coexistem e ambos são __IncompleteArrayField, então o compilador aceita — e passa a ler offsets de uma struct de 104 B sobre um array de 16 B. |
| [pgrx não gera o script de upgrade — e uma pg_extern nova não alcança catálogo existente](pgrx-nao-gera-script-de-upgrade.md) | Adicionar uma função depois que default_version foi congelada faz o símbolo existir no .so e não no catálogo; o erro só aparece em instalação pré-existente. |
| [Spi::get_one marca a transação como mutável](pgrx-spi-nao-e-read-only.md) | Apesar do nome, get_one impede operações que exigem transação read-only; Spi::connect + c.select é o caminho que não marca. |
| [SET de uma GUC inexistente no namespace de uma extensão SUCEDE — como placeholder silencioso](set-de-guc-inexistente-sucede-como-placeholder.md) | O comando devolve SET, o valor é lembrado e nada acontece; pg_settings é o único discriminador entre GUC real e placeholder. |
| [Trocar o .so não afeta backends enquanto o postmaster não reinicia](so-obsoleto-sob-shared-preload.md) | Sob shared_preload_libraries o postmaster mapeia o .so no arranque; substituir o arquivo deixa /proc/PID/maps marcando '(deleted)' e os testes rodam contra o binário ANTIGO. |
| [Um callback extern C-unwind gerado por macro_rules! sem frame de guarda derruba a instância inteira](stub-extern-c-sem-guarda-derruba-o-servidor.md) | Sem #[pg_guard] o unwinder sai da pilha (_URC_END_OF_STACK, signal 6) e mata o postmaster — e o atributo NÃO pode ser aplicado dentro da macro. |
| [O TableAmRoutine tem de ser alocado em TopMemoryContext](tableam-routine-em-topmemorycontext.md) | Alocar a routine handler num contexto de menor duração produz ponteiro pendente e segfault quando o contexto é resetado. |
| [UNLOGGED é truncada por crash recovery — sempre, sem aviso](unlogged-truncado-por-recovery.md) | Uma tabela UNLOGGED perde 100% do conteúdo quando o cluster reinicializa após crash. Se ela é a fonte de um A/B, o oráculo passa a comparar contra vazio. |
| [O VFD do PostgreSQL pode segurar até `max_files_per_process` (1000) dentro de um soft limit de 1024](vfd-do-pg-consome-o-orcamento-de-descritores.md) | Uma lib embarcada que abre arquivos fora do gerenciador do PG começa com folga quase nula: o spill do DataFusion falhou em `File::create` com 205 GB livres em disco, e chega como `Execution` — não `ResourcesExhausted`, não `IoError`. |
| [O VmRSS de um backend PostgreSQL inclui os shared_buffers que ele tocou — meça RssAnon](vmrss-de-backend-pg-inclui-shared-buffers.md) | Um backend varrendo uma tabela grande vê o RSS subir até o tamanho de shared_buffers sem alocar nada próprio; ler isso como crescimento de memória do código é o erro. |
| [Um background worker roda em backend próprio e não enxerga SET de sessão](worker-nao-ve-set-de-sessao.md) | Configurar GUCs com SET numa sessão psql não afeta o worker; ele precisa de ALTER SYSTEM, e shared_preload_libraries exige restart, não reload. |
