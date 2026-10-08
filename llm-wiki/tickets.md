# peacockdb tickets

**This file is the index; the tickets themselves live in individual files under
[`tickets/`](tickets/), one per milestone.** Add a ticket to the file of the milestone it
blocks, never here.

Migrated from GitHub issues on 2026-07-31; GitHub issues are closed and these files are the
registry. The number is the permanent ticket ID. Each ticket carries an `<a id="tNN">` anchor
above its header, which the cost widget links to, so link a ticket as
`tickets/<milestone>.md#tNN`. Device labels are `tp<N>-<tier>` (micro=100MiB, mini=2GiB,
standard=12GiB).

A ticket carries a **Priority** line only when it is not medium; medium is the default.
New tickets take the next free number (currently 284), one ID space across every file. 264–279
are left to chain J, whose branches number from 263 on their own boards.
Finished and lapsed tickets move to `archive/archived-tickets.md` (Done / Stale). Numbers
are never reused, so an old reference still resolves there.

## Contents

119 open tickets, in file order.

| File | Milestone | Open | Tickets |
|---|---|--:|---|
| [`corpus-coverage.md`](tickets/corpus-coverage.md) | Corpus rollout: every corpus query on the cpu and the device, all modes | 36 | #225 #216 #94 #280 #199 #55 #65 #62 #202 #217 #204 #214 #205 #168 #210 #57 #56 #60 #186 #282 #206 #240 #189 #145 #95 #197 #154 #227 #164 #201 #174 #233 #234 #235 #262 #281 |
| [`complete-coverage.md`](tickets/complete-coverage.md) | MVP SQL functionality: shapes the corpus does not reach | 7 | #239 #195 #161 #144 #261 #249 #283 |
| [`corpus-coverage.md`](tickets/corpus-coverage.md) | Corpus rollout: every corpus query on the cpu and the device, all modes | 33 | #225 #216 #94 #199 #55 #65 #62 #202 #217 #204 #214 #205 #168 #210 #57 #56 #60 #251 #186 #206 #240 #189 #145 #95 #197 #154 #227 #164 #201 #174 #233 #234 #235 |
| [`complete-coverage.md`](tickets/complete-coverage.md) | MVP SQL functionality: shapes the corpus does not reach | 5 | #239 #195 #161 #144 #249 |
| [`scalars.md`](tickets/scalars.md) | Scalar functions and expressions, for MVP SQL | 11 | #224 #211 #203 #223 #218 #222 #221 #219 #200 #162 #230 |
| [`joins.md`](tickets/joins.md) | Join execution | 21 | #155 #152 #153 #80 #59 #215 #208 #207 #190 #63 #212 #173 #136 #137 #159 #160 #220 #243 #245 #246 #250 |
| [`df-upgrade.md`](tickets/df-upgrade.md) | The DataFusion upgrade | 5 | #241 #23 #166 #228 #247 |
| [`memory.md`](tickets/memory.md) | Memory accounting and budgets | 5 | #182 #179 #177 #167 #229 |
| [`optimizer.md`](tickets/optimizer.md) | The optimizer project | 16 | #73 #101 #140 #170 #141 #139 #147 #20 #71 #19 #16 #75 #146 #158 #142 #138 |
| [`performance.md`](tickets/performance.md) | The pre-production performance path | 7 | #150 #149 #148 #231 #232 #242 #248 |
| [`benchmarks.md`](tickets/benchmarks.md) | Wall-time benchmarks | 2 | #226 #69 |
| [`system-hardening.md`](tickets/system-hardening.md) | Pre-production system hardening | 6 | #13 #196 #169 #128 #244 #260 |
| [`testinfra.md`](tickets/testinfra.md) | Tests, CI, hosts and testdata | 4 | #252 #178 #176 #129 |
