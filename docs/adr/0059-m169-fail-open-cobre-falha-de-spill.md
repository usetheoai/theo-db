# ADR-0059 — O fail-open do agregado cobre também a falha de SPILL, e o que isso custa

- **Status:** ACCEPTED
- **Data:** 2026-07-31
- **Milestone:** M169
- **Emenda a:** ADR-5 do plano `m169-scale-bugs-100m` ("O fail-open do agregado é CONDICIONAL, com pré-check
  exato; não é o do top-k copiado")

## Contexto

O ADR-5 autorizou um fail-open **condicional** no caminho agregado: quando o braço streaming falha por
`ResourcesExhausted` **e** o pré-check de `varlena_bytes < i32::MAX` garante que o caminho eager *pode* ter
sucesso, a consulta recua para o eager em vez de virar erro duro.

A corrida completa das 43 consultas do ClickBench a 100M (T4.1) mediu uma **regressão** que o ADR-5 não previu:
`q08` e `q09` (`COUNT(DISTINCT UserID) … GROUP BY RegionID`) saíram de `ok` para `error:XX000`.

A causa não é defeito novo. No caminho eager a pool do DataFusion é dimensionada pelo **batch decodificado**
(`max(work_mem, batch_bytes*2) + 64MB`, `df_executor.rs:798`); a 100M isso dava ~2,5 GB e o `count_distinct`
nunca derramava. O streaming removeu o batch O(N) — que é o objetivo declarado do milestone — e **com ele foi
embora a pool generosa, que era seu efeito colateral**. O agregado passou a derramar, e o derrame falhou:
`max_files_per_process = 1000` dentro de um `ulimit -n` de 1024 deixa folga quase nula para arquivos abertos
fora do gerenciador de VFD do PostgreSQL. Havia 205 GB de disco livres — não é disco, não é memória, é
descritor.

A rede do ADR-5 não pegou porque a falha chega como `DataFusionError::Execution`, não `ResourcesExhausted`
(`datafusion-physical-plan-54.0.0/src/spill/mod.rs:311` embrulha o `File::create` em `exec_datafusion_err!`).

## Decisão

Ampliar a classe do fail-open para incluir a falha de criação de arquivo de spill, mantendo intacto o pré-check
de offsets. O predicado virou uma função pura testável, `stream_failure_is_fail_open`.

Erro genuíno do braço streaming **NÃO** recua: recuar nele daria a resposta certa pelo caminho errado e
esconderia defeito nosso — travado pelo teste `unrelated_execution_error_does_not_fail_open`.

## Alternativas rejeitadas

| Alternativa | Por que não |
|---|---|
| **Desabilitar o `DiskManager`** do runtime do agregado | A pressão passaria a sair como `ResourcesExhausted`, que o predicado ANTIGO já casava — solução tipada e elegante. Mas mata o spill, e é o spill que faz a **q32 completar** (295,6 s medidos). Trocaria duas consultas por uma. |
| **Restaurar o piso `max(work_mem, batch*2)` na pool do streaming** | Reintroduz exatamente a acoplagem pool↔batch que o milestone remove. |
| **Casar `Execution` inteiro** | Recuaria em erro genuíno, mascarando defeito do streaming. |
| **Subir o `ulimit -n` da box** | É configuração de ambiente compensando código que passou a criar arquivos temporários que o PG não contabiliza, não limita por `temp_file_limit` e não limpa no crash. Trataria o sintoma na máquina em vez do contrato no código. **Ver "Consequência aberta" abaixo.** |

## Consequências — e a que dói

**q08/q09 voltam a completar, mas pelo caminho eager**, ou seja com o consumo O(N) que este milestone existe
para remover. Provado, não suposto: duas linhas de `theodb_agg_stream_fallback` no log do servidor, uma por
consulta, cada uma com `Os { code: 24, "Too many open files" }`. Uma ocorrência por consulta (e não duas)
significa que o eager **não** falhou também.

Isso é honestamente **pior** do que o ADR-5 previa. O pré-check que autoriza o recuo cobre o teto de offsets e
**apenas ele** — o próprio código declara isso (`df_executor.rs:657-661`: *"a wide fixed-width relation can
exhaust memory with `varlena_bytes = 0`"*). Para q08/q09 o pré-check passa trivialmente (não há coluna de
texto) e o recuo re-materializa ~2,5 GB.

**Consequência aberta, registrada e não resolvida:** não foi medido se q08/q09 passariam **pelo streaming** com
um orçamento de descritores maior. Se passassem, a classe ampliada viraria defesa em profundidade em vez de ser
a razão pela qual duas consultas completam — e essa distinção pertence ao artefato. Fica como trabalho
declarado, não como pendência esquecida.

**Concorrência:** a medição é de uma corrida serial, com uma conexão. N backends recuando ao mesmo tempo pagam
N × O(N). Não medido.

## Referências

- `theodb_rs/src/am/df_executor.rs:766-795` (marcador + predicado), `:700-721` (o ramo)
- `.claude/knowledge-base/okf/invariants/vfd-do-pg-consome-o-orcamento-de-descritores.md`
- `.claude/knowledge-base/okf/measurements/delta-medido-m169-28-para-30.md`
- `docs/benchmarks/m169-t41-delta.md`
