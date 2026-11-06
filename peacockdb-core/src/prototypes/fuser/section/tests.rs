//! The fused format read back: a measured batch, an out-of-memory one, a marker, and a section
//! rendered from a fused plan reading back as it was written.

use crate::prototypes::fuser::tests::{baseline, fused_with};
use crate::prototypes::fuser::tests::{held_pair, re_read_every_hold};
use crate::prototypes::fuser::{Outcome, cap_for, cap_inputs, merge_hint, pipeline_hint};
use crate::prototypes::fuser::{pipeline_inputs, render_fused_section};
use crate::prototypes::parse_fused_section;

const MEASURED: &str = "\
--- batch ---
queries=[q1,q2]
outcome=measured
shared=[lineitem:q1+q2]
unshared=[]
reads=[orders:1,lineitem:2]
buffers=1
buffer_bytes=40
rereads=1
hold query=q2 table=lineitem hold=GpuHashJoin choice=buffer above=3 bytes=40 reread_us=900 why=fits: 40 <= 900
hold query=q1 table=lineitem hold=GpuHashJoin choice=re-read above=- bytes=0 reread_us=700 why=too large: 60 > 40
cap=2
pool_bytes=1000
budget_bytes=900
estimate_bytes=500
over_budget=false
answers=match
--- merge ---
saved_us=10
entry saved_us=10 members=q1:2,q2:4 node=GpuFilter: predicate=x, lanes=1
--- pipeline ---
model=one loader and one executor
load_order=[lineitem,orders]
serial_us=300
makespan_us=250
saved_us=50
fused_measured_us=400
pipelined_estimate_us=350
--- tree ---
root 0:
  GpuUnload
    time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
    GpuProject: exprs=[a]
      time_us=[[1]] total_us=1 batch_rows=[[1]] batch_bytes=[[17]]
      GpuFilter: predicate=x
        time_us=[[10]] total_us=10 batch_rows=[[4]] batch_bytes=[[32]]
        @1 GpuLoadParquet: table=lineitem
          time_us=[[100]] total_us=100 batch_rows=[[40]] batch_bytes=[[320]]
root 1:
  GpuUnload
    time_us=[[5]] total_us=5 batch_rows=[[1]] batch_bytes=[[17]]
    GpuHashJoin: on=x
      time_us=[[9]] total_us=9 batch_rows=[[3]] batch_bytes=[[24]]
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
runs=[260,250]
device=NVIDIA H100 80GB HBM3
";

const OUT_OF_MEMORY: &str = "\
--- batch ---
queries=[q1,q2]
outcome=out-of-memory
reason=std::bad_alloc: out_of_memory: RMM failure
shared=[lineitem:q1+q2]
unshared=[]
reads=[lineitem:1]
buffers=0
buffer_bytes=0
rereads=1
hold query=q1 table=lineitem hold=GpuHashJoin choice=re-read above=- bytes=0 reread_us=700 why=no edge: the reader takes the shared load directly
cap=1
pool_bytes=1000
budget_bytes=900
estimate_bytes=950
over_budget=true
--- merge ---
saved_us=0
--- pipeline ---
model=one loader and one executor
load_order=[lineitem]
serial_us=300
makespan_us=250
saved_us=50
--- run ---
device=NVIDIA H100 80GB HBM3
driver=580.173.02
cuda=12.2
cudf=25.02.02
host=computeinstance-e00e6yena4nzjgaym0
";

#[test]
fn a_measured_section_reads_back() {
    let section = parse_fused_section(MEASURED).expect("a measured batch");
    assert_eq!(section.count("cap"), Some(2));
    assert_eq!(section.field("outcome"), Some("measured"));
    assert_eq!(section.count("fused_measured_us"), Some(400));
    let (load, execute, device) = (
        section.count("load_us").unwrap(),
        section.count("execute_us").unwrap(),
        section.count("device_us").unwrap(),
    );
    assert_eq!(load + execute, device);
    assert_eq!(section.tree_load_us(), load, "the shared load counted once");
    assert_eq!(section.roots, [0, 4]);
    assert_eq!(section.nodes.len(), 9, "the second visit of @1 is no node");
    let filter = section.nodes[8].children.clone();
    assert_eq!(filter, [3], "it reads the shared load");
    assert_eq!(section.holds.len(), 2);
    assert!(section.holds[1].ends_with("why=too large: 60 > 40"));
    assert!(section.merge[1].ends_with("node=GpuFilter: predicate=x, lanes=1"));
    assert_eq!(section.pipeline.len(), 7);
}

