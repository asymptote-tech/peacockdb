//! The checks one timed section answers to, and the fields it must carry, over a literal section
//! as well as the committed trees. The H200's tree carries no batches and no load, and the H100's
//! passes every check, so only a literal can show those checks refusing a section.

use peacockdb_core::prototypes::{FUSED_ALL_RUNS, FusedSection, parse_fused_section};
use peacockdb_core::test_support::{BenchSection, MEASURED_RUNS, parse_benchmark_section};

/// The device trees whose sections carry no batches and no load, execute, peak, cache or
/// prefill keys: written before the harness wrote them. Only the H200's, as `PREDATES_PROVENANCE`.
pub(crate) const PREDATES_ROWS: &[&str] = &["h200"];

/// The trailer keys every section outside `PREDATES_ROWS` carries beside its batches.
const ROWS_KEYS: [&str; 7] = [
    "load_us",
    "execute_us",
    "peak_bytes",
    "pinned_cache",
    "prefill_files",
    "prefill_bytes",
    "prefill_us",
];

/// The first field a timed section under `device` lacks, outside `PREDATES_ROWS`: an old
/// binary's output, which the page and the later readers cannot use.
pub(crate) fn rows_problem(device: &str, body: &str, at: &str) -> Result<(), String> {
    let Some(section) = parse_benchmark_section(body) else {
        return Ok(());
    };
    if PREDATES_ROWS.contains(&device) {
        return Ok(());
    }
    let lacking = |field: &str| {
        format!(
            "{at}: carries no {field}, and only {PREDATES_ROWS:?} were written before the \
             harness wrote batches and the load"
        )
    };
    for node in &section.nodes {
        for (field, lists) in [
            ("batch_rows", &node.batch_rows),
            ("batch_bytes", &node.batch_bytes),
        ] {
            if lists.is_none() {
                return Err(lacking(&format!("{field} under {}", node.line.name)));
            }
        }
    }
    match ROWS_KEYS.iter().find(|key| section.field(key).is_none()) {
        Some(key) => Err(lacking(key)),
        None => Ok(()),
    }
}

/// The first way a timed section contradicts itself. Every `total_us` is the sum of the
/// `time_us` beside it, and `device_us` the sum of those totals. Where the section carries them,
/// `load_us` is the loaders' totals, `load_us + execute_us` is `device_us`, each node's
/// `batch_rows` and `batch_bytes` have one shape, and `peak_bytes` is at least every batch,
/// since the accountant holds every batch. `run_us` is the second-smallest of the `runs`. A `0`
/// entry is a call that opened no region; a `1` is a region the clock rounded down.
pub(crate) fn section_problem(section: &BenchSection, at: &str) -> Result<(), String> {
    for node in &section.nodes {
        let name = node.line.name;
        let entries: u64 = node.time_us.iter().flatten().sum();
        if node.total_us != entries {
            return Err(format!(
                "{at}: {name}'s total_us={} is not the sum of the time_us beside it, {entries}",
                node.total_us
            ));
        }
        let shape = |lists: &Option<Vec<Vec<u64>>>| {
            lists
                .as_ref()
                .map(|lanes| lanes.iter().map(Vec::len).collect::<Vec<_>>())
        };
        if shape(&node.batch_rows) != shape(&node.batch_bytes) {
            return Err(format!(
                "{at}: {name}'s batch_rows and batch_bytes differ in shape"
            ));
        }
    }
    let device_us = section.count("device_us");
    if device_us != Some(section.tree_device_us()) {
        return Err(format!(
            "{at}: the trailer's device_us={device_us:?} is not the sum of the tree's total_us, {}",
            section.tree_device_us()
        ));
    }
    if let Some(load_us) = section.count("load_us") {
        let loaders = section.tree_load_us();
        if load_us != loaders {
            return Err(format!(
                "{at}: load_us={load_us} is not the loaders' total_us, {loaders}"
            ));
        }
        if section.count("execute_us").map(|execute| load_us + execute) != device_us {
            return Err(format!("{at}: load_us + execute_us is not device_us"));
        }
    }
    if let Some(peak) = section.count("peak_bytes") {
        let batches = section
            .nodes
            .iter()
            .flat_map(|node| node.batch_bytes.iter());
        let largest = batches.flatten().flatten().max().copied().unwrap_or(0);
        if peak < largest {
            return Err(format!(
                "{at}: peak_bytes={peak} is below a batch of {largest} bytes, which the \
                 accountant held"
            ));
        }
    }
    let runs = section
        .field("runs")
        .ok_or_else(|| format!("{at}: the trailer has no runs"))?;
    let spread = runs
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|e| {
            e.parse()
                .map_err(|_| format!("{at}: runs entry {e:?} is not a number"))
        })
        .collect::<Result<Vec<u64>, String>>()?;
    if spread.len() != MEASURED_RUNS {
        return Err(format!(
            "{at}: the spread holds {} executions",
            spread.len()
        ));
    }
    // The reported run is the second-smallest of the spread, which is the whole reason the
    // spread is written: the file would otherwise state one number with nothing saying which.
    let mut sorted = spread.clone();
    sorted.sort_unstable();
    if section.count("run_us") != Some(sorted[1]) {
        return Err(format!(
            "{at}: run_us is not the second-smallest of {spread:?}"
        ));
    }
    Ok(())
}

