---
type: Measurement
title: m177 stress — o servidor de embeddings COLAPSA sob sobrecarga e retém 7 GB depois dela
description: Empurrado além da saturação, o throughput cai a 26% do pico com 19,7% de recusa de conexão, e a memória cresce 43× sem ser devolvida — um modo de falha que o teste de carga curta não revelava.
resource: benchmarks/artifacts/m177/stress.json
tags: [benchmark, m177, embedding, stress, colapso, memoria, thread-per-connection, modo-de-falha, honest-negative]
milestone: M177
generated: { by: claude-code/opus-5, at: 2026-08-08T00:15:00Z }
sources:
  - id: stress
    resource: benchmarks/artifacts/m177/stress.json
    title: Stress 8→128 clientes, 20 s por nível, com recuperação pós-pico
---

Os testes anteriores deste milestone mediram **carga curta até a saturação**. Este empurra **além** dela
e procura o modo de falha. O resultado muda o veredito operacional do pilar de embeddings: o servidor
**não degrada graciosamente — ele colapsa**, e não devolve a memória depois.

# O colapso

8 → 128 clientes, 20 s sustentados por nível, todo pedido contabilizado:

| clientes | throughput OK | pedidos | erros | **taxa de erro** | p50 | p99 | **RSS do servidor** |
|---|---|---|---|---|---|---|---|
| 8 | 62,3 rps | 1 252 | 0 | 0,0% | 125 ms | 208 ms | 1 125 MB |
| 32 | **65,0 rps** ← pico | 1 336 | 6 | 0,5% | 402 ms | 1 869 ms | 3 158 MB |
| 64 | **26,5 rps** | 652 | 24 | 3,7% | 1 091 ms | 11 623 ms | 5 081 MB |
| 128 | **17,1 rps** | 721 | 142 | **19,7%** | 1 801 ms | 19 727 ms | **6 932 MB** |
| *recuperação (1 cliente)* | 44,9 rps | 450 | 0 | 0,0% | **19 ms** | 58 ms | 6 932 MB |

**Isto é congestion collapse, não saturação.** Passado o pico em 32 clientes, **mais carga produz menos
trabalho**: o throughput cai para 26% do pico enquanto a p99 sobe de 1,9 s para 19,7 s. Um sistema
saturado entrega o mesmo sob mais carga; um sistema em colapso entrega menos.

**Todos os 172 erros são `conn_error`** — recusa ou reset de conexão, nenhum timeout, nenhum erro HTTP. O
servidor não está devolvendo erro: ele está deixando de aceitar conexão.

# A memória: 43× e sem devolução

**161 MB antes do teste → 6 932 MB depois.** Num host de 15,7 GB, isso deixou **599 MB livres** — o
processo ficou a um passo do OOM killer.

E o dado que transforma isto de "consumo alto" em **defeito**: depois do pico, com **um** cliente e p50
de volta a 19 ms, o RSS permanece em **6 932 MB**. A carga acabou, a latência normalizou, a memória não
voltou. Só foi liberada ao encerrar o processo.

**Mecanismo, verificado no processo vivo:** 33 threads residentes ao fim do teste. O
`ThreadingHTTPServer` do stdlib cria **uma thread por conexão**, sem limite, e o ONNX Runtime aloca
**arena de memória por thread** que executa o modelo. Concorrência ilimitada × arena por thread = a
curva acima. As arenas não são devolvidas quando as threads morrem.

# Por que a carga curta não viu isto

O teste anterior ([concorrência](/benchmarks/m177-embed-concurrency-verdict.md)) usava 10 pedidos por
cliente e parava em 16 clientes. Nesse regime o servidor parece sadio: satura em ~61 rps e a cauda
degrada de forma previsível. **Nenhuma das duas patologias aparece** — nem a inversão de throughput, nem
o crescimento de memória, porque nem a concorrência nem a duração eram suficientes.

É a diferença entre teste de carga e teste de stress, e vale registrar: um relatório baseado só no
primeiro teria declarado o componente pronto.

## O erro de medição que este script evita

Sob sobrecarga, medir apenas a latência dos pedidos **bem-sucedidos** produz um relatório enganoso: se
metade falha rápido, a latência das sobreviventes *melhora*. A 128 clientes, a p50 dos que passaram é
1 801 ms — parece ruim mas administrável, e esconde que **um em cada cinco pedidos nem foi atendido**.
Por isso a taxa de erro aparece ao lado de cada latência, e não numa nota de rodapé.

# O que isto significa para o produto

O pilar de embeddings tem **capacidade útil de ~32 clientes concorrentes** nesta configuração — não os
128 que um pool de conexões de aplicação pode facilmente produzir. Acima disso o sistema piora com mais
carga, o que é a forma mais perigosa de falha: a reação natural de quem opera (aumentar concorrência,
reiniciar, retentar) **acelera** o colapso.

**A correção não é otimização, é limite.** As técnicas conhecidas — nenhuma medida aqui:

| técnica | o que corrige |
|---|---|
| pool de workers de tamanho fixo, em vez de thread-por-conexão | ambas as patologias na raiz |
| semáforo limitando inferências concorrentes | a explosão de arenas |
| `listen` backlog explícito + rejeição rápida | troca `conn_error` silencioso por erro tipado |
| `intra_op_num_threads` / arena compartilhada do ONNX Runtime | o consumo por thread |

Todas contradizem o instinto de "aumentar a concorrência para escalar". O caminho é **limitar** a
concorrência e enfileirar — o oposto do que o número de 65 rps no pico sugere.

# Limites honestos

- **Uma máquina não dedicada** (12 cores, 15,7 GB, dez containers ativos), um modelo (384d, 213 MB), um
  servidor. A curva é desta configuração; a **forma** dela — inversão de throughput e retenção de
  memória — é da arquitetura thread-por-conexão, não da máquina.
- O host já estava com ~7 GB usados por outros processos quando o teste começou. **Parte da recusa de
  conexão a 128 clientes pode ser pressão de memória do host**, não só do servidor. Não foi isolado.
- **Não medido:** nenhuma das correções acima; comportamento com o modelo multilíngue de 1,7 GB (que
  agravaria a explosão de arena); e o efeito com PostgreSQL no laço.
- A recuperação foi medida a **1 cliente por 10 s** — suficiente para mostrar que a latência normaliza,
  insuficiente para afirmar que o servidor voltou ao estado íntegro.

# Relacionados

- A carga curta que não via isto: [concorrência e flamegraph](/benchmarks/m177-embed-concurrency-verdict.md)
- O teto do transporte, que este achado torna secundário: [camadas](/benchmarks/m177-camadas-python-http-verdict.md)
- Residência do modelo por processo: [fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md)
- O footgun de escala registrado no desenho: [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)
