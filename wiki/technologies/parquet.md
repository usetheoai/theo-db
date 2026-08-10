---
type: Technology
title: Apache Parquet
description: O formato colunar em arquivo que é o substrato do lakehouse do TheoDB — e a razão pela qual o ganho analítico da adoção externa aparecia sobre arquivos e não sobre o heap.
resource: https://parquet.apache.org/
tags: [tecnologia, formato, columnar, arquivo, lakehouse]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: parquet-site
    resource: https://parquet.apache.org/
    title: Apache Parquet, site oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O Parquet é um **formato colunar em arquivo**, com compressão por coluna e estatísticas por grupo de
linhas que permitem pular blocos sem descomprimir. É o formato de fato de data lakes, com licença
permissiva.[^recalled]

# Papel neste acervo

**É o substrato do [lakehouse](/features/15-lakehouse-parquet.md)** — ler, escrever e agregar Parquet
externo, hoje em código próprio.

E ele explica um dos resultados mais instrutivos da linhagem colunar: a adoção externa media ganho
**sobre Parquet (~9×)** e **honest-negative sobre o heap**
([m61](/benchmarks/m61-columnar-adoption.md)).

**A razão é do formato, não do motor:** um executor vetorizado precisa de dados **já dispostos por
coluna**. Ler linhas e transpô-las para colunas paga um custo que anula a vantagem. O ganho colunar não
vem do executor sozinho — vem do **par formato + executor**.

Foi esse achado que fixou o posicionamento: **analytics sobre arquivos, não acelerador transparente do
heap**.

# O parentesco com o formato interno

As estatísticas por bloco do Parquet têm a mesma função do **diretório de mínimo e máximo** do formato
colunar interno, que permite o [skip por zone-map](/benchmarks/columnar-zonemap-verdict.md) e o
[caminho rápido de extremos](/benchmarks/columnar-minmax-zonemap-verdict.md).

**A ideia é a mesma; a diferença é quem controla o storage** — e é por isso que o formato interno pôde
delegar visibilidade ao MVCC do Postgres, enquanto o Parquet externo é somente-leitura e datado, com a
freshness exposta explicitamente.

[^parquet-site]: Apache Parquet, site oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
