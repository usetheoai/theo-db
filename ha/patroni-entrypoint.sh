#!/usr/bin/env bash
# Generate a per-node patroni.yml from env and exec patroni. Runs as the postgres user.
# Env: PATRONI_NAME (required), PATRONI_SCOPE (default theodb), PATRONI_ETCD3_HOSTS (required, e.g.
#      "etcd1:2379,etcd2:2379,etcd3:2379"), POSTGRES_PASSWORD, REPL_PASSWORD.
set -euo pipefail

SCOPE="${PATRONI_SCOPE:-theodb}"
NAME="${PATRONI_NAME:?PATRONI_NAME required}"
ETCD_HOSTS="${PATRONI_ETCD3_HOSTS:?PATRONI_ETCD3_HOSTS required}"
PGPW="${POSTGRES_PASSWORD:-postgres}"
REPLPW="${REPL_PASSWORD:-replpass}"
IP="$(hostname -i | awk '{print $1}')"

cat > /tmp/patroni.yml <<EOF
scope: ${SCOPE}
name: ${NAME}
restapi:
  listen: 0.0.0.0:8008
  connect_address: ${IP}:8008
etcd3:
  hosts: ${ETCD_HOSTS}
bootstrap:
  dcs:
    # RTO tuning — invariant: loop_wait + 2*retry_timeout <= ttl  (5 + 2*5 = 15 <= 20)
    ttl: 20
    loop_wait: 5
    retry_timeout: 5
    maximum_lag_on_failover: 1048576
    postgresql:
      use_pg_rewind: true
      # GLOBAL params (survive failover — every node inherits via DCS): pgBackRest archiving (ADR-2)
      parameters:
        wal_level: replica
        hot_standby: "on"
        max_wal_senders: 10
        max_replication_slots: 10
        archive_mode: "on"
        archive_command: "pgbackrest --stanza=${SCOPE} archive-push %p"
  initdb:
    - encoding: UTF8
    - data-checksums
  pg_hba:
    - host replication replicator 0.0.0.0/0 md5
    - host all all 0.0.0.0/0 md5
    - local all all trust
postgresql:
  listen: 0.0.0.0:5432
  connect_address: ${IP}:5432
  data_dir: /var/lib/postgresql/data
  bin_dir: /usr/lib/postgresql/17/bin
  pgpass: /tmp/pgpass
  authentication:
    superuser:
      username: postgres
      password: ${PGPW}
    replication:
      username: replicator
      password: ${REPLPW}
EOF

exec patroni /tmp/patroni.yml
