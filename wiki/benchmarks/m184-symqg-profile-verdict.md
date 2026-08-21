---
type: Measurement
title: m184 — por que o SymQG é mais lento: o custo é de CPU no nosso código, não do imposto do PostgreSQL
description: O perfil mostra o SymQG gastando mais tempo em userspace e menos no kernel que o HNSW, o que contradiz a hipótese registrada de que o ganho in-memory não sobreviveu à página, ao WAL e ao MVCC.
resource: git:7cd157d^:benchmarks/artifacts/m184/symqg-profile.json
tags: [benchmark, m184, symqg, perf, profiling, mecanismo, hipotese-contradita]
milestone: M184
generated: { by: claude-code/opus-5, at: 2026-08-08T07:00:00Z }
sources:
  - id: prof
    resource: git:7cd157d^:benchmarks/artifacts/m184/symqg-profile.json
    title: perf record durante CREATE INDEX de cada access method, CPU dedicada
---

Os três artefatos do e2 mediram **que** o SymQG é mais lento. **Nenhum mediu por quê** — verificado por
grep: as palavras *mecanismo*, *gargalo*, *perf* e *profil* não aparecem em nenhum deles. Este artefato
responde a pergunta que ficou.

# O perfil

`perf record -F 199 -a -g` durante o `CREATE INDEX` de cada access method, mesmos 20 000 vetores 128d,
CPU dedicada:

| objeto | **SymQG** | HNSW |
|---|---|---|
| `theodb_rs.so` (nosso código) | **76,54%** | 65,96% |
| kernel | **18,73%** | 27,94% |
| libc | 4,26% | 5,61% |

# A hipótese registrada, contradita

O conceito da [feature](/features/17-indice-symqg.md) explica o resultado do e2 assim:

> *"ganho medido in-memory frequentemente não sobrevive ao **imposto de página, WAL e MVCC**"*

Se fosse esse o mecanismo, o SymQG passaria **mais** tempo no kernel — página, WAL e MVCC são syscalls,
I/O e locks. **Ele passa menos: 18,7% contra 27,9% do HNSW.**

E os símbolos de kernel confirmam: em ambos os builds o topo é **escalonador e contadores**
(`native_write_msr`, `psi_group_change`, `__update_load_avg_se`, `dequeue_entity`), nenhum acima de
0,6%. **Nenhum símbolo de I/O, de página ou de WAL aparece.**

**O SymQG é compute-bound.** O custo está no nosso próprio código, executando mais instruções para
construir o mesmo índice — não no imposto que o PostgreSQL cobra por estar dentro dele.

# Por que isso muda a decisão do M176

A leitura anterior — "o ganho não sobreviveu ao ambiente do banco" — sugere que o problema é
**estrutural e caro de atacar**: seria preciso mudar o layout de página, o caminho de WAL, a disciplina
de MVCC. Uma promoção exigiria reabrir decisões grandes.

A leitura medida é outra: **o problema é algorítmico e mora no nosso código**. Isso não torna o SymQG
promovível — ele continua 3,5× mais lento no build e 2,6–3,9× na busca —, mas muda a natureza do
trabalho que uma promoção exigiria, e **torna o problema investigável com as ferramentas que já temos**.

Não altera a recomendação de tirá-lo da superfície default; altera o que se diria sobre ele ao fazê-lo.

# Limite honesto — e ele é grande

**Os símbolos do `theodb_rs.so` não resolvem.** O binário de release não carrega debuginfo, então o
perfil atribui 76,54% a um objeto, não a uma função. **Sei em qual código o tempo está, não em qual
linha.**

Fechar isso exige rodar com build de debug ou com `debug = true` no perfil de release — trabalho de
build, não de medição, e fora do escopo deste artefato. **Sem isso, nenhuma otimização específica pode
ser proposta a partir daqui**: o achado é sobre *onde não está* (o kernel) tanto quanto sobre onde está.

Outros limites: um dataset (20k × 128d), um regime (build, não busca), e a comparação é entre dois
access methods no mesmo binário — o que é justo para atribuição, e não diz nada sobre escala.

# Relacionados

- O veredito que mediu o resultado sem o mecanismo: [e2](/benchmarks/e2-symqg-inpg-verdict.md)
- A feature e a hipótese que este perfil contradiz: [SymQG](/features/17-indice-symqg.md)
- A superfície e o build medidos: [m184](/benchmarks/m184-pilares-superficie-medida-verdict.md)
- O padrão que a hipótese invocava: [ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md)
