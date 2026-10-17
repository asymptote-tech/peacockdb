
# Corpus Rollout tickets

Tickets required for corpus rollout (CPU+GPU, all modes), TPC-H numbered and named queries, and 84/99 TPC-DS queries (window functions excluded). Minimum SQL functionality needs to be built for this milestone.

## Contents

- [#302 (non-inner join gpu cells) the named non-inner join queries' device cells have never run](#t302)

<a id="t302"></a>
### #302 (non-inner join gpu cells) the named non-inner join queries' device cells have never run

`tpch/left-join-reorder`, `semi-join-reorder`, `anti-join-reorder` and `left-join-fixed` put a
non-inner join where a reorder may move it. They are planned and answered on the cpu at every
mode, and no device has run one, so a device answer could differ from the cpu's and nothing would
say so. Their siblings `left-join`, `semi-join` and `anti-join` run everywhere on both engines.

**Corpus queries:** the four, gpu cells off at all five modes on this ticket alone.

**Fix proposed:** on a GPU host, run the twenty device cells against the cpu's answer and record
their `gpu-result.txt` sections; each one passing is enabled, each one failing gets the ticket it
fails on. Then this ticket drops from the registry rows and is archived.