/// What is wrong with one fused section, if anything. Every section: its hold lines agree
/// with `buffers`, `rereads` and `buffer_bytes`, and the cap fits its budget unless it says it
/// does not. A measured one also: bench-rows' tree arithmetic over each node once, the runs
/// (`FUSED_ALL_RUNS` for `all`, `MEASURED_RUNS` otherwise) with `run_us` their second-smallest,
/// the merge entries largest first, the pipelined estimate from the measured run, `Σ reads` the
/// tree's loads and its `GpuHoldBuffer`s the buffers.
pub(crate) fn fused_problem(section: &FusedSection, batch: &str, at: &str) -> Result<(), String> {
    let count = |key: &str| {
        section
            .count(key)
            .ok_or_else(|| format!("{at}: no `{key}`"))
    };
    let hold_field = |line: &str, key: &str| -> String {
        let after = line.split(&format!(" {key}=")).nth(1).unwrap_or("");
        after.split(' ').next().unwrap_or("").to_string()
    };
    let chose = |choice: &str| {
        section
            .holds
            .iter()
            .filter(|line| hold_field(line, "choice") == choice)
            .collect::<Vec<_>>()
    };
    let (buffers, rereads) = (chose("buffer"), chose("re-read"));
    if buffers.len() as u64 != count("buffers")? || rereads.len() as u64 != count("rereads")? {
        return Err(format!(
            "{at}: the hold lines disagree with buffers= or rereads="
        ));
    }
    let kept: u64 = buffers
        .iter()
        .map(|line| hold_field(line, "bytes").parse::<u64>().unwrap_or(u64::MAX))
        .sum();
    if kept != count("buffer_bytes")? {
        return Err(format!(
            "{at}: buffer_bytes is not its buffer lines' bytes, {kept}"
        ));
    }
    if section.field("over_budget") != Some("true")
        && count("estimate_bytes")? > count("budget_bytes")?
    {
        return Err(format!(
            "{at}: the estimate is over the budget and over_budget says not"
        ));
    }
    if section.field("outcome") != Some("measured") {
        return Ok(());
    }
    let (load, execute, device) = (count("load_us")?, count("execute_us")?, count("device_us")?);
    if section.tree_device_us() != device
        || section.tree_load_us() != load
        || load + execute != device
    {
        return Err(format!(
            "{at}: the tree does not add up to device_us, load_us and execute_us"
        ));
    }
    for node in &section.nodes {
        let calls: u64 = node.time_us.iter().flatten().sum();
        if calls != node.total_us {
            return Err(format!(
                "{at}: {}'s total_us is not its time_us",
                node.line.name
            ));
        }
    }
    let spread: Vec<u64> = section
        .field("runs")
        .ok_or_else(|| format!("{at}: no runs"))?
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|e| e.parse().map_err(|_| format!("{at}: runs entry {e:?}")))
        .collect::<Result<_, String>>()?;
    let wanted = if batch == "all" {
        FUSED_ALL_RUNS
    } else {
        MEASURED_RUNS
    };
    let mut sorted = spread.clone();
    sorted.sort_unstable();
    if spread.len() != wanted || section.count("run_us") != sorted.get(1).copied() {
        return Err(format!(
            "{at}: runs {spread:?} are not {wanted} with run_us the second-smallest"
        ));
    }
    let savings: Vec<u64> = section
        .merge
        .iter()
        .filter_map(|line| line.strip_prefix("entry saved_us="))
        .map(|rest| rest.split(' ').next().unwrap_or("").parse().unwrap_or(0))
        .collect();
    if savings.windows(2).any(|pair| pair[0] < pair[1]) {
        return Err(format!("{at}: the merge entries are not largest first"));
    }
    if count("fused_measured_us")? != load + execute
        || count("pipelined_estimate_us")? != (load + execute).saturating_sub(count("saved_us")?)
    {
        return Err(format!(
            "{at}: the pipelined estimate is not the measured run less the saving"
        ));
    }
    let reads: u64 = section
        .field("reads")
        .unwrap_or("[]")
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .rsplit(':')
                .next()
                .unwrap_or("")
                .parse::<u64>()
                .unwrap_or(0)
        })
        .sum();
    let named = |name: &str| section.nodes.iter().filter(|n| n.line.name == name).count() as u64;
    if reads != named("GpuLoadParquet") || named("GpuHoldBuffer") != count("buffers")? {
        return Err(format!(
            "{at}: reads= or buffers= is not what the tree holds"
        ));
    }
    Ok(())
}

