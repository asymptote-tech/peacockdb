# peacockdb tickets

**This file is the index; the tickets themselves live in individual files under
[`tickets/`](tickets/), one per milestone.** Add a ticket to the file of the milestone it
blocks, never here.

Migrated from GitHub issues on 2026-07-31; GitHub issues are closed and these files are the
registry. The number is the permanent ticket ID. Each ticket carries an `<a id="tNN">` anchor
above its header, which the cost widget links to, so link a ticket as
`tickets/<milestone>.md#tNN`. A header reads `### #NN (short label) title`: the label is at
most 25 characters and is what `scripts/ticket_widget.py` shows beside the number. Device labels are `tp<N>-<tier>` (micro=100MiB, mini=2GiB,
standard=12GiB).

A ticket carries a **Priority** line only when it is not medium; medium is the default.
New tickets take the next free number (currently 291), one ID space across every file. 264–279
are reserved for chain J, whose next number is 268.
Finished and lapsed tickets move to `archive/archived-tickets.md` (Done / Stale). Numbers
are never reused, so an old reference still resolves there.

## Contents

91 open tickets, in file order.

| File | Milestone | Open | Tickets |
|---|---|--:|---|
| [`corpus-coverage.md`](tickets/corpus-coverage.md) | Corpus rollout: every corpus query on the cpu and the device, all modes | 14 | #264 #267 #289 #202 #217 #204 #168 #56 #60 #251 #227 #253 #254 #287 |
| [`complete-coverage.md`](tickets/complete-coverage.md) | MVP SQL functionality: shapes the corpus does not reach | 9 | #239 #195 #161 #144 #261 #249 #283 #255 #284 |
| [`scalars.md`](tickets/scalars.md) | Scalar functions and expressions, for MVP SQL | 12 | #224 #211 #203 #223 #218 #222 #221 #219 #200 #162 #230 #288 |
| [`joins.md`](tickets/joins.md) | Join execution | 4 | #243 #245 #246 #250 |
| [`df-upgrade.md`](tickets/df-upgrade.md) | The DataFusion upgrade | 6 | #241 #23 #166 #228 #247 #257 |
| [`memory.md`](tickets/memory.md) | Memory accounting and budgets | 6 | #182 #179 #177 #167 #229 #265 |
| [`optimizer.md`](tickets/optimizer.md) | The optimizer project | 16 | #73 #101 #140 #170 #141 #139 #147 #20 #71 #19 #16 #75 #146 #158 #142 #138 |
| [`performance.md`](tickets/performance.md) | The pre-production performance path | 9 | #150 #149 #148 #231 #232 #242 #248 #286 #290 |
| [`benchmarks.md`](tickets/benchmarks.md) | Wall-time benchmarks | 2 | #226 #69 |
| [`system-hardening.md`](tickets/system-hardening.md) | Pre-production system hardening | 6 | #13 #196 #169 #128 #244 #260 |
| [`testinfra.md`](tickets/testinfra.md) | Tests, CI, hosts and testdata | 7 | #266 #263 #258 #252 #178 #176 #129 |
