# Guias

* [Migrar do Pinecone para o TheoDB](migrate-from-pinecone.md) - Traz vetores e metadados para uma tabela PostgreSQL comum; a escolha entre função atômica e procedure com commit por lote é a decisão operacional que importa.
* [Migração mínima — PostgreSQL vanilla para TheoDB](minimal-migration.md) - Usa pg_dump e pg_restore padrão, sem ferramenta especial, com um checksum de linha inteira como oráculo de integridade e flags que fazem a restauração falhar em vez de restaurar parcialmente.
* [Migrar do pgvector para o tipo vector próprio](pgvector-migration.md) - Playbook de janela de manutenção para bancos existentes; o cast binário grátis não se aplica porque os dois tipos ocupam o mesmo nome e não coexistem.
* [Quickstart — todas as capacidades por um CREATE EXTENSION](quickstart.md) - Do container à superfície completa com uma única extensão; inclui a query unificada que é o diferencial do produto, e uma nota de drift sobre trechos que envelheceram.
* [Self-host — subir o TheoDB com a superfície AI-native](self-host-quickstart.md) - Receita de self-host com vectorizer e busca híbrida, incluindo os três erros de configuração que travam quem faz isso pela primeira vez.
* [Funções generativas em SQL — contratos, garantias e segurança](sql-ai-functions.md) - O documento operacional da superfície ai.*, com os erros tipados que cada função emite, a postura de segurança e os limites declarados de cada forma.
* [Embeddings a partir do SQL (theodb.embed)](sql-embeddings.md) - Gera vetores direto do SQL chamando um endpoint configurável — o banco não embarca modelo, o que mantém a imagem enxuta e o modelo trocável.
* [Unificação — um sistema contra dois](unification-1-vs-2-systems.md) - Compara simplicidade operacional e consistência de dados entre fazer a busca filtrada e aumentada por IA numa SQL transacional ou colando dois sistemas na aplicação. Não é comparação de velocidade.
