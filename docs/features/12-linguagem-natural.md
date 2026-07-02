# Consultas em linguagem natural (`theodb_ai_nl`)

> **Status:** ✅ **Entregue (M19, superfície M7-S4).** A função `ai.nl_to_sql(question text, allowed_relations
> text[], model text DEFAULT NULL) RETURNS text` (`theodb_rs/src/api.rs:372`, implementada em
> `theodb_rs/src/nl.rs:31` `nl_to_sql`) gera SQL a partir de linguagem natural restrito a um allow-list de
> relações, via LLM, com validação de segurança. Provado por `benchmarks/tests/test_nl_sql.py`. A guarda de
> segurança (NL→SQL só-leitura, allow-list, anti-prompt-injection) é revisada pelo agente `council-security` —
> ver blueprint `.claude/knowledge-base/discoveries/blueprints/m7-nl-to-sql-safe-blueprint.md`. **Nota de
> honestidade:** a qualidade da geração depende do modelo LLM (modelo síncrono por-linha, ADR
> `docs/adr/0007-synchronous-per-row-model-http.md`); não há benchmark de acurácia NL→SQL (ex.: Spider/BIRD)
> publicado.

> **Superfície implementada (M7-S4):** o MVP seguro está entregue como `ai.nl_to_sql` (gera+valida) e
> `ai.nl_query` (executa em sandbox read-only) — `sql/60-theodb-nl.sql`. A guarda anti-prompt-injection é o
> gate: defesa em 4 camadas (prompt + validação estática + **sandbox read-only nativo `25006`** + allowlist de
> relações = "views parametrizadas seguras"). A superfície completa de configuração/templates/value-index do
> `theodb_ai_nl` abaixo é o alvo AlloyDB, **deferida** (YAGNI). Doc operacional: `docs/sql-ai-functions.md`.

Esta página cobre as consultas SQL e as APIs da extensão `theodb_ai_nl`, que traduz perguntas em
linguagem natural para SQL — configuração, contexto de schema, templates, fragmentos, concept types,
value index e geração/execução de consultas.

1. **Instalar extensão**

```sql
CREATE EXTENSION theodb_ai_nl CASCADE;
```

Instala a API de linguagem natural no banco. Usa `theodb_ml` para chamar modelos configuráveis do TheoDB.

2. **Verificar versão disponível**

```sql
SELECT *
FROM pg_available_extensions
WHERE name = 'theodb_ai_nl';
```

Consulta a versão instalada e a versão padrão disponível.

3. **Atualizar extensão**

```sql
ALTER EXTENSION theodb_ai_nl UPDATE;
```

Atualiza a extensão para a versão mais recente disponível.

4. **Criar configuração NL**

```sql
SELECT theodb_ai_nl.g_create_configuration('my_app_config');
```

Cria um `nl_config`, que vincula aplicação, schemas, contexto, templates e modelo.

5. **Registrar schema**

```sql
SELECT theodb_ai_nl.g_manage_configuration(
  operation => 'register_schema',
  configuration_id_in => 'my_app_config',
  schema_names_in => '{my_schema}'
);
```

Define quais schemas/tabelas/colunas podem ser usados na geração SQL.

6. **Adicionar contexto geral**

```sql
SELECT theodb_ai_nl.g_manage_configuration(
  'add_general_context',
  'my_app_config',
  general_context_in => '{"regra de negócio"}'
);
```

Inclui regras de negócio ou termos de domínio usados pelo gerador SQL.

7. **Listar contexto geral**

```sql
SELECT theodb_ai_nl.list_general_context(nl_config TEXT);
```

Retorna os contextos gerais cadastrados.

8. **Gerar contexto de schema**

```sql
SELECT theodb_ai_nl.generate_schema_context('my_app_config');
```

Gera descrições automáticas para tabelas, views e colunas.

9. **Revisar contexto gerado**

```sql
SELECT schema_object, object_context
FROM theodb_ai_nl.generated_schema_context_view;
```

Consulta os contextos gerados antes de aplicá-los.

10. **Atualizar contexto de relação**

```sql
SELECT theodb_ai_nl.update_generated_relation_context(
  'my_schema.my_table',
  'descrição técnica da tabela'
);
```

