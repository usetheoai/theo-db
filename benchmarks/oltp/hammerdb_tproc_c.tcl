# M129 — HammerDB TPROC-C driver for TheoDB (PostgreSQL-wire-compatible). OUR script; it drives the GPLv3 HammerDB
# CLI (run inside the tpcorg/hammerdb Docker image) — NO HammerDB source is vendored or linked (D1). Config comes
# from env (set by run_m129_oltp.py). TPROC-C = the real TPC-C 45/43/4/4/4 mix; metric = NOPM (NOT audited tpmC).
set pg_host   [expr {[info exists env(PG_HOST)]      ? $env(PG_HOST)      : "localhost"}]
set pg_port   [expr {[info exists env(PG_PORT)]      ? $env(PG_PORT)      : "28900"}]
set pg_ware   [expr {[info exists env(PG_WAREHOUSES)]? $env(PG_WAREHOUSES): "4"}]
set pg_vu     [expr {[info exists env(PG_VU)]        ? $env(PG_VU)        : "4"}]
set pg_rampup [expr {[info exists env(PG_RAMPUP)]    ? $env(PG_RAMPUP)    : "1"}]
set pg_dur    [expr {[info exists env(PG_DURATION)]  ? $env(PG_DURATION)  : "2"}]

dbset db pg
dbset bm TPC-C
diset connection pg_host $pg_host
diset connection pg_port $pg_port
diset tpcc pg_superuser postgres
diset tpcc pg_superuserpass postgres
diset tpcc pg_defaultdbase postgres
diset tpcc pg_count_ware $pg_ware
diset tpcc pg_num_vu $pg_vu
puts "M129: building TPROC-C schema ($pg_ware warehouses) against TheoDB $pg_host:$pg_port"
buildschema
after 5000

diset tpcc pg_driver timed
diset tpcc pg_rampup $pg_rampup
diset tpcc pg_duration $pg_dur
diset tpcc pg_timeprofile true
loadscript
puts "M129: running TPROC-C ($pg_vu VUs, rampup $pg_rampup min, duration $pg_dur min)"
vuset vu $pg_vu
vucreate
set jobid [vurun]
runtimer [expr {($pg_rampup * 60) + ($pg_dur * 60) + 30}]
vudestroy
# HammerDB prints "System achieved NNN NOPM from MMM PostgreSQL TPM" — parsed by run_m129_oltp.parse_hammerdb_nopm.
