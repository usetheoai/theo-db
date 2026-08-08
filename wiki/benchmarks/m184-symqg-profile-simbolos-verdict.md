---
type: Measurement
title: m184 — o perfil do SymQG com símbolos: 39% do build está no próprio ambuild, e a distância de kernel some
description: Resolver os símbolos mostrou que o SymQG gasta 39% do build numa função que o HNSW não tem análoga, enquanto ambos compartilham o mesmo kernel SIMD — e revelou que o artefato anterior falhou por resolução de caminho, não por falta de debuginfo.
tags: [benchmark, m184, symqg, perf, simbolos, flamegraph, mecanismo, correcao-de-metodo]
milestone: M184
resource: benchmarks/artifacts/m184/symqg-profile-symbols.json
generated: { by: claude-code/opus-5, at: 2026-08-08T08:00:00Z }
sources:
  - id: sym
    resource: benchmarks/artifacts/m184/symqg-profile-symbols.json
    title: perf com símbolos resolvidos, build de cada access method, CPU dedicada
---

Fecha a lacuna que o [perfil anterior](/benchmarks/m184-symqg-profile-verdict.md) declarou como seu maior
limite: *"os símbolos do `theodb_rs.so` não resolvem… nenhuma otimização específica pode ser proposta a
partir daqui"*.

# A correção de método, que vale por si

O artefato anterior atribuiu a falha a **release sem debuginfo**. Verificado: **falso.**

```
nm theodb_rs.so | wc -l   →  86.191 símbolos estáticos, com nomes Rust legíveis
readelf -S                →  .symtab presente
```

O `.so` **nunca foi stripped**. O `perf` do host falhava por **resolução de caminho**: o backend roda no
namespace do container, e o host não tinha o `.so` no caminho que o mapa de memória aponta. A correção é
de execução, não de build:

```
docker run --pid=host …                                    # PIDs iguais dentro e fora
docker cp tdb:/usr/lib/postgresql/18/lib/theodb_rs.so \
          /usr/lib/postgresql/18/lib/theodb_rs.so          # espelha no MESMO caminho
```

Concluir "precisa rebuildar com debug" teria custado um build inteiro para resolver um `docker cp`.

# O perfil, agora por função

Mesmos 20 000 vetores 128d, CPU dedicada, `perf record -F 199 -a -g`:

| **SymQG** | | **HNSW** | |
|---|---|---|---|
| `am::build::ambuild_symqg` | **39,27%** | — | *(sem análogo)* |
| `vec::simd_x86::l2_sq` | 23,08% | `vec::simd_x86::l2_sq` | **36,39%** |
| `vec::l2_dist_from_bytes` | 8,46% | `vec::l2_dist_from_bytes` | 15,31% |
| `ann::hnsw_parallel::select_from` | 2,31% | `ann::hnsw_parallel::select_from` | 3,55% |
| `vec::rabitq::…::rotate` | 1,16% | `hnsw_parallel::build_parallel` | 3,93% |
| | | `RwLock::read_contended` | 2,89% |

# O que isto diz

**O gargalo tem nome: `ambuild_symqg`, 39,27% do build.** É a função de construção do próprio access
method, e o HNSW **não tem análoga no perfil** — o build dele se distribui entre o kernel de distância e
o paralelismo, sem uma função de orquestração dominante.

**Os dois compartilham o mesmo kernel SIMD.** `l2_sq` e `l2_dist_from_bytes` são as mesmas funções nos
dois caminhos — 31,5% no SymQG contra 51,7% no HNSW. **O SymQG não é mais lento por calcular distância
pior; ele é mais lento por gastar 39% em outra coisa.**

E há um detalhe que merece nota: `RabitqQuantizer::rotate` aparece com 1,16% no SymQG e **não aparece no
HNSW** — a rotação do quantizador é custo exclusivo do caminho quantizado, coerente com o desenho.

O `RwLock::read_contended` aparece nos dois (2,89% no HNSW, 1,95% no SymQG), o que indica contenção de
lock no build paralelo — **compartilhada**, não específica do SymQG.

# Confirma o artefato anterior e o torna acionável

O perfil por objeto já dizia *compute-bound, no nosso código*. Agora sabe-se **qual código**: uma única
função de build, que é onde uma investigação de otimização começaria.

**Isto continua não tornando o SymQG promovível** — ele segue 3,5× mais lento no build e 2,6–3,9× na
busca. Mas a decisão do M176 muda de "problema estrutural do ambiente" para **"39% do custo está numa
função nomeada"**, o que é um alvo, não um muro.

# Limites honestos

- **Um regime: o build.** A busca — onde os 2,6–3,9× do [e2](/benchmarks/e2-symqg-inpg-verdict.md)
  foram medidos — **não foi perfilada**. O gargalo da busca pode ser outro, e não há dado sobre ele.
- **Um dataset** (20k × 128d), uma máquina, uma coleta por índice. Sem repetição, sem intervalo.
- **`ambuild_symqg` aparece dentro de `pgrx run_guarded`** — o wrapper que contém pânico na fronteira
  FFI. Os 39,27% incluem tudo o que a função chama, então é custo **inclusivo**: diz onde entrar, não o
  que exatamente pesa lá dentro.

# Relacionados

- O perfil por objeto que este detalha: [perfil SymQG](/benchmarks/m184-symqg-profile-verdict.md)
- O veredito de lentidão sem mecanismo: [e2](/benchmarks/e2-symqg-inpg-verdict.md)
- A decisão que isto informa: M176 no `ROADMAP.md`