Corrige ou melhora a descrição de uma tabela/view/materialized view.

11. **Atualizar contexto de coluna**

```sql
SELECT theodb_ai_nl.update_generated_column_context(
  'my_schema.my_table.column1',
  'descrição técnica da coluna'
);
```

Corrige ou melhora a semântica de uma coluna.

12. **Aplicar contexto gerado**

```sql
SELECT theodb_ai_nl.apply_generated_schema_context('my_app_config');
```

Promove o contexto revisado para uso real na geração SQL.

13. **Aplicar sobrescrevendo contexto existente**

```sql
SELECT theodb_ai_nl.apply_generated_schema_context(
  'my_app_config',
  TRUE
);
```

Aplica o contexto gerado substituindo descrições anteriores.

14. **Consultar contexto de tabela**

```sql
SELECT theodb_ai_nl.get_relation_context('my_schema.my_table');
```

Mostra o contexto ativo de uma tabela/view.

15. **Consultar contexto de coluna**

```sql
SELECT theodb_ai_nl.get_column_context('my_schema.my_table.column1');
```

Mostra o contexto ativo de uma coluna.

16. **Definir contexto manual de tabela**

```sql
SELECT theodb_ai_nl.set_relation_context(
  'my_schema.my_table',
  'descrição manual'
);
```

Define diretamente a descrição sem depender da geração automática.

17. **Definir contexto manual de coluna**

```sql
SELECT theodb_ai_nl.set_column_context(
  'my_schema.my_table.column1',
  'descrição manual da coluna'
);
```

Define a semântica da coluna manualmente.

18. **Adicionar template de consulta**

```sql
SELECT theodb_ai_nl.add_template(
  nl_config_id => 'my_app_config',
  intent => 'pergunta em linguagem natural',
  sql => 'SELECT ...',
  check_intent => TRUE
);
```

Cadastra um par pergunta + SQL para guiar futuras gerações.

19. **Consultar templates**

```sql
SELECT *
FROM theodb_ai_nl.template_store_view;
```

Lista templates cadastrados, SQL parametrizado, intenção e status.

20. **Desabilitar template**

```sql
SELECT theodb_ai_nl.disable_template(template_id);
```

Mantém o template salvo, mas impede seu uso na geração SQL.

21. **Habilitar template**

```sql
SELECT theodb_ai_nl.enable_template(template_id);
```

Reativa um template desabilitado.

22. **Remover template**

```sql
SELECT theodb_ai_nl.drop_template(template_id);
```

Remove permanentemente o template.

23. **Adicionar fragmento**

```sql
SELECT theodb_ai_nl.add_fragment(
  nl_config_id => 'my_app_config',
  table_aliases => ARRAY['district AS T'],
  intent => 'Average salary between 6000 and 10000',
  fragment => 'T."A11" BETWEEN 6000 AND 10000',
  check_intent => TRUE
);
```

Cria um predicado SQL reutilizável para especializar templates.

24. **Consultar fragmentos**

```sql
SELECT *
FROM theodb_ai_nl.fragment_store_view;
```

Mostra fragmentos, escopo, intent, manifest e versão parametrizada.

25. **Desabilitar fragmento**

```sql
SELECT theodb_ai_nl.disable_fragment(fragment_id);
```

Impede uso do fragmento sem excluí-lo.

26. **Habilitar fragmento**

```sql
SELECT theodb_ai_nl.enable_fragment(fragment_id);
```

Reativa o fragmento.

27. **Remover fragmento**

```sql
SELECT theodb_ai_nl.drop_fragment(fragment_id);
```

Exclui o fragmento permanentemente.

28. **Gerar templates automaticamente**

```sql
SELECT theodb_ai_nl.generate_templates('my_app_config');
```

Gera templates com base no histórico de queries frequentes.

29. **Revisar templates gerados**

```sql
SELECT *
FROM theodb_ai_nl.generated_templates_view;
```

Mostra os templates sugeridos antes da aplicação.

30. **Atualizar template gerado**

```sql
SELECT theodb_ai_nl.update_generated_template(
  id => 1,
  manifest => 'descrição geral',
  nl => 'pergunta natural',
  intent => 'intenção',
  pintent => 'intenção parametrizada'
);
```

