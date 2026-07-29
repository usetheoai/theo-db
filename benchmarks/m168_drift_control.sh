#!/bin/bash
# M168 — o controle de deriva. Converte a comparação ENTRE coletas de `aaabbb` para `ababab`, que é o que o
# Georges et al. § 2.1.2 prescreve. Roda o binário da coleta A e o da coleta F alternados, na MESMA janela.
set -uo pipefail
P=/root/.pgrx/18.4/pgrx-install/bin
D=/home/pgtest/m167data
SO_LIVE=/root/.pgrx/18.4/pgrx-install/lib/postgresql/theodb_rs.so
SO_A=/root/theo-db-A/theodb_rs/target/release/libtheodb_rs.so
SO_F=/root/so_F.bak
OUT=/root/m168-drift-control
mkdir -p $OUT

swap() {  # $1 = caminho do .so, $2 = rótulo
  cp "$1" "$SO_LIVE"
  su - pgtest -c "THEODB_ADMIT_TRACE=1 $P/pg_ctl -D $D -m fast -l $D/pg.log restart -o '-p 28900'" >/dev/null 2>&1
  sleep 5
  PID=$(head -1 $D/postmaster.pid)
  if grep -m1 theodb_rs /proc/$PID/maps | grep -q deleted; then
    echo "FATAL: .so mapeado como (deleted) — o restart não pegou o binário de $2"; exit 2
  fi
  echo "  [$2] md5=$(md5sum $SO_LIVE | cut -d' ' -f1)  postmaster=$(su - pgtest -c "$P/psql -h localhost -p 28900 -U postgres -d postgres -tAc 'SELECT pg_postmaster_start_time()'")"
}

for i in 1 2; do
  for arm in A F; do
    [ "$arm" = A ] && src=$SO_A || src=$SO_F
    swap "$src" "$arm-$i"
    f=$OUT/$arm-$i.log
    { echo "=== drift-control arm=$arm rep=$i start $(date -Is) ==="
      echo "so_md5=$(md5sum $SO_LIVE | cut -d' ' -f1)"
      echo "loadavg=$(cut -d' ' -f1-3 /proc/loadavg)"
      su - pgtest -c "$P/psql -h localhost -p 28900 -U postgres -d postgres -f /root/theo-db/benchmarks/m168_stream_ab.sql" 2>&1
      echo "=== end $(date -Is) ===" ; } > "$f" 2>&1
    printf '  %-6s -> %s\n' "$arm-$i" "$f"
  done
done
cp "$SO_F" "$SO_LIVE"   # restaura F como o binário vivo
su - pgtest -c "THEODB_ADMIT_TRACE=1 $P/pg_ctl -D $D -m fast -l $D/pg.log restart -o '-p 28900'" >/dev/null 2>&1
echo "  binário F restaurado"