/// A measured fused batch of two roots over one shared load: q1 reads it directly, q2 through a
/// buffer under a join that re-read nothing.
const FUSED: &str = "\
--- batch ---
queries=[q1,q2]
outcome=measured
shared=[lineitem:q1+q2]
unshared=[]
reads=[lineitem:1,orders:1]
buffers=1
buffer_bytes=32
rereads=0
hold query=q2 table=lineitem hold=GpuHashJoin choice=buffer above=4 bytes=32 reread_us=900 why=fits: 32 <= 900
cap=2
pool_bytes=1000
budget_bytes=900
estimate_bytes=500
over_budget=false
answers=match
--- merge ---
saved_us=10
entry saved_us=10 members=q1:1,q2:4 node=GpuFilter: predicate=x
--- pipeline ---
model=one loader and one executor
load_order=[lineitem,orders]
serial_us=300
makespan_us=250
saved_us=50
fused_measured_us=200
pipelined_estimate_us=150
--- tree ---
root 0:
  GpuUnload
    time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
    GpuFilter: predicate=x
      time_us=[[10]] total_us=10 batch_rows=[[4]] batch_bytes=[[32]]
      @1 GpuLoadParquet: table=lineitem
        time_us=[[100]] total_us=100 batch_rows=[[40]] batch_bytes=[[320]]
root 1:
  GpuUnload
    time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
    GpuHashJoin: on=x
      time_us=[[6,4]] total_us=10 batch_rows=[[3]] batch_bytes=[[24]]
      GpuLoadParquet: table=orders
        time_us=[[60]] total_us=60 batch_rows=[[5]] batch_bytes=[[40]]
      GpuHoldBuffer
        time_us=[[]] total_us=0 batch_rows=[[4]] batch_bytes=[[32]]
        GpuFilter: predicate=x
          time_us=[[10]] total_us=10 batch_rows=[[4]] batch_bytes=[[32]]
          @1
--- run ---
run_us=260
device_us=200
load_us=160
execute_us=40
peak_bytes=700
runs=[270,260,250,280,290,300,310,320,330,340]
";

fn fused_checked(body: &str, batch: &str) -> Result<(), String> {
    fused_problem(
        &parse_fused_section(body).expect("a measured batch"),
        batch,
        "doctored",
    )
}

#[test]
fn a_fused_section_that_adds_up_passes() {
    assert_eq!(fused_checked(FUSED, "first10"), Ok(()));
}