Ajusta templates automáticos antes de aplicá-los.

31. **Aplicar templates gerados**

```sql
SELECT theodb_ai_nl.apply_generated_templates('my_app_config');
```

Move templates aprovados para o `template_store`.

32. **Associar concept type**

```sql
SELECT theodb_ai_nl.associate_concept_type(
  column_names_in => 'my_schema.country.country_name',
  concept_type_in => 'country_name',
  nl_config_id_in => 'my_app_config'
);
```

Associa uma coluna a uma entidade semântica, como país, cidade, data ou pessoa.

33. **Criar value index**

```sql
SELECT theodb_ai_nl.create_value_index(
  nl_config_id_in => 'my_app_config'
);
```

Cria índice semântico para busca de valores citados nas perguntas.

34. **Atualizar value index**

```sql
SELECT theodb_ai_nl.refresh_value_index(
  nl_config_id_in => 'my_app_config'
);
```

Reprocessa o índice após mudanças em colunas ou concept types.

35. **Cadastrar sinônimos**

```sql
SELECT theodb_ai_nl.insert_synonym_set(
  ARRAY['USA', 'US', 'United States', 'United States of America']
);
```

Permite equivalência semântica entre valores diferentes.

36. **Buscar conceito e valor**

```sql
SELECT theodb_ai_nl.get_concept_and_value(
  value_phrases_in => ARRAY['United States'],
  nl_config_id_in => 'my_app_config'
);
```

Resolve termos da pergunta para valores reais no banco.

37. **Gerar associações automáticas de concept type**

```sql
SELECT theodb_ai_nl.generate_concept_type_associations(
  nl_config => 'my_app_config'
);
```

Sugere automaticamente quais colunas representam conceitos semânticos.

38. **Revisar associações geradas**

```sql
SELECT *
FROM theodb_ai_nl.generated_value_index_columns_view;
```

Consulta as associações sugeridas antes de aplicar.

39. **Atualizar associação gerada**

```sql
SELECT theodb_ai_nl.update_generated_concept_type_associations(
  id => 1,
  column_names => NULL,
  concept_type => 'generic_entity_name',
  additional_info => NULL
);
```

Altera uma associação sugerida.

40. **Remover associação gerada**

```sql
SELECT theodb_ai_nl.drop_generated_concept_type_association(id => 1);
```

Descarta uma sugestão de associação.

41. **Aplicar associações geradas**

```sql
SELECT theodb_ai_nl.apply_generated_concept_type_associations(
  nl_config => 'my_app_config'
);
```

Ativa as associações aprovadas.

42. **Gerar SQL a partir de linguagem natural**

```sql
SELECT theodb_ai_nl.get_sql(
  'my_app_config',
  'What is the population of the United States?'
);
```

Retorna JSON com SQL gerado, pergunta original, erro e metadados.

43. **Extrair apenas o SQL**

```sql
SELECT theodb_ai_nl.get_sql(
  'my_app_config',
  'What is the population of the United States?'
) ->> 'sql';
```

Retorna somente a string SQL.

44. **Gerar resumo dos resultados**

```sql
SELECT theodb_ai_nl.get_sql_summary(
  nl_config_id => 'my_app_config',
  nl_question => 'pergunta em linguagem natural'
);
```

Executa a consulta gerada e retorna uma resposta textual resumida.

45. **Resumo com filtros seguros**

```sql
SELECT theodb_ai_nl.get_sql_summary(
  nl_config_id => 'my_app_config',
  nl_question => 'pergunta',
  param_names => ARRAY['user_id'],
  param_values => ARRAY['123']
);
```

Executa a consulta respeitando parâmetros de segurança, como usuário autenticado.


---

## Exemplo end-to-end — montar e consultar um schema demo

O passo a passo abaixo monta um schema de demonstração (clientes, produtos, pedidos), popula dados,
gera embeddings e configura o `theodb_ai_nl` sobre esse schema — fechando a referência acima com um
caso concreto de ponta a ponta.

1. **Instalar extensão**

```sql
CREATE EXTENSION theodb_ai_nl CASCADE;
```

