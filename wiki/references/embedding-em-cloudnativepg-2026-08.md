---
type: Reference
title: Embeddings locais sob CloudNativePG — o que a plataforma de destino impõe
description: O alvo de deploy é um operador Kubernetes, e isso torna a rota do modelo embarcado pior e a do serviço separado natural — por mecanismo, não por preferência.
resource: https://cloudnative-pg.io/docs/1.29/imagevolume_extensions/
tags: [referencia, cloudnativepg, kubernetes, deploy, extensao, embedding, image-volume, restricao-de-plataforma]
generated: { by: claude-code/opus-5, at: 2026-08-08T00:50:00Z }
sources:
  - id: cnpgimgvol
    resource: https://cloudnative-pg.io/docs/1.29/imagevolume_extensions/
    title: Image Volume Extensions — CloudNativePG 1.29
  - id: cnpgimg
    resource: https://cloudnative-pg.io/blog/creating-container-images/
    title: Creating a custom container image for CloudNativePG
  - id: cnpgext
    resource: https://github.com/cloudnative-pg/postgres-extensions-containers
    title: cloudnative-pg/postgres-extensions-containers — imagens de extensão da comunidade
---

Registro de uma restrição de plataforma informada pelo owner em 2026-08-08: **o banco vai rodar sob
[CloudNativePG](https://cloudnative-pg.io/)**, o operador Kubernetes de PostgreSQL. Isso não muda nenhum
número medido no [M177](/benchmarks/m177-hop-vs-residencia-verdict.md), mas muda **o que os números
significam** para a decisão de arquitetura.

**Isto é prior art e restrição de plataforma — não é evidência medida.** Nada aqui foi executado contra
um cluster; é leitura da documentação do operador. Uma validação real exige subir um `Cluster` e medir.

# Como o CNPG entrega extensões (1.29+)

Extensões modernas não são "baked" na imagem do PostgreSQL: entram pelo stanza
`.spec.postgresql.extensions` como **image volumes**, montados **read-only** em
`/extensions/<NOME>`, com o operador ajustando `extension_control_path` e `dynamic_library_path`
automaticamente. A comunidade mantém imagens prontas (pgvector, PostGIS) nesse formato.

Duas consequências operacionais documentadas:

- **Adicionar, remover ou atualizar uma imagem de extensão dispara rolling update** dos pods do
  PostgreSQL — é como image volume funciona no Kubernetes.
- O layout esperado é `/share/extension/` para control files e `/lib/` para bibliotecas. **A
  documentação não descreve suporte a arquivos arbitrários** — o que pesos de modelo seriam.

# O que isso faz com as duas rotas do M177

| | **modelo embarcado na extensão** | **serviço de embedding separado** |
|---|---|---|
| onde vive | image volume montado no pod do Postgres | `Deployment` + `Service` próprios |
| memória | soma ao pod do Postgres, **contra o `limits.memory` do pod** | limite próprio, isolado |
| falha por memória | **OOMKill do pod do banco** — não swap | OOMKill só do serviço |
| escalar | replicar o **banco inteiro** | replicar só o serviço (`replicas: N`) |
| trocar de modelo | nova imagem de extensão → **rolling update do banco** | `kubectl set image` no Deployment |
| pesos de centenas de MB | fora do layout esperado de extensão | imagem comum, sem restrição |

**A rota embarcada piora sob CNPG, e por mecanismo:** em Kubernetes o limite de memória do pod é rígido.
Um pod que ultrapassa `limits.memory` é **morto**, não paginado. O M177 mediu que um modelo multilíngue
custa [1,7 GB de RSS por processo](/benchmarks/m177-hop-vs-residencia-verdict.md); somar isso ao pod do
banco significa dimensionar cada réplica do PostgreSQL para carregar o modelo — e um pico de
concorrência que estoure o limite derruba **o banco**, não o embedding.

**A rota do serviço separado é o padrão nativo da plataforma.** É um Deployment como qualquer outro, e o
teto de ~195 rps medido por instância deixa de ser um limite do produto para virar um parâmetro de
`replicas` — o eixo que Kubernetes resolve melhor.

# Onde o número medido encontra a plataforma

O [stress em CPU dedicada](/benchmarks/m177-stress-colapso-verdict.md) mostrou throughput **plano em
~195 rps** de 8 a 128 clientes, com 13% de recusa de conexão no topo. Sob CNPG isso se traduz em algo
acionável: **capacidade por réplica conhecida**, escalada horizontalmente pelo operador, com o limite de
aceitação de conexão virando o sinal para adicionar réplica — exatamente o tipo de métrica que um HPA
consome.

# O que continua não sabido

- **Se a extensão do TheoDB já roda sob CNPG.** O `theodb_rs` é um cdylib pgrx; a documentação de image
  volume descreve o layout, mas **nada foi testado** contra um cluster real.
- Se o CNPG permite sidecar arbitrário no pod do banco (versões recentes suportam patch de `podSpec`,
  mas não verificado) — relevante caso alguém queira o modelo *ao lado* do banco em vez de num Deployment.
- Se pesos de modelo cabem num image volume de extensão sem violar o contrato do operador.
- O custo de rolling update do banco a cada troca de modelo, em tempo real de indisponibilidade.

# Relacionados

- Residência do modelo por processo, o número que a plataforma amplifica: [m177 fase 1](/benchmarks/m177-hop-vs-residencia-verdict.md)
- Capacidade por instância em CPU dedicada: [m177 stress](/benchmarks/m177-stress-colapso-verdict.md)
- Prior art da extensão de embedding: [prior art](/references/embedding-local-como-extensao-2026-08.md)
- O desenho atual: [embeddings em SQL](/guides/sql-embeddings.md)
