# Sirius (sirius-db/sirius): activity, governance and NVIDIA involvement

All facts accessed 2026-10-28 unless stated otherwise. "R" below = repo https://github.com/sirius-db/sirius. File citations are pinned to commit b9e05ffc653c8ed031c789e5c1415ac1186e0619 (main HEAD on 2026-10-28), e.g. https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/GOVERNANCE.md.

Raw gh api output used for the numbers is saved under `/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ghraw/`:
- `repo.json`, `commits_flat.json` (1,244 commits since 2025-10-28), `stats_code_frequency.json`, `stats_contributors.json`, `stats_commit_activity.json`, `releases.json`, `tags.json`, `profiles.tsv`
- `issues_gql.json` (all 591 issues), `prs_gql.json` (all 1,763 PRs), `resp_summary.txt`
- The responsiveness analysis script is `/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/analyze_resp.py`.

Method limit: the session's WebSearch budget ran out after one query. Web evidence therefore comes from one search plus direct fetches of known URLs: the NVIDIA blog, arXiv, and sirius-db.com. The GTC catalogue, news coverage and HN were not searched; the Gaps entries record this.

## 1. Commit volume and cadence (2025-10-28 → 2026-10-28)

### Takeaway
Sirius is very active. The default branch `main` received 1,244 commits in the 12 months: 1,111 by people and 133 by dependabot. Activity is speeding up. The last 6 months (May–Oct 2026) had 848 commits against 382 in the 6 months before. October 2026 alone had 209 commits from 23 distinct human authors.