#[test]
fn a_fused_section_that_does_not_add_up_is_refused() {
    for (from, to) in [
        ("load_us=160", "load_us=161"),
        ("rereads=0", "rereads=1"),
        ("buffer_bytes=32", "buffer_bytes=33"),
        ("pipelined_estimate_us=150", "pipelined_estimate_us=151"),
        ("reads=[lineitem:1,orders:1]", "reads=[lineitem:2,orders:1]"),
        ("estimate_bytes=500", "estimate_bytes=901"),
    ] {
        assert!(FUSED.contains(from), "{from}");
        assert!(
            fused_checked(&FUSED.replace(from, to), "first10").is_err(),
            "{to} passed"
        );
    }
    assert!(
        fused_checked(FUSED, "all").is_err(),
        "all measures FUSED_ALL_RUNS"
    );
}

/// tpch q3 at tp1-single as the H100 measured it on 2026-11-05 (bench-rows-detail.md, Task 5):
/// three loaders under two joins. Each `partition_groups` list is cut to its first and last
/// group; nothing checked reads it.
const TIMED: &str = "\
GpuUnload
  time_us=[[118]] total_us=118 batch_rows=[[10]] batch_bytes=[[328]]
  GpuAccumulateBatchesAndSort: by=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last], fetch=10, lanes=1, batches=single, sorted_on=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last]
    time_us=[[152,20]] total_us=172 batch_rows=[[10]] batch_bytes=[[328]]
    GpuSort: by=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last], fetch=10, lanes=1, batches=single, sorted_on=[revenue@1 desc nulls_first, o_orderdate@2 asc nulls_last]
      time_us=[[636]] total_us=636 batch_rows=[[10]] batch_bytes=[[328]]
      GpuProject: exprs=[l_orderkey@0, `sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`@3 as revenue, o_orderdate@1, o_shippriority@2], lanes=1, batches=single
        time_us=[[2]] total_us=2 batch_rows=[[453784]] batch_bytes=[[14747980]]
        GpuAggregateBatches: group_by=[l_orderkey@0, o_orderdate@1, o_shippriority@2], aggs=[sum(`sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`@3) as `sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`], final=[`sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`@3], lanes=1, batches=single
          time_us=[[0,583]] total_us=583 batch_rows=[[453784]] batch_bytes=[[14747980]]
          GpuAggregate: group_by=[l_orderkey@2, o_orderdate@0, o_shippriority@1], aggs=[sum(l_extendedprice@3 * (1 - l_discount@4)) as `sum(lineitem.l_extendedprice * Int64(1) - lineitem.l_discount)`], lanes=1, batches=multiple
            time_us=[[1646,0]] total_us=1646 batch_rows=[[453784]] batch_bytes=[[14747980]]
            GpuHashJoin: join_type=Inner, on=[(o_orderkey@0, l_orderkey@0)], projection=[o_orderdate@1, o_shippriority@2, l_orderkey@3, l_extendedprice@4, l_discount@5], lanes=1, batches=multiple
              time_us=[[436,24241,0]] total_us=24677 batch_rows=[[1198277]] batch_bytes=[[58266221]]
              GpuCoalesceAllBatches: lanes=1, batches=single
                time_us=[[0,75]] total_us=75 batch_rows=[[5835860]] batch_bytes=[[95562209]]
                GpuHashJoin: join_type=Inner, on=[(c_custkey@0, o_custkey@1)], projection=[o_orderkey@1, o_orderdate@3, o_shippriority@4], lanes=1, batches=multiple
                  time_us=[[96,7084,0]] total_us=7180 batch_rows=[[5835860]] batch_bytes=[[95562209]]
                  GpuCoalesceAllBatches: lanes=1, batches=single
                    time_us=[[0,18]] total_us=18 batch_rows=[[1200419]] batch_bytes=[[9753405]]
                    GpuFilter: predicate=c_mktsegment@1 = BUILDING, projection=[c_custkey@0], lanes=1, batches=multiple
                      time_us=[[519]] total_us=519 batch_rows=[[1200419]] batch_bytes=[[9753405]]
                      GpuLoadParquet: table=customer, projections=[c_custkey@0, c_mktsegment@6], partition_groups=[[[0,48]]], lanes=1, batches=multiple
                        time_us=[[35540]] total_us=35540 batch_rows=[[6000000]] batch_bytes=[[127501181]]
                  GpuFilter: predicate=o_orderdate@2 < 1995-03-15, lanes=1, batches=multiple
                    time_us=[[1674]] total_us=1674 batch_rows=[[29150056]] batch_bytes=[[714176372]]
                    GpuLoadParquet: table=orders, projections=[o_orderkey@0, o_custkey@1, o_orderdate@4, o_shippriority@7], partition_groups=[[[0,237]]], lanes=1, batches=multiple
                      time_us=[[45060]] total_us=45060 batch_rows=[[29245440]] batch_bytes=[[716513280]]
              GpuFilter: predicate=l_shipdate@3 > 1995-03-15, projection=[l_orderkey@0, l_extendedprice@1, l_discount@2], lanes=1, batches=multiple
                time_us=[[8371]] total_us=8371 batch_rows=[[129398483]] batch_bytes=[[5224463753]]
                GpuLoadParquet: table=lineitem, projections=[l_orderkey@0, l_extendedprice@5, l_discount@6, l_shipdate@10], partition_groups=[[[900,1953]]], lanes=1, batches=multiple
                  time_us=[[122848]] total_us=122848 batch_rows=[[129420290]] batch_bytes=[[5759202908]]
