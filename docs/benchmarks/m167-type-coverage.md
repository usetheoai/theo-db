# M163 — type-coverage A/B run

**Rows loaded:** 2000 (equal in `hits` columnar + `hits_heap`).  
**Positive control:** seeded divergence detected (diverged=2) — the oracle catches a wrong result.  
**Result:** 35/35 cases as-expected.

| case | expect | got | diverged | ok |
|---|---|---|---|---|
| agg_count | route | ok | 0 | ✅ |
| agg_sum_i4 | route | ok | 0 | ✅ |
| agg_sum_i8 | route | ok | 0 | ✅ |
| inlist_i4 | route | ok | 0 | ✅ |
| inlist_i2 | route | ok | 0 | ✅ |
| inlist_null | decline | declined | None | ✅ |
| intpk_i2 | route | ok | 0 | ✅ |
| intpk_i4 | route | ok | 0 | ✅ |
| intpk_i8_result | decline | declined | None | ✅ |
| intpk_i4_wide | decline | declined | None | ✅ |
| date_plus | decline | declined | None | ✅ |
| ts_inlist | decline | declined | None | ✅ |
| tz_group | route | ok | 0 | ✅ |
| extract_minute | route | ok | 0 | ✅ |
| extract_day | decline | declined | None | ✅ |
| group_f8 | decline | declined | None | ✅ |
| group_f4 | decline | declined | None | ✅ |
| group_i2 | route | ok | 0 | ✅ |
| group_bool | route | ok | 0 | ✅ |
| group_text | route | ok | 0 | ✅ |
| const_out_i4 | route | ok | 0 | ✅ |
| const_out_i2 | route | ok | 0 | ✅ |
| const_out_i8 | route | ok | 0 | ✅ |
| const_out_float | decline | declined | None | ✅ |
| const_out_text | decline | declined | None | ✅ |
| const_out_null | decline | declined | None | ✅ |
| sum_i2_add | route | ok | 0 | ✅ |
| sum_i2_sub | route | ok | 0 | ✅ |
| sum_i4_add_decline | decline | declined | None | ✅ |
| sum_i8_add_decline | decline | declined | None | ✅ |
| sum_i2_wide_decline | decline | declined | None | ✅ |
| topk_int_order | route | ok | 0 | ✅ |
| topk_ts_order | route | ok | 0 | ✅ |
| topk_multikey | route | ok | 0 | ✅ |
| topk_text_named_collation_decline | decline | declined | None | ✅ |

Each `route` case is EXPLAIN=Custom Scan + symmetric-EXCEPT diverged=0; each `decline` case is native (no Custom Scan), the M161 fail-closed contract, over the type-edge catalog the ClickBench A/B misses.