### Cited Findings
- Default branch is `main`. The repo was created 2024-06-19 and has 1,101 stars, 145 forks and 351 open issues plus PRs. Licence metadata: Apache-2.0. Not archived. — [gh api repos/sirius-db/sirius](https://api.github.com/repos/sirius-db/sirius) (accessed 2026-10-28; `ghraw/repo.json`)
- On 2026-09-24 the default branch was renamed from `dev` to `main` (issue #1859, opened by mike-wendt 2026-09-22). The old `main` is kept read-only as `legacy-main`, frozen at 2611fa28 on 2025-11-07. The `dev` tip at the time of the rename is kept as `legacy-dev`, f79a6f42 on 2026-09-23. — [CONTRIBUTING.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/CONTRIBUTING.md); [issue #1859](https://github.com/sirius-db/sirius/issues/1859) (accessed 2026-10-28)
- Today's `main` continues the old `dev` line rather than replacing it. `main` is 247 commits ahead of `legacy-dev`, 0 behind, and the merge base is f79a6f42. `main` is 1,199 commits ahead of `legacy-main`. — [compare legacy-dev...main](https://github.com/sirius-db/sirius/compare/legacy-dev...main) (accessed 2026-10-28)
- Commits on `main` by month (committer date), from `GET /repos/sirius-db/sirius/commits?since=2025-10-28`. — [commits API](https://api.github.com/repos/sirius-db/sirius/commits?since=2025-10-28T00:00:00Z) (accessed 2026-10-28; `ghraw/commits_flat.json`)

  ```
  2025-10 (28–31 only)  14
  2025-11   41
  2025-12   17
  2026-01   28
  2026-02  104
  2026-03  104
  2026-04   88
  2026-05   74
  2026-06  121
  2026-07  174
  2026-08  142
  2026-09  128
  2026-10  209   (to 28 Oct)
  total   1,244  (dependabot 133, human 1,111; 6 multi-parent commits, rest squash merges)
  ```
- Distinct human commit authors per month rose from 3 (Oct 2025), 6 (Nov) and 10 (Dec) to 18 (Jul 2026), 18 (Aug), 24 (Sep) and 23 (Oct). 44 distinct human authors in the 12 months. — same source, computed (accessed 2026-10-28)
- Merging uses squash merges through a merge queue. PR titles become commit messages, in Conventional Commits format. — [CONTRIBUTING.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/CONTRIBUTING.md) (accessed 2026-10-28)
- PR and issue volume in the window: 1,720 PRs created, 1,198 PRs merged, 542 issues created. All-time totals: 1,763 PRs (1,235 merged, 394 closed unmerged, 134 open) and 591 issues (217 open, 374 closed). — [GitHub search API, repo:sirius-db/sirius](https://github.com/sirius-db/sirius/pulls?q=is%3Apr) (accessed 2026-10-28)

### Inferences
- About 97% of all PRs were created in the last 12 months (1,720 of 1,763). The project in its current form is about one year old in practice. Before late 2025 it was a small academic repo.
- Since Feb 2026 the pace has been about 100–200 merged commits per month, or roughly 5–10 per working day.

### Gaps
- `stats/commit_activity` and `stats/participation` were saved but not used. The commits list is the authoritative source.

## 2. Contributors and employers; NVIDIA share

### Takeaway
NVIDIA dominates development. By the affiliations listed in MAINTAINERS.md, NVIDIA staff authored about 92% of human commits in the window (1,019 of 1,111). UW–Madison authored about 4% (47) and everyone else about 4% (45). 20 of the 23 maintainers list NVIDIA, and the UW principal investigator now lists "UW-Madison, NVIDIA".

### Cited Findings
- MAINTAINERS.md lists 23 maintainers. 20 are affiliated "NVIDIA", 2 "UW-Madison" (Andy Lin/yayen-lin, Duc Nguyen/ducndh), and 1 "UW-Madison, NVIDIA" (Xiangyao Yu/yxymit, the UW professor who leads the project). — [MAINTAINERS.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/MAINTAINERS.md) (accessed 2026-10-28)
- PR #1808 (mbrobbel, 2026-09-21), "update affiliation of Yu and Xiangyao", changed Xiangyao Yu from "UW-Madison" to "UW-Madison, NVIDIA" and Yu Teng from "TBD" to "NVIDIA". — [commit b2961f8f](https://github.com/sirius-db/sirius/commit/b2961f8f) (accessed 2026-10-28)
- Maintainer additions since governance began: James Bourbeau (#2018/#2069, 2026-10), Mads Kristensen (#2072) and Lawrence Mitchell (#2071). All three are NVIDIA. Ben Zaitlen (quasiben) was made Collaborator (#2050). All nominations were opened by NVIDIA maintainers mike-wendt and mbrobbel. — [issues labelled `nominate`](https://github.com/sirius-db/sirius/issues?q=label%3Anominate) (accessed 2026-10-28)
- Top human commit authors on `main`, 2025-10-28 → 2026-10-28, with affiliation. Affiliation is from MAINTAINERS.md unless marked; profiles were fetched via `gh api users/<login>` (`ghraw/profiles.tsv`). — [commits API](https://api.github.com/repos/sirius-db/sirius/commits?since=2025-10-28T00:00:00Z); [MAINTAINERS.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/MAINTAINERS.md) (accessed 2026-10-28)

  ```
  236 mbrobbel        Matthijs Brobbel      NVIDIA (profile: @NVIDIA)
  132 bwyogatama      Bobbi Yogatama        NVIDIA (UW PhD; paper lists NVIDIA)
   72 kevkrist        Kevin Kristensen      NVIDIA (profile: Solutions Architect at NVIDIA; paper Aug 2025: UW–Madison)
   67 wmalpica        William Malpica       NVIDIA
   66 joosthooz       Joost Hoozemans       NVIDIA
   65 felipeblazing   Felipe Aramburu       NVIDIA
   60 9prady9         Pradeep Garigipati    NVIDIA
   58 Jedi18          Akhil Nair            NVIDIA
   53 aminaramoon     Amin Aramoon          NVIDIA
   53 mike-wendt      Mike Wendt            NVIDIA
   36 ran-yuan-rui    Yu Teng               NVIDIA
   26 infblueocean    Abel Brown            NVIDIA
   23 ducndh          Duc Nguyen            UW–Madison
   20 johanpel        Johan Peltenburg      NVIDIA
   19 dhruv9vats      Dhruv Vats            NVIDIA
   17 madsbk          Mads Kristensen       NVIDIA
   14 jrbourbeau      James Bourbeau        NVIDIA
   11 aocsa           Alexander Ocsa        NVIDIA
    9 buzzcrow        "Gian"                unknown (no company on profile)
    9 Yifei-yang7     Yifei Yang            UW–Madison (profile)
    8 wence-          Lawrence Mitchell     NVIDIA
    7 yayen-lin       Andy Lin              UW–Madison
    7 woodyycchang    Woody Chang           Cornell MEng student (profile)
    6 ye-yi-yy        Ye Yi                 unknown
    6 roaramburu      Rodrigo Aramburu      NVIDIA
    4 realAaronWu     Aaron Wu              unknown
    3 jjohnson-99     Jeremy                unknown (Bristol, UK; "Mathematics Ph.D.")
    3 morningman      Mingyu Chen           profile: "Apache Doris PMC Chair"
    3 yxymit          Xiangyao Yu           UW–Madison, NVIDIA
    3 sarda-devesh    Devesh Sarda          UW–Madison (profile)
  ```
- Split of the 1,111 human commits: NVIDIA-listed maintainers 1,019 (91.7%), UW–Madison 47 (4.2%), other or unknown 45 (4.1%, 19 logins). NVIDIA's share is at least 80% in every month from Jan 2026 on. — computed from the [commits API](https://api.github.com/repos/sirius-db/sirius/commits?since=2025-10-28T00:00:00Z) and [MAINTAINERS.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/MAINTAINERS.md) (accessed 2026-10-28)
- Some affiliations conflict. Rodrigo Aramburu's GitHub profile says "Voltron Data", and Joshua Patterson's says "Voltron Data, Former Nvidia". MAINTAINERS.md lists both as NVIDIA, and the Dec 2025 NVIDIA blog bio says Rodrigo Aramburu "leads Developer Relations for accelerated data processing at NVIDIA". — [github.com/roaramburu](https://github.com/roaramburu), [github.com/datametrician](https://github.com/datametrician), [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
- Author affiliations on the Sirius paper ("Rethinking Analytical Processing in the GPU Era", arXiv 2508.04701, v1 2025-08-06, v3 2025-12-20; CIDR 2026):
  - Bobbi Yogatama: NVIDIA
  - Yifei Yang, Kevin Kristensen, Devesh Sarda, Abigale Kim, Xiangyao Yu: UW–Madison
  - Adrian Cockcroft: OrionX
  - Yu Teng: no affiliation listed
  - Joshua Patterson, Gregory Kimball: NVIDIA
  - Wes McKinney: Posit PBC
  - Weiwei Gong: Oracle

  Funding note in the paper: "supported (in part) by the Lambda Cloud Credits and NVIDIA Data Science Cloud Credits". — [arXiv HTML v3](https://arxiv.org/html/2508.04701v3); [arXiv abs](https://arxiv.org/abs/2508.04701); [CIDR 2026 page](https://vldb.org/cidrdb/2026/rethinking-analytical-processing-in-the-gpu-era.html) (accessed 2026-10-28)

### Inferences
- The 92% figure applies current affiliations backwards. Some early-window commits, for example Kevin Kristensen's while he may still have been at UW, could be miscounted. Even with generous corrections NVIDIA's share stays above 85% (estimate).
- Many NVIDIA maintainers (the Aramburus, Malpica, Ocsa, Patterson, Wendt, Brobbel, Peltenburg, Hoozemans) are known from the BlazingSQL, Voltron Data and RAPIDS ecosystems. This looks like an NVIDIA data-processing team staffed onto Sirius, not volunteers (estimate; based on names and profile hints only, not confirmed).
- The "other" contributors cluster in scan, Paimon and DuckLake work (buzzcrow, ye-yi-yy, realAaronWu) and in Doris (morningman). Their employers are unknown.

### Gaps
- Employers of buzzcrow, ye-yi-yy, realAaronWu and jjohnson-99 are not on their GitHub profiles. They could be NVIDIA contractors or vendor engineers.
- The employer behind morningman's Doris work is not confirmed. His profile says only "Apache Doris PMC Chair".
- Abel Brown is listed as maintainer `infblueocean`, but an account `abelatnvidia` also exists. Not reconciled.

## 3. Releases, licence, CLA/DCO, governance, roadmap

### Takeaway
Sirius has no versioned releases. It publishes only two rolling, mutable GitHub releases (`stable` and `latest`, both started 2026-10-02 and marked pre-release), and the CMake project version is 0.0.0. The licence is plain Apache-2.0 with no CLA, no DCO and no copyright assignment. Governance is minimal lazy consensus among maintainers, with a stated aim of moving to a neutral foundation. In practice NVIDIA staff hold 20 of 23 votes. There is no published roadmap document. Direction lives in tracking issues.

### Cited Findings
- **Releases and tags.** The only tags are `stable` and `latest`. Two releases, both `prerelease: true`:
  - `stable`: published 2026-10-02T20:43Z, commit 8d17afe
  - `latest`: published 2026-10-02T21:20Z, commit 5350b8f

  — [releases API](https://api.github.com/repos/sirius-db/sirius/releases); [tags](https://github.com/sirius-db/sirius/tags) (accessed 2026-10-28)
- RELEASING.md says: "Neither is a versioned release; both are mutable snapshots that get replaced in place". `stable` is maintainer-picked, usually the latest commit that passed a nightly benchmark suite "external to this repo". `latest` auto-publishes on every successful build of `main`, and the README warns it is "possible to encounter breaking changes". — [docs/RELEASING.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/RELEASING.md); [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-28)
- CMake `project(... VERSION 0.0.0)`. — [CMakeLists.txt](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/CMakeLists.txt) (accessed 2026-10-28)
- The `stable` release is built against DuckDB v1.5.6. Its notes say: "Sirius extensions must match the exact DuckDB version they were built against". `main`, meanwhile, pins the DuckDB submodule to branch `v2.0-cyanoptera`, and docs say "the v2 extension requires the exact DuckDB revision pinned in this repository". — [release `stable`](https://github.com/sirius-db/sirius/releases/tag/stable); [.gitmodules](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.gitmodules); [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-28)
- **Licence.** Apache License 2.0, 201-line standard text, no NOTICE file. Source headers read "Copyright 2025, Sirius Contributors" (or 2026) followed by the Apache header. — [LICENSE](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/LICENSE); [src/config.cpp](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/config.cpp) (accessed 2026-10-28)
- Licence history: until 2025-06-25 the LICENSE file was the MIT text from the DuckDB extension template ("Copyright 2018-2024 Stichting DuckDB Foundation"). Commit c9c0d81f by bwyogatama replaced it with Apache-2.0. — [commit c9c0d81f](https://github.com/sirius-db/sirius/commit/c9c0d81f) (accessed 2026-10-28)
- **CLA/DCO.** CONTRIBUTING.md, `.github/` and the workflows (check, test, validate, distribution, release, docs, experimental, auto-assign) do not mention a CLA, DCO or sign-off. Only 148 of 1,244 window commits (12%) carry `Signed-off-by`, which points to a voluntary habit, not enforcement. — [CONTRIBUTING.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/CONTRIBUTING.md); [.github/workflows](https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.github/workflows) (accessed 2026-10-28)
- **GOVERNANCE.md**, added 2026-07-24 by yxymit in PR #1213 as "initial drafts". Key points:
  - "Governance is intentionally minimal and will formalize as the project grows, with the goal of eventually moving under a neutral open source foundation."
  - Maintainers "approve and merge contributions, make technical decisions, manage the maintainer base … decide by lazy consensus, falling back to a majority vote of maintainers".
  - New maintainers need a nomination issue and, after 7 days, support from 2 maintainers other than the nominator.
  - There is no steering committee, no company-neutrality rule and no cap on per-employer seats.

  — [GOVERNANCE.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/GOVERNANCE.md); [PR #1213](https://github.com/sirius-db/sirius/pull/1213) (accessed 2026-10-28)
- Merge rules: "Approval from any maintainer is sufficient to merge". CODEOWNERS routes PRs to teams under `@sirius-db/sirius-*`: core, io, integrations, telemetry, cmake, build, docs, rust and python. Stacked PRs and direct pushes to the repo are restricted to maintainers. Coordination happens in a Slack workspace (`#sirius`). — [CONTRIBUTING.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/CONTRIBUTING.md); [.github/CODEOWNERS](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.github/CODEOWNERS) (accessed 2026-10-28)
- **Roadmap.** No milestones exist (the milestones API is empty), Discussions are disabled (HTTP 410), there are no pinned issues and there is no ROADMAP file. Direction appears in tracking issues:
  - #545 "decouple Sirius internal from DuckDB" (open since 2026-04-02)
  - #1728 "Sirius as a standalone library" (open, 2026-09-08)
  - #1734 "Package Sirius libraries and the DuckDB extensions separately" (open)
  - #1807 "Move exchange into Sirius" (distributed, NIXL)
  - #1760 table-aware scan framework (Iceberg, DuckLake, Paimon)
  - #2199 StarRocks CN runtime filters
  - #2025 Apache Doris backend roadmap

  — [issues #545](https://github.com/sirius-db/sirius/issues/545), [#1728](https://github.com/sirius-db/sirius/issues/1728), [#1734](https://github.com/sirius-db/sirius/issues/1734), [#1807](https://github.com/sirius-db/sirius/issues/1807), [#1760](https://github.com/sirius-db/sirius/issues/1760), [#2199](https://github.com/sirius-db/sirius/issues/2199), [#2025](https://github.com/sirius-db/sirius/issues/2025) (accessed 2026-10-28)
- docs/README.md says: "Sirius currently supports DuckDB and Starrocks (coming soon), other systems marked with * are on our roadmap." Experimental Doris and StarRocks backends live in `experimental/`. — [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md); [experimental/doris/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/doris/README.md) (accessed 2026-10-28)
- The NVIDIA blog (Dec 2025) gave this roadmap:
  - spilling beyond GPU memory
  - GPU-native file readers with prefetching
  - a pipeline-oriented execution model
  - multi-node, multi-GPU execution

  Most of these have since shipped or are in progress (out-of-core execution, multi-GPU and exchange work). — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/); [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-28)

### Inferences
- Under GOVERNANCE.md, NVIDIA employees can make every decision on their own: lazy consensus or a majority with 20 of 23 votes, plus maintainer nominations. Calling it "project-governed" is formally true, but control is effectively NVIDIA's.
- Apache-2.0 with no CLA means a fork or downstream product needs no permission. The patent grant in Apache-2.0 §3 covers contributors' patents in their contributions.

### Gaps
- No public steering, foundation timeline or trademark policy for the name "Sirius" was found.

## 4. Issue and PR responsiveness

### Takeaway
External PRs get attention and are mostly merged. In the last 6 months the median first response was about 17 h and 31 of 38 closed external PRs were merged (82%). External issues are triaged slowly and rarely closed: median first response about 4.5 days, and 28 of 31 opened in the last 6 months are still open. Internally, issues act as a work tracker, not a support channel.

### Cited Findings
- Sample: all 591 issues and 1,763 PRs, fetched via GraphQL; the analysis covers items created on or after 2025-10-28. "External" means the author is not in MAINTAINERS.md, not a bot, and has `authorAssociation` other than MEMBER or OWNER. "First response" is the earliest comment or review by someone other than the author and not a bot, taken from the first 15 comments; for issues it also counts label, assign or close events. — [GitHub GraphQL API, repository(sirius-db/sirius)](https://github.com/sirius-db/sirius) (accessed 2026-10-28; `ghraw/issues_gql.json`, `ghraw/prs_gql.json`, `ghraw/resp_summary.txt`)
- Results:

  ```
  Group (created ≥2025-10-28)       n     first response   time to close      outcome
                                          median / p75     median
  External PRs (12 mo)              77    35.4 h / 191 h   204 h (8.5 d)      47 merged, 18 closed unmerged, 12 open; 12 never responded
  External PRs (last 6 mo)          49    16.7 h / 79 h    112 h (4.7 d)      31 merged, 7 closed unmerged, 11 open; 8 never responded
  Internal PRs (non-bot)         1,499    10.8 h / 71 h    36 h               1,015 merged, 362 closed unmerged, 122 open
  Dependabot PRs                   144    1.7 h            13 h               133 merged
  External issues (12 mo)           42    108 h / 200 h    695 h (29 d; n=14) 28 open; 9 never responded
  External issues (last 6 mo)       31    108 h / 183 h    418 h (n=3)        28 open; 9 never responded
  Internal issues                  500    264 h / 841 h    291 h              188 open
  ```
- Substantial external PRs are being merged. Examples:
  - #1842 morningman: Doris pseudo-BE, +37,498 lines, merged
  - #2314 ye-yi-yy: +8,756/−1,892, merged
  - #1956 buzzcrow: +9,971, merged
  - #1993 jjohnson-99: UNION on GPU, merged
  - #2323 paul841029: TPC-DS Q41 joins, merged 2026-10
  - #517 liaowanyou: first-time contributor, merged

  Rejected or stalled examples:
  - #1153: AMD ROCm/HIP backend, closed
  - #567: MatrixOrigin/MatrixOne doc mention, closed
  - #2304: 34k-line vector join, closed
  - #411: ASOF join, open since 2026-03

  — [R pulls](https://github.com/sirius-db/sirius/pulls?q=is%3Apr+author%3Amorningman) and the PR numbers above, e.g. [#1842](https://github.com/sirius-db/sirius/pull/1842), [#1153](https://github.com/sirius-db/sirius/pull/1153) (accessed 2026-10-28)

### Inferences
- The "external" bucket includes some likely NVIDIA staff who are not maintainers (abelatnvidia, matal-nvidia, quasiben, and probably kingcrimsontianyu of KvikIO and tarang-jain of cuVS; estimate). Truly outside contributors' figures are slightly worse than shown.
- Large outside contributions are accepted when they fit NVIDIA's direction, as the Doris backend did. A non-NVIDIA hardware backend (ROCm) was not.
- The project's own `sirius-stats` dashboard tracks time-to-first-review p50/p90/p95 and zero-review rate. This shows maintainers pay attention to review latency. — [sirius-db.com/sirius-stats](https://www.sirius-db.com/sirius-stats/) (accessed 2026-10-28; numbers did not render in fetch)

### Gaps
- Times use calendar hours, not business hours.
- Comments beyond the first 15 per item were not fetched. This has little effect on first-response times.

## 5. NVIDIA's stance and commercialisation

### Takeaway
NVIDIA's public framing (Dec 2025) was that Sirius is a UW–Madison-led open-source project "with support from NVIDIA" that showcases CUDA-X, cuDF and RMM. No NVIDIA product page, price or commercial SKU was found. In practice NVIDIA now staffs and controls the project. I found no public NVIDIA statement that it will not compete or sell, and the search was cut short.

### Cited Findings
- NVIDIA Technical Blog, 2025-12-15, "NVIDIA CUDA-X Powers the New Sirius GPU Engine for DuckDB, Setting ClickBench Records". Authors: Xiangyao Yu (UW), Bobbi Yogatama (NVIDIA), Yifei Yang (UW) and Rodrigo Aramburu (NVIDIA DevRel). It says Sirius was "Developed by University of Wisconsin-Madison with support from NVIDIA engineers" and "Led by the University of Wisconsin-Madison with support from NVIDIA", that "Sirius is open source with the permissive Apache License 2.0", and that it is built on cuDF and RMM. There is no mention of a commercial offering. — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
- The project site sirius-db.com lists no company, pricing or support offering. Its contact is siriusdb@cs.wisc.edu, a UW address. It does not mention NVIDIA. Its goal statement: "Sirius targets 100× speedups over CPU-based SQL engines." — [sirius-db.com](https://sirius-db.com) (accessed 2026-10-28)
- The repo's research page describes "Joint development between NVIDIA and UW–Madison, alongside various external contributors" and lists the Apache 2.0 licence as letting users "fork the architecture without legal friction, in either academic or industrial settings". — [docs/sirius-for-research.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/sirius-for-research.md) (accessed 2026-10-28)
- The README frames the engine as "Built on NVIDIA CUDA-X libraries including cuDF, cuVS, and cuCascade". cuCascade is pulled in as a submodule from github.com/NVIDIA/cuCascade. — [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md); [.gitmodules](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.gitmodules) (accessed 2026-10-28)
- Ecosystem integrations, not commercial:
  - The Apache Doris PMC chair (morningman) contributed the Doris backend, PR #1842 merged 2026-09; tracking issue #2025.
  - StarRocks compute-node integration is driven by NVIDIA maintainer aocsa (#2037, #2199).
  - A MatrixOrigin contributor tried to add a MatrixOne mention (#567, closed).
  - FanRuan staff opened a PR (#210, closed).

  — [#1842](https://github.com/sirius-db/sirius/pull/1842), [#2025](https://github.com/sirius-db/sirius/issues/2025), [#2199](https://github.com/sirius-db/sirius/issues/2199), [#567](https://github.com/sirius-db/sirius/pull/567) (accessed 2026-10-28)
- The arXiv paper reports 8.3× (TPC-H) and 7.4× (ClickBench) better cost efficiency than CPU engines with DuckDB, and up to 12.5× with Doris. — [arXiv 2508.04701](https://arxiv.org/abs/2508.04701) (accessed 2026-10-28)

### Inferences
- NVIDIA's business pattern here looks like RAPIDS and cuDF: open-source software that sells GPUs, with integrations into existing engines (DuckDB, Doris, StarRocks) instead of a standalone database product (estimate). That fits the claim that NVIDIA does not compete in this field, but nothing found here makes it a binding commitment.
- The direction of work is building Sirius as a standalone library (#1728), packaging libsirius separately (#1734), distributed exchange (#1807) and backends for Doris and StarRocks. That makes Sirius a general GPU execution engine that any vendor, including NVIDIA's partners, can embed. A small project building "a GPU SQL engine product" on Sirius would be competing with whatever Doris, StarRocks or cloud vendors build on the same core (estimate).

### Gaps
- No GTC 2026 session, press release or NVIDIA executive statement about Sirius was found. Web search was unavailable (budget exhausted), so this is a gap in the search, not proof that none exists.
- No statement found either way on whether NVIDIA will sell or support Sirius.
- No startup or cloud vendor publicly commercialising Sirius was found. VeloDB, CelerData and other vendor plans are unverified.
- Voltron Data's status, and whether NVIDIA hired its team (several maintainers' profiles still say "Voltron Data"), could not be verified: voltrondata.com failed DNS resolution.

## 6. Fork feasibility and building on top

### Takeaway
A fork is legally trivial but practically very hard to keep up for 1–3 people.
- About 1.1 M lines were added or removed on `main` in the last 6 months, against a current source plus test tree of about 470 k lines.
- The legacy execution engine was deleted in Sep 2026, with −39 k lines in one PR.
- The DuckDB dependency is moving from v1.5 to v2.
- There are no versions or API-stability promises.

Building on top as a dependency is also immature: the standalone library and packaging are open tracking issues, and the public C-ABI is about 330 lines of headers. The realistic options are to contribute upstream and pin SHAs, or to take a one-time hard fork and accept divergence.

### Cited Findings
- `stats/code_frequency`, weekly additions and deletions on the default branch:
  - 12 months (weeks from 2025-10-26): +1,036,447 / −472,451
  - 6 months (weeks from 2026-04-26): +780,471 / −320,285
  - all time: +1,562,579 / −774,076
  - Last 4 weeks alone: +243k / −65k

  — [stats/code_frequency](https://api.github.com/repos/sirius-db/sirius/stats/code_frequency) (accessed 2026-10-28; `ghraw/stats_code_frequency.json`)
- Current tree at b9e05ffc, lines of C++/CUDA/Rust/Python:
  - `src/` ≈ 215.8 k
  - `test/` ≈ 251.6 k
  - `experimental/` ≈ 33.5 k
  - `rust/` ≈ 7.2 k
  - `include/` ≈ 0.6 k

  — local clone measured with find/wc; [tree](https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619) (accessed 2026-10-28)
- Engine replacement: PR #1055, "remove the legacy `gpu_processing` execution path" by mbrobbel, opened 2026-07-02 and merged 2026-09-30, +65 / −39,427 lines across 214 files. The README now points to the `legacy-dev` archive pinned at f79a6f42 for "the legacy Sirius implementation used for the ClickBench results". The new engine is documented as "Super Sirius" in docs/super-sirius/. — [PR #1055](https://github.com/sirius-db/sirius/pull/1055); [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-28)
- DuckDB coupling:
  - main tracks DuckDB `v2.0-cyanoptera` while `stable` is on 1.5.6.
  - UPDATING.md is a multi-step manual procedure for bumping DuckDB across two submodules, pixi, CI and Makefiles.
  - building.md says the DuckDB dependency "is a build-only contract, not an installed Sirius target or a stable DuckDB ABI", that "Decoupling the library's and extension's DuckDB versions requires a separate API/ABI change", and that "Consumers are responsible for DuckDB and C++ runtime ABI compatibility".
  - Issue #545 (decouple from DuckDB) is still open.

  — [docs/UPDATING.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/UPDATING.md); [docs/building.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/building.md); [#545](https://github.com/sirius-db/sirius/issues/545) (accessed 2026-10-28)
- Extension points:
  - Substrait plan input (`Context::execute_substrait`, used by the Doris and StarRocks backends).
  - A public C-ABI in `include/sirius/ffi.hpp` (216 lines; 334 lines across the public headers).
  - Rust crates `sirius-sys` (cxx bindings) and `sirius` (safe wrapper).
  - An installable CMake package `sirius::sirius` whose "public headers do not require CUDA or DuckDB headers".

  The Doris README says "the shared libSirius Rust boundary is being designed". Issue #2025 plans to move the Doris backend "from the `ffi` APIs to the `sirius` crate on the #1728 API: cancellation and progress through its execution handle … the resource management API (#840) and the libsirius packages (#1734)". — [rust/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/rust/README.md); [docs/building.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/building.md); [experimental/doris/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/experimental/doris/README.md); [#2025](https://github.com/sirius-db/sirius/issues/2025); [#1728](https://github.com/sirius-db/sirius/issues/1728) (accessed 2026-10-28)
- There is no semver, deprecation policy or API-stability statement anywhere in docs/, CONTRIBUTING.md or the Rust README; a grep for semver, "stable API" and "backward compat" found nothing relevant. A `! - breaking` label exists. — [labels](https://github.com/sirius-db/sirius/labels); [docs/](https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs) (accessed 2026-10-28)
- Development is heavily AI-agent-assisted: AGENTS.md, a `.claude/` directory with a `sirius-db/claude-tools` submodule, and a `sirius-fuzz` agent skill. — [AGENTS.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/AGENTS.md); [.gitmodules](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.gitmodules) (accessed 2026-10-28)
- Hard runtime requirements: an NVIDIA Turing+ GPU, CUDA 12/13, io_uring and O_DIRECT. The engine is built on cuDF, RMM and NVIDIA cuCascade, which makes it NVIDIA-only by design. — [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-28)

### Inferences
- **Tracking upstream in a fork.** Upstream moves about 50–60 k changed lines per week recently, with about 15 full-time-equivalent NVIDIA engineers plus AI agents (estimate). A 1-person + 2-weekend team cannot rebase a divergent fork onto that stream. Conflicts in the planner, executor and scan layers, which are exactly where the churn is, would dominate (estimate). This confirms the presumption, with evidence.
- **One-time hard fork.** This is feasible legally and technically: Apache-2.0, no CLA, and a self-contained build via pixi. The team would then own about 216 k lines of engine code plus a large CUDA dependency stack (cuDF, RMM, cuCascade, DuckDB v2 submodules) with no upstream fixes. That is realistic only if the product uses a narrow, frozen slice (estimate).
- **Building on top as a dependency.** This is the most viable path, but it is early.
  - Integrate through Substrait plans plus the `ffi.hpp` C-ABI or the `sirius` Rust crate.
  - Pin `main` SHAs, because the rolling `stable`/`latest` tags are mutable.
  - Expect breaking changes until #1728 and #1734 land.
  - Contribute missing operators upstream, where external PRs are merged with a median time to close of about 5 days over the last 6 months.
- Competitive overlap is the main risk. Whatever thin layer peacockdb builds, for example a Substrait front end or distribution, NVIDIA's own team is building too (exchange #1807, standalone library #1728, Doris and StarRocks backends) (estimate).

### Gaps
- `code_frequency` counts all files, including lockfiles (pixi.lock), test fixtures and snapshots. Engine-only churn is lower; it was not computed because the clone is shallow.
- No per-directory churn over 6 months was computed for the same reason. A full clone and `git log --numstat` would give it.