Instala a API de linguagem natural no banco.

2. **Atualizar extensão**

```sql
ALTER EXTENSION theodb_ai_nl UPDATE;
```

Atualiza a extensão já instalada.

3. **Criar schema demo**

```sql
CREATE SCHEMA nla_demo;
```

Cria o schema usado no tutorial.

4. **Criar tabela `addresses`**

```sql
CREATE TABLE nla_demo.addresses (...);
```

Armazena endereços de clientes e pedidos.

5. **Criar tabela `customers`**

```sql
CREATE TABLE nla_demo.customers (...);
```

Armazena dados de clientes.

6. **Criar tabela `categories`**

```sql
CREATE TABLE nla_demo.categories (...);
```

Armazena categorias de produtos.

7. **Criar tabela `brands`**

```sql
CREATE TABLE nla_demo.brands (...);
```

Armazena marcas de produtos.

8. **Criar tabela `products`**

```sql
CREATE TABLE nla_demo.products (..., description_embedding VECTOR(768));
```

Armazena produtos, preço e embedding para busca semântica.

9. **Criar tabela `orders`**

```sql
CREATE TABLE nla_demo.orders (...);
```

Armazena pedidos, status e valores.

10. **Criar tabela `order_items`**

```sql
CREATE TABLE nla_demo.order_items (...);
```

Armazena os itens individuais de cada pedido.

11. **Inserir dados em `addresses`**

```sql
INSERT INTO nla_demo.addresses (...) VALUES (...);
```

Popula endereços sintéticos.

12. **Inserir dados em `customers`**

```sql
INSERT INTO nla_demo.customers (...) VALUES (...);
```

Popula clientes.

13. **Inserir dados em `categories`**

```sql
INSERT INTO nla_demo.categories (...) VALUES (...);
```

Popula categorias.

14. **Inserir dados em `brands`**

```sql
INSERT INTO nla_demo.brands (...) VALUES (...);
```

Popula marcas.

15. **Inserir dados em `products`**

```sql
INSERT INTO nla_demo.products (...) VALUES (...);
```

Popula catálogo de produtos.

16. **Gerar embeddings dos produtos**

```sql
UPDATE nla_demo.products
SET description_embedding = embedding('theodb-embedding-004', description);
```

Calcula vetores semânticos das descrições.

17. **Inserir dados em `orders`**

```sql
INSERT INTO nla_demo.orders (...) VALUES (...);
```

Popula pedidos.

18. **Inserir dados em `order_items`**

```sql
INSERT INTO nla_demo.order_items (...) VALUES (...);
```

Popula itens de pedidos.

19. **Criar configuração NL**

```sql
SELECT theodb_ai_nl.g_create_configuration('nla_demo_cfg');
```

Cria a configuração usada para perguntas em linguagem natural.

20. **Registrar tabelas na configuração**

```sql
SELECT theodb_ai_nl.g_manage_configuration(
  operation => 'register_table_view',
  configuration_id_in => 'nla_demo_cfg',
  table_views_in => '{nla_demo.customers, nla_demo.addresses, nla_demo.brands, nla_demo.products, nla_demo.categories, nla_demo.orders, nla_demo.order_items}'
);
```

Define quais tabelas o gerador SQL pode usar.

21. **Gerar contexto de schema**

```sql
SELECT theodb_ai_nl.generate_schema_context(
  'nla_demo_cfg',
  TRUE
);
```

Gera contexto técnico automático para tabelas e colunas.

22. **Consultar contexto de tabela gerado**

```sql
SELECT object_context
FROM theodb_ai_nl.generated_schema_context_view
WHERE schema_object = 'nla_demo.products';
```

Verifica o contexto gerado para a tabela `products`.

23. **Consultar contexto de coluna gerado**

```sql
SELECT object_context
FROM theodb_ai_nl.generated_schema_context_view
WHERE schema_object = 'nla_demo.products.name';
```

Verifica o contexto gerado para a coluna `name`.

24. **Atualizar contexto de tabela**

```sql
SELECT theodb_ai_nl.update_generated_relation_context(
  'nla_demo.products',
  'The "nla_demo.products" table stores product details...'
);
```

Ajusta manualmente o contexto da tabela.