--- run ---
run_us=249880
device_us=249119
load_us=203448
execute_us=45671
peak_bytes=5948138877
runs=[265479,251120,251474,251307,250587,249880,250096,249202,250998,250099]
build=release
allocator=rmm-pool initial=69.0GiB max=69.0GiB of 78.7GiB free on a discrete device
pinned_cache=on cap=147.4GiB resident=11.2GiB files=3 fills=3 fill_us=6306227 fallbacks=0 evictions=0 reloads=0 source=default
prefill_files=3
prefill_bytes=12051239504
prefill_us=6306272
device=NVIDIA H100 80GB HBM3
driver=580.173.02
cuda=12.2
cudf=25.02.02
host=computeinstance-e00e6yena4nzjgaym0
";

/// `TIMED` with its one `from` replaced by `to`.
fn mutated(from: &str, to: &str) -> String {
    assert_eq!(
        TIMED.matches(from).count(),
        1,
        "{from:?} is not in TIMED once"
    );
    TIMED.replacen(from, to, 1)
}

fn checked(body: &str) -> Result<(), String> {
    section_problem(
        &parse_benchmark_section(body).expect("a timed section"),
        "q3",
    )
}

#[test]
fn the_h100_q3_section_passes_every_check() {
    assert_eq!(checked(TIMED), Ok(()));
    assert_eq!(rows_problem("h100-80gb-hbm3", TIMED, "q3"), Ok(()));
}

#[test]
fn a_load_us_off_by_one_is_refused() {
    let problem = checked(&mutated("load_us=203448", "load_us=203449")).expect_err("passed");
    assert!(
        problem.contains("load_us=203449 is not the loaders' total_us"),
        "{problem}"
    );
}

#[test]
fn an_execute_us_off_by_one_is_refused() {
    let problem = checked(&mutated("execute_us=45671", "execute_us=45672")).expect_err("passed");
    assert!(problem.contains("load_us + execute_us"), "{problem}");
}

/// One byte below lineitem's loader's batch, the largest the accountant held.
#[test]
fn a_peak_below_a_batch_is_refused() {
    let body = mutated("peak_bytes=5948138877", "peak_bytes=5759202907");
    let problem = checked(&body).expect_err("passed");
    assert!(problem.contains("below a batch of 5759202908"), "{problem}");
}

/// orders' loader with its one batch's bytes gone: `batch_rows` still holds the batch.
#[test]
fn a_batch_bytes_entry_missing_is_refused() {
    let body = mutated("batch_bytes=[[716513280]]", "batch_bytes=[[]]");
    let problem = checked(&body).expect_err("passed");
    assert!(
        problem.contains("GpuLoadParquet's batch_rows and batch_bytes differ in shape"),
        "{problem}"
    );
}

/// Arithmetic alone passes it: only the presence check names the key.
#[test]
fn a_section_without_prefill_us_is_refused() {
    let body = mutated("prefill_us=6306272\n", "");
    assert_eq!(checked(&body), Ok(()));
    let problem = rows_problem("h100-80gb-hbm3", &body, "q3").expect_err("passed");
    assert!(problem.contains("carries no prefill_us"), "{problem}");
}
