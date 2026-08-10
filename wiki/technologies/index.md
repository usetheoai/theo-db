# Tecnologias e conceitos

* [AlloyDB](alloydb.md) - O banco compatível com PostgreSQL do Google Cloud que serve de âncora SOTA do TheoDB — o alvo declarado, e a referência contra a qual as divergências são justificadas.
* [Apache Arrow](arrow.md) - O formato colunar em memória que é a linguagem comum entre o PostgreSQL e o executor analítico — e a fronteira onde as conversões de tipo precisam ser exatas.
* [BEIR](beir.md) - O conjunto de benchmarks de recuperação zero-shot que o projeto usa como metodologia; sua lição central é que resultado de recuperação é dependente de corpus.
* [BM25](bm25.md) - A função de ranqueamento lexical padrão da recuperação de informação; sua peça SOTA no PostgreSQL é AGPL, e essa restrição moldou anos de decisão no projeto.
* [DataFusion](datafusion.md) - O motor de query vetorizado em Rust sobre Arrow; é o executor analítico do TheoDB e a peça que tornou possível remover a dependência C++.
* [DiskANN](diskann.md) - A família de índices ANN projetada para grafos residentes em disco em escala de bilhão; foi o substituto permissivo de qualidade-ScaNN do projeto, com envelope de projeto declarado.
* [Go](go.md) - A linguagem designada para a camada de produto e operação no mandato de código próprio — e deliberadamente NÃO para extensões in-engine.
* [HNSW](hnsw.md) - O grafo navegável de pequeno mundo hierárquico — o algoritmo ANN mais usado da indústria, e o índice vetorial default do TheoDB, escolhido por evidência.
* [Apache Parquet](parquet.md) - O formato colunar em arquivo que é o substrato do lakehouse do TheoDB — e a razão pela qual o ganho analítico da adoção externa aparecia sobre arquivos e não sobre o heap.
* [pg_duckdb](pg-duckdb.md) - A extensão que embutia o motor analítico DuckDB no PostgreSQL; foi o pilar colunar do projeto por um tempo, e virou o último componente C++ a ser removido.
* [pgrx](pgrx.md) - O framework que permite escrever extensões PostgreSQL em Rust; é o que torna viável o mandato de código próprio, e a fonte da maior parte das restrições técnicas do projeto.
* [pgvector](pgvector.md) - A extensão vetorial de referência do ecossistema PostgreSQL — foi a base do TheoDB, depois o baseline de paridade, e por fim foi removida e substituída por código próprio.
* [pgvectorscale](pgvectorscale.md) - A extensão que trouxe StreamingDiskANN e quantização binária ao PostgreSQL; foi o substituto permissivo de qualidade-ScaNN do projeto até ser removida junto com o pgvector.
* [RaBitQ](rabitq.md) - O quantizador vetorial permissivo do estado da arte — 1 bit, sem treino de codebook, com erro provado; e a alavanca que o projeto mediu como ganho de memória, não de QPS.
* [Reciprocal Rank Fusion (RRF)](rrf.md) - A técnica que funde rankings de recuperadores diferentes usando posições em vez de scores — o que a torna robusta a escalas incomparáveis, e explica por que melhorar uma perna nem sempre melhora a fusão.
* [ScaNN](scann.md) - A biblioteca de busca vetorial aproximada do Google, cujo algoritmo está sob o índice vetorial do AlloyDB — e cujo gap de QPS o projeto perseguiu e mediu como intransponível.
* [Tantivy](tantivy.md) - O motor de busca full-text em Rust que é a base do BM25 próprio; sua abstração de storage permitiu persistir o índice no heap do PostgreSQL em vez de escrever páginas.