25. **Atualizar contexto de coluna**

```sql
SELECT theodb_ai_nl.update_generated_column_context(
  'nla_demo.products.name',
  'The "name" column contains the specific name or title...'
);
```

Ajusta manualmente o contexto da coluna.

26. **Aplicar contexto de tabela**

```sql
SELECT theodb_ai_nl.apply_generated_relation_context(
  'nla_demo.products',
  TRUE
);
```

Aplica o contexto revisado à tabela.

27. **Aplicar contexto de coluna**

```sql
SELECT theodb_ai_nl.apply_generated_column_context(
  'nla_demo.products.name',
  TRUE
);
```

Aplica o contexto revisado à coluna.

28. **Aplicar todo o contexto gerado**

```sql
SELECT theodb_ai_nl.apply_generated_schema_context(
  'nla_demo_cfg',
  TRUE
);
```

Aplica o contexto a todos os objetos registrados.

29. **Criar concept type `product_name`**

```sql
SELECT theodb_ai_nl.add_concept_type(
  concept_type_in => 'product_name',
  match_function_in => 'theodb_ai_nl.get_concept_and_value_generic_entity_name',
  additional_info_in => '{...}'::jsonb
);
```

Cria um tipo semântico para nomes de produtos.

30. **Associar `product_name` à coluna**

```sql
SELECT theodb_ai_nl.associate_concept_type(
  'nla_demo.products.name',
  'product_name',
  'nla_demo_cfg'
);
```

Liga o conceito `product_name` à coluna `products.name`.

31. **Listar concept types**

```sql
SELECT theodb_ai_nl.list_concept_types();
```

Valida se o concept type foi criado.

32. **Verificar associação de concept type**

```sql
SELECT *
FROM theodb_ai_nl.value_index_columns
WHERE column_names = 'nla_demo.products.name';
```

Confirma a associação da coluna ao conceito.

33. **Criar concept type `brand_name`**

```sql
SELECT theodb_ai_nl.add_concept_type(
  concept_type_in => 'brand_name',
  match_function_in => 'theodb_ai_nl.get_concept_and_value_generic_entity_name',
  additional_info_in => '{...}'::jsonb
);
```

Cria tipo semântico para nomes de marcas.

34. **Associar `brand_name` à coluna**

```sql
SELECT theodb_ai_nl.associate_concept_type(
  'nla_demo.brands.brand_name',
  'brand_name',
  'nla_demo_cfg'
);
```

Liga o conceito à coluna `brands.brand_name`.

35. **Criar value index**

```sql
SELECT theodb_ai_nl.create_value_index('nla_demo_cfg');
```

Cria índice para resolver valores citados nas perguntas.

36. **Atualizar value index**

```sql
SELECT theodb_ai_nl.refresh_value_index('nla_demo_cfg');
```

Atualiza o índice semântico.

37. **Gerar associações automáticas**

```sql
SELECT theodb_ai_nl.generate_concept_type_associations('nla_demo_cfg');
```

Sugere automaticamente concept types para colunas.

38. **Revisar associações geradas**

```sql
SELECT *
FROM theodb_ai_nl.generated_value_index_columns_view;
```

Mostra sugestões de associação.

39. **Atualizar associação gerada**

```sql
SELECT theodb_ai_nl.update_generated_concept_type_associations(
  id => 1,
  column_names => NULL,
  concept_type => 'generic_entity_name',
  additional_info => NULL
);
```

Altera uma associação sugerida.

40. **Remover associação gerada**

```sql
SELECT theodb_ai_nl.drop_generated_concept_type_association(id => 1);
```

Descarta uma associação sugerida.

41. **Aplicar associações geradas**

```sql
SELECT theodb_ai_nl.apply_generated_concept_type_associations('nla_demo_cfg');
```

Ativa as associações revisadas.

42. **Adicionar template de consulta**

```sql
SELECT theodb_ai_nl.add_template(
  nl_config_id => 'nla_demo_cfg',
  intent => 'List the first names and the last names of all customers who ordered Swimsuit.',
  sql => 'SELECT ...',
  sql_explanation => '...',
  check_intent => TRUE
);
```

Cria um exemplo orientador para perguntas críticas.

