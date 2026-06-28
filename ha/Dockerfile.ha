# TheoDB HA node = theo-db:dev (postgres 17 + pgvector + pgvectorscale + plpython3) managed by Patroni,
# with pgBackRest for WAL archiving + PITR. M4 (operação básica). Patroni MIT, pgBackRest MIT (D1-clean).
FROM theo-db:dev

USER root

# pgBackRest from PGDG (already configured in the postgres base) + python for Patroni + net tools.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      python3-venv python3-pip pgbackrest iproute2 hostname && \
    rm -rf /var/lib/apt/lists/*

# Patroni in a venv (Debian python is PEP-668 externally-managed). etcd3 DCS + psycopg2 driver.
RUN python3 -m venv /opt/patroni && \
    /opt/patroni/bin/pip install --no-cache-dir 'patroni[etcd3]' psycopg2-binary
ENV PATH="/opt/patroni/bin:${PATH}"

# Writable dirs owned by postgres: PGDATA (Patroni runs its own initdb), pgBackRest repo + logs.
RUN mkdir -p /var/lib/postgresql/data /var/lib/pgbackrest /var/log/pgbackrest /etc/pgbackrest && \
    chown -R postgres:postgres /var/lib/postgresql/data /var/lib/pgbackrest /var/log/pgbackrest && \
    chmod 700 /var/lib/postgresql/data && chmod 750 /var/lib/pgbackrest

COPY ha/pgbackrest.conf /etc/pgbackrest/pgbackrest.conf
COPY ha/patroni-entrypoint.sh /usr/local/bin/patroni-entrypoint.sh
RUN chmod +x /usr/local/bin/patroni-entrypoint.sh

USER postgres
EXPOSE 5432 8008
ENTRYPOINT ["/usr/local/bin/patroni-entrypoint.sh"]
