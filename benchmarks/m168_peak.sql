-- M168 — memória decodificada: eager (whole relation) vs streaming (per chunk-group), NA MESMA SESSÃO.
--
-- WHY BOTH ARMS HERE. A primeira versão deste verdict comparava um "antes" medido num binário anterior contra um
-- "depois" medido no atual, e o "antes" não tinha artefato commitado — a razão principal tinha um numerador que o
-- leitor não conseguia verificar (achado de review). Rodando os dois estados da GUC numa sessão só, o par vira um
-- artefato único, mesmo processo, mesmo binário, e a pergunta "binários diferentes invalidam a comparação?"
-- deixa de existir.
--
-- REQUER `THEODB_ADMIT_TRACE=1` **no ambiente do postmaster** — o backend herda dele, não do psql. Exportar a
-- variável na linha do psql devolve zero traces e um falso-negativo silencioso (custou uma medição nesta sessão).
--
-- O que ler no output: linhas `theodb_decode_batch:` são o caminho eager (um por consulta, a relação inteira);
-- linhas `theodb_decode_batch_stream:` são o streaming (uma por chunk-group, incluindo `probe=1`). As duas
-- famílias saem de sítios mutuamente exclusivos, então a contagem prova qual caminho rodou — não é inferência.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';

\echo '########## ARM=eager (theodb.enable_columnar_topk_stream = off) ##########'
SET theodb.enable_columnar_topk_stream = off;
\echo '===q23==='
SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10 \g /dev/null
\echo '===q24==='
SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10 \g /dev/null
\echo '===q25==='
SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10 \g /dev/null
\echo '===q26==='
SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10 \g /dev/null

\echo '########## ARM=stream (theodb.enable_columnar_topk_stream = on) ##########'
SET theodb.enable_columnar_topk_stream = on;
\echo '===q23==='
SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10 \g /dev/null
\echo '===q24==='
SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10 \g /dev/null
\echo '===q25==='
SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10 \g /dev/null
\echo '===q26==='
SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10 \g /dev/null

\echo '########## fim — some as linhas por braço com benchmarks/m168_peak_summarize.py ##########'