#[test]
fn an_out_of_memory_section_reads_back_with_its_cap() {
    let section = parse_fused_section(OUT_OF_MEMORY).expect("a recorded outcome");
    assert!(section.nodes.is_empty());
    assert_eq!(section.field("outcome"), Some("out-of-memory"));
    assert_eq!(section.count("cap"), Some(1));
    let keys: Vec<&str> = section.trailer.iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, ["device", "driver", "cuda", "cudf", "host"]);
    assert_eq!(section.holds.len(), 1);
}

#[test]
fn a_marker_is_not_timed() {
    assert!(parse_fused_section("skipped: same set as all: the list holds 18 queries\n").is_none());
    assert!(parse_fused_section("status=failed reason=prefill: no room\n").is_none());
}

#[test]
#[should_panic(expected = "twice")]
fn a_key_held_twice_panics() {
    let twice = MEASURED.replace("answers=match\n", "answers=match\ncap=3\n");
    parse_fused_section(&twice).expect("parsed").field("cap");
}

#[test]
fn a_rendered_section_reads_back() {
    let trees = held_pair(&[0, 4], &[0, 5]);
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let times = baseline(&trees, |_, _| 3);
    let queries = vec!["q1".to_string(), "q2".to_string()];
    let decision = cap_for(&cap_inputs(&fused, &times, 1000));
    let merge = merge_hint(&fused, &times);
    let (loads, roots) = pipeline_inputs(&fused, &times);
    let pipeline = pipeline_hint(&loads, &roots);
    let run = "--- run ---\ndevice=d\ndriver=r\ncuda=c\ncudf=f\nhost=h\n";
    let outcome = Outcome::OutOfMemory {
        reason: "out of memory",
        run,
    };
    let text = render_fused_section(
        &queries, &fused, &decision, 1000, &merge, &pipeline, outcome,
    );
    let section = parse_fused_section(&text).expect("a recorded outcome");
    assert_eq!(section.field("queries"), Some("[q1,q2]"));
    assert_eq!(section.field("shared"), Some("[lineitem:q2]"));
    assert_eq!(section.field("reads"), Some("[orders:1,lineitem:2]"));
    assert_eq!(section.count("rereads"), Some(1));
    assert_eq!(section.count("buffers"), Some(0));
    assert_eq!(section.holds.len(), 1);
    assert!(section.holds[0].starts_with("hold query=q1 table=lineitem hold=GpuHashJoin"));
    assert!(section.holds[0].ends_with("why=test: re-read"));
    assert_eq!(section.count("cap"), Some(decision.cap.get() as u64));
    assert_eq!(section.field("over_budget"), Some("false"));
    assert_eq!(section.count("pool_bytes"), Some(1000));
    assert_eq!(section.count("serial_us"), Some(pipeline.serial_us));
    assert_eq!(section.field("fused_measured_us"), None);
    assert_eq!(section.trailer.len(), 5);
}

#[test]
fn a_rendered_measured_section_reads_back() {
    let trees = held_pair(&[0, 4], &[0, 4]);
    let fused = fused_with(&trees, &mut re_read_every_hold);
    let times = baseline(&trees, |_, _| 3);
    let queries = vec!["q1".to_string(), "q2".to_string()];
    let decision = cap_for(&cap_inputs(&fused, &times, 1000));
    let merge = merge_hint(&fused, &times);
    let (loads, roots) = pipeline_inputs(&fused, &times);
    let pipeline = pipeline_hint(&loads, &roots);
    let tree = "GpuUnload\n  time_us=[[1]] total_us=1 batch_rows=[[1]] batch_bytes=[[8]]\n";
    let run = "--- run ---\nrun_us=9\ndevice_us=1\nload_us=0\nexecute_us=1\n";
    let outcome = Outcome::Measured {
        tree,
        measured_us: 1 + pipeline.saved_us,
        run,
    };
    let text = render_fused_section(
        &queries, &fused, &decision, 1000, &merge, &pipeline, outcome,
    );
    let section = parse_fused_section(&text).expect("a measured batch");
    assert_eq!(section.field("outcome"), Some("measured"));
    assert_eq!(section.field("answers"), Some("match"));
    assert_eq!(section.count("pipelined_estimate_us"), Some(1));
    assert_eq!((section.roots.clone(), section.nodes.len()), (vec![0], 1));
    assert_eq!(section.count("run_us"), Some(9));
}