43. **Consultar templates**

```sql
SELECT nl, sql, intent, psql, pintent
FROM theodb_ai_nl.template_store_view
WHERE config = 'nla_demo_cfg';
```

Lista template original, SQL e versões parametrizadas.

44. **Adicionar template com busca semântica**

```sql
SELECT theodb_ai_nl.add_template(
  nl_config_id => 'nla_demo_cfg',
  intent => 'List 3 products most similar to a Swimwear.',
  sql => $$SELECT name FROM nla_demo.products
          ORDER BY description_embedding <=> embedding('theodb-embedding-004', 'Swimwear')::vector$$,
  sql_explanation => $$...$$,
  check_intent => TRUE
);
```

Ensina o gerador a usar embeddings e distância vetorial.

45. **Adicionar fragmento**

```sql
SELECT theodb_ai_nl.add_fragment(
  nl_config_id => 'nla_demo_cfg',
  table_aliases => ARRAY['nla_demo.products AS T'],
  intent => 'luxury product',
  fragment => $$description LIKE '%luxury%' OR description LIKE '%premium%' ...$$
);
```

Cria um predicado SQL reutilizável para filtros de domínio.

46. **Consultar fragmentos**

```sql
SELECT intent, fragment, pintent
FROM theodb_ai_nl.fragment_store_view;
```

Lista fragmentos disponíveis.

47. **Gerar SQL para clientes que compraram produto**

```sql
SELECT theodb_ai_nl.get_sql(
  'nla_demo_cfg',
  'Find the customers who purchased Tote Bag.'
) ->> 'sql';
```

Gera SQL com joins entre clientes, pedidos, itens e produtos.

48. **Gerar SQL usando value index para produto**

```sql
SELECT theodb_ai_nl.get_sql(
  'nla_demo_cfg',
  'List the maximum price of any CymbalShoe.'
) ->> 'sql';
```

Resolve `CymbalShoe` como nome de produto.

49. **Gerar SQL usando value index para marca**

```sql
SELECT theodb_ai_nl.get_sql(
  'nla_demo_cfg',
  'List the maximum price of any CymbalPrime.'
) ->> 'sql';
```

Resolve `CymbalPrime` como marca e gera join com `brands`.

50. **Executar pergunta em linguagem natural**

```sql
SELECT theodb_ai_nl.execute_nl_query(
  'nla_demo_cfg',
  'Find the last name of the customers who live in Lisbon.'
);
```

Gera e executa a consulta, retornando o resultado.

51. **Gerar SQL com busca semântica**

```sql
SELECT theodb_ai_nl.get_sql(
  'nla_demo_cfg',
  'List 2 products similar to a Tote Bag.'
);
```

Produz SQL com ordenação por distância vetorial.

52. **Gerar resumo SQL**

```sql
SELECT theodb_ai_nl.get_sql_summary(
  nl_config_id => 'nla_demo_cfg',
  nl_question => 'which brands have the largest number of products.'
);
```

Executa a pergunta e retorna resposta textual resumida.

53. **Remover templates**

```sql
SELECT theodb_ai_nl.drop_template(id)
FROM theodb_ai_nl.template_store_view
WHERE config = 'nla_demo_cfg';
```

Remove templates criados no tutorial.

54. **Remover associações geradas**

```sql
SELECT theodb_ai_nl.drop_generated_concept_type_association(id)
FROM theodb_ai_nl.generated_value_index_columns_view
WHERE config = 'nla_demo_cfg';
```

Remove associações automáticas pendentes/geradas.

55. **Remover concept type**

```sql
SELECT theodb_ai_nl.drop_concept_type('product_name');
```

Exclui o tipo semântico customizado.

56. **Atualizar value index global**

```sql
SELECT theodb_ai_nl.refresh_value_index();
```

Recalcula o índice após remoções.

57. **Remover configuração**

```sql
SELECT theodb_ai_nl.g_manage_configuration(
  'drop_configuration',
  'nla_demo_cfg'
);
```

Exclui a configuração NL.

58. **Remover schema demo**

```sql
DROP SCHEMA nla_demo CASCADE;
```

Remove todas as tabelas e objetos do schema.
