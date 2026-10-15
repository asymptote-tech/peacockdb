"""Every join mode the engine can be asked for, against an oracle written from SQL.

The join is the one operator whose state has to survive a call, so each mode runs with its
probe side arriving in batches, at every batching and partitioning config, on the pandas
backend (`operators/joins.py`), which answers each call as the device's join session does:
one answer per probe batch, and a finish for the types that owe build rows. The matrix is
`join_types.py`'s two facts — every probe streams, five types finish — and nothing is
refused.

Split out of `test_end_to_end.py`, which it shares helpers with: same drivers, same
oracle-comparison rules, same budget. The join block simply outgrew that file.
"""

from __future__ import annotations

if __package__ in (None, ""):  # allow `python scripts/exec_model/tests/<file>.py`
    import pathlib as _pathlib, sys as _sys

    _sys.path.insert(0, str(_pathlib.Path(__file__).resolve().parents[3]))
    __package__ = "scripts.exec_model.tests"

import pathlib
import re

import numpy as np
import pandas as pd

from .harness import main
from .test_end_to_end import CONFIGS, execute, fixture, same
from ..operators import joins
from ..operators import nodes as N
from ..operators.expressions import Binary, Col, Lit
from ..operators.injection import HashMode, LayoutInjector, LayoutPreset
from ..operators.join_types import JoinType, needs_finish

# -- the shape with same-named keys ------------------------------------------------
#
# These three join on `k` against `k`, so the join's `projection` has a duplicate to
# drop; the matrix below joins `k` against `fk`, where both key columns survive. Two
# different paths through `joined_projection`, and both are the general case for some
# query.


def dim_fixture():
    return pd.DataFrame({"k": [0, 1, 2, 3, 9], "label": list("ABCDE")})


def test_inner_join_with_a_shuffle_on_both_sides():
    fact, dim = fixture(), dim_fixture()
    want = dim.merge(fact, how="inner", on="k")
    for parts, group, target in CONFIGS:
        build = N.coalesce_all(
            "build_collect",
            N.emit_partitions(
                "build_emit",
                N.merge_partitions("build_merge", N.scan("dim", dim, parts, group, target)),
                ["k"],
                parts,
            ),
            schema=dict(dim.dtypes),
        )
        probe = N.emit_partitions(
            "probe_emit",
            N.merge_partitions("probe_merge", N.scan("fact", fact, parts, group, target)),
            ["k"],
            parts,
        )
        root = N.unload(
            "unload", N.hash_join("join", build, probe, JoinType.INNER, ["k"], ["k"])
        )
        got, _ = execute(root)
        same(got, want, f"inner join {(parts, group, target)}")


def test_left_outer_join_finish_pass_matches_the_oracle():
    fact, dim = fixture(), dim_fixture()
    want = dim.merge(fact, how="left", on="k")
    for parts, group, target in CONFIGS:
        build = N.coalesce_all(
            "build_collect",
            N.emit_partitions(
                "build_emit",
                N.merge_partitions("build_merge", N.scan("dim", dim, parts, group, target)),
                ["k"],
                parts,
            ),
            schema=dict(dim.dtypes),
        )
        probe = N.emit_partitions(
            "probe_emit",
            N.merge_partitions("probe_merge", N.scan("fact", fact, parts, group, target)),
            ["k"],
            parts,
        )
        root = N.unload(
            "unload",
            # probe_schema: a post-shuffle probe lane can be empty, and the finish must
            # still null-pad the probe columns it never saw.
            N.hash_join(
                "join", build, probe, JoinType.LEFT, ["k"], ["k"],
                probe_schema=list(fact.columns),
            ),
        )
        got, _ = execute(root)
        same(got, want, f"left outer join {(parts, group, target)}")


def test_semi_and_anti_joins_match_the_oracle():
    fact, dim = fixture(), dim_fixture()
    present = set(fact.k.unique())
    for join_type, keep in ((JoinType.LEFT_SEMI, True), (JoinType.LEFT_ANTI, False)):
        want = dim[dim.k.isin(present) == keep].reset_index(drop=True)
        for parts, group, target in CONFIGS:
            build = N.coalesce_all(
                "build_collect",
                N.emit_partitions(
                    "build_emit",
                    N.merge_partitions("build_merge", N.scan("dim", dim, parts, group, target)),
                    ["k"],
                    parts,
                ),
                schema=dict(dim.dtypes),
            )
            probe = N.emit_partitions(
                "probe_emit",
                N.merge_partitions("probe_merge", N.scan("fact", fact, parts, group, target)),
                ["k"],
                parts,
            )
            root = N.unload("unload", N.hash_join("join", build, probe, join_type, ["k"], ["k"]))
            got, _ = execute(root)
            same(got, want, f"{join_type.value} {(parts, group, target)}")


# -- the join capability matrix ----------------------------------------------------
#
# Every join shape the engine can be asked for, at every batching and partitioning config,
# against an oracle written from what the SQL means rather than from the engine's
# mechanics. The build side is always left and always one batch; the probe side streams,
# whatever the type and whatever the condition. Key columns are named differently on the
# two sides (`k` / `fk`) so both survive the join, which is the general case — the
# same-name case, where the join's `projection` drops the duplicate, is what the three
# tests above it exercise.


def build_side():
    """The small side. `k=9` matches no probe row, so every build-preserving type has an
    unmatched row to prove its finish pass on."""
    return pd.DataFrame({"k": [0, 1, 2, 3, 9], "label": list("ABCDE")})


def probe_side(n=48, seed=11):
    """The streamed side. `fk` reaches 5, which no build row has, so the probe-preserving
    types have unmatched rows of their own."""
    rng = np.random.default_rng(seed)
    return pd.DataFrame(
        {
            "fk": rng.integers(0, 6, n),
            "g": rng.choice(list("xyz"), n),
            "v": rng.integers(1, 100, n),
        }
    )


#: One oracle per join type, in pandas, written from the SQL meaning rather than from the
#: engine's mechanics. `label` going null in an outer result is the pad; `mark` is the
#: extra column DataFusion's mark join appends.
ORACLES = {
    JoinType.INNER: lambda b, p: b.merge(p, how="inner", left_on="k", right_on="fk"),
    JoinType.LEFT: lambda b, p: b.merge(p, how="left", left_on="k", right_on="fk"),
    JoinType.RIGHT: lambda b, p: b.merge(p, how="right", left_on="k", right_on="fk"),
    JoinType.FULL: lambda b, p: b.merge(p, how="outer", left_on="k", right_on="fk"),
    JoinType.LEFT_SEMI: lambda b, p: b[b.k.isin(p.fk)],
    JoinType.LEFT_ANTI: lambda b, p: b[~b.k.isin(p.fk)],
    JoinType.LEFT_MARK: lambda b, p: b.assign(mark=b.k.isin(p.fk)),
    JoinType.RIGHT_SEMI: lambda b, p: p[p.fk.isin(b.k)],
    JoinType.RIGHT_ANTI: lambda b, p: p[~p.fk.isin(b.k)],
}


def on_oracle(join_type, build, probe, condition):
    """SQL's answer for a join whose whole ON clause is `condition` over a crossed row.

    A pair the condition rejects, or leaves NULL, is no match: an outer type pads the row
    instead of dropping it, and a semi or anti type counts it as unmatched. That is what
    separates an ON condition from a WHERE above the join, and why this oracle crosses the
    sides rather than merging on the keys and filtering after.
    """
    b, p = build.reset_index(drop=True), probe.reset_index(drop=True)
    crossed = b.assign(_b=range(len(b))).merge(p.assign(_p=range(len(p))), how="cross")
    hit = crossed[condition(crossed).fillna(False).astype(bool).to_numpy()]
    matched_b, matched_p = b.index.isin(hit._b), p.index.isin(hit._p)
    pairs = hit.drop(columns=["_b", "_p"]).reset_index(drop=True)
    padded_b = b[~matched_b].assign(**{c: np.nan for c in p.columns})
    padded_p = p[~matched_p].assign(**{c: np.nan for c in b.columns})[list(pairs.columns)]
    answers = {
        JoinType.INNER: lambda: pairs,
        JoinType.LEFT: lambda: pd.concat([pairs, padded_b], ignore_index=True),
        JoinType.RIGHT: lambda: pd.concat([pairs, padded_p], ignore_index=True),
        JoinType.FULL: lambda: pd.concat([pairs, padded_b, padded_p], ignore_index=True),
        JoinType.LEFT_SEMI: lambda: b[matched_b],
        JoinType.LEFT_ANTI: lambda: b[~matched_b],
        JoinType.LEFT_MARK: lambda: b.assign(mark=matched_b),
        JoinType.RIGHT_SEMI: lambda: p[matched_p],
        JoinType.RIGHT_ANTI: lambda: p[~matched_p],
    }
    return answers[join_type]()


def keyed_and(residual):
    """The ON clause of an equi-join carrying a residual: `k = fk AND <residual>`."""
    return lambda crossed: (crossed.k == crossed.fk) & residual(crossed)


def shuffled_sides(build_frame, probe_frame, config):
    """Both sides hash-partitioned on their join key — the shape a shuffle join has."""
    parts, group, target = config
    build = N.coalesce_all(
        "build_collect",
        N.emit_partitions(
            "build_emit",
            N.merge_partitions("build_merge", N.scan("build_scan", build_frame, parts, group, target)),
            ["k"],
            parts,
        ),
        schema=dict(build_frame.dtypes),
    )
    probe = N.emit_partitions(
        "probe_emit",
        N.merge_partitions("probe_merge", N.scan("probe_scan", probe_frame, parts, group, target)),
        ["fk"],
        parts,
    )
    return build, probe


def join_plan(build_frame, probe_frame, config, join_type, **kwargs):
    build, probe = shuffled_sides(build_frame, probe_frame, config)
    return N.unload(
        "unload",
        N.hash_join(
            "join", build, probe, join_type, ["k"], ["fk"],
            # A post-shuffle probe lane can be empty, and a build-preserving finish must
            # still null-pad the probe columns it never saw.
            probe_schema=list(probe_frame.columns),
            **kwargs,
        ),
    )


def collapsed_sides(build_frame, probe_frame, config):
    """One lane each — what a join with no keys to shuffle on gets."""
    parts, group, target = config
    build = N.coalesce_all(
        "build_collect",
        N.merge_partitions("build_merge", N.scan("build_scan", build_frame, parts, group, target)),
        schema=dict(build_frame.dtypes),
    )
    probe = N.merge_partitions(
        "probe_merge", N.scan("probe_scan", probe_frame, parts, group, target)
    )
    return build, probe


def probe_calls(driver):
    """How many probe batches the run's join lanes were handed, summed over lanes."""
    return sum(
        lane_driver._executor.probe_calls
        for state in driver.states
        for lane_driver in state.lane_drivers.values()
        if isinstance(lane_driver._executor, joins.HashJoin)
    )


def test_every_join_type_matches_the_oracle():
    build, probe = build_side(), probe_side()
    for join_type, oracle in ORACLES.items():
        want = oracle(build, probe)
        for config in CONFIGS:
            got, _ = execute(join_plan(build, probe, config, join_type))
            same(got, want, f"{join_type.name} at {config}")


def test_the_types_that_finish_are_the_build_preserving_five():
    finishing = {t for t in JoinType if needs_finish(t)}
    assert finishing == {
        JoinType.LEFT, JoinType.FULL, JoinType.LEFT_SEMI, JoinType.LEFT_ANTI, JoinType.LEFT_MARK,
    }, finishing


def test_a_cross_join_streams_its_probe_side():
    build, probe = build_side(), probe_side(n=12)
    want = build.merge(probe, how="cross")
    for config in CONFIGS:
        b, p = collapsed_sides(build, probe, config)
        got, _ = execute(N.unload("unload", N.cross_join("cross", b, p)))
        same(got, want, f"cross join at {config}")


def test_every_join_type_matches_the_oracle_as_a_nested_loop():
    # An inequality, which is what makes DataFusion choose a nested-loop join over a hash
    # join: there is no equality to hash on. All nine types, the probe streamed.
    build, probe = build_side(), probe_side(n=12)
    predicate = Binary("<", Col("k"), Col("fk"))
    for join_type in JoinType:
        want = on_oracle(join_type, build, probe, lambda c: c.k < c.fk)
        for config in CONFIGS:
            b, p = collapsed_sides(build, probe, config)
            got, _ = execute(N.unload("unload", N.nested_loop_join("nlj", b, p, join_type, predicate)))
            same(got, want, f"nested-loop {join_type.name} at {config}")


def test_a_predicate_free_nested_loop_over_an_empty_probe_pads_every_build_row():
    # The literal `true` a non-Inner predicate-free join carries: over no probe row, Left
    # and Full owe every build row padded, where a cross join would answer nothing. The
    # probe is filtered to nothing, so its batches arrive with zero rows.
    build, probe = build_side(), probe_side(n=12)
    for join_type in (JoinType.LEFT, JoinType.FULL):
        b, p = collapsed_sides(build, probe, (1, 5, 10))
        p = N.filter_("nothing", p, Binary("<", Col("v"), Lit(0)))
        got, _ = execute(N.unload("unload", N.nested_loop_join("nlj", b, p, join_type, None)))
        same(got, build.assign(**{c: np.nan for c in probe.columns}), f"true {join_type.name}")


def test_a_residual_filter_rides_the_join():
    # An equi-join plus a condition over both sides' columns — TPC-H q17's and q21's shape,
    # the shapes the corpus has (residual filters on Inner, LeftSemi and LeftAnti). The
    # filter is part of each pair's match, so the probe streams for every type.
    build, probe = build_side(), probe_side()
    residual = Binary(">", Col("v"), Lit(50))
    for join_type in (JoinType.INNER, JoinType.LEFT_SEMI, JoinType.LEFT_ANTI):
        want = on_oracle(join_type, build, probe, keyed_and(lambda c: c.v > 50))
        for config in CONFIGS:
            got, _ = execute(join_plan(build, probe, config, join_type, residual=residual))
            same(got, want, f"residual {join_type.name} at {config}")


def test_an_outer_join_with_a_residual_filter_keeps_its_unmatched_rows():
    # A padded row's probe columns are NULL, so a residual applied after the outer gather
    # would make the predicate NULL and drop the row the outer join exists to keep — an ON
    # condition demoted to a WHERE. The residual decides the match instead, per pair.
    build, probe = build_side(), probe_side()
    residual = Binary(">", Col("v"), Lit(50))
    for join_type in (JoinType.LEFT, JoinType.RIGHT, JoinType.FULL):
        want = on_oracle(join_type, build, probe, keyed_and(lambda c: c.v > 50))
        for config in CONFIGS:
            got, _ = execute(join_plan(build, probe, config, join_type, residual=residual))
            same(got, want, f"residual {join_type.name} at {config}")


def test_a_filtered_semi_join_streams_its_probe_side():
    # The semi family finishes from the build rows a probe batch matched, residual
    # included, so a filtered LeftSemi needs no single-batch probe beneath it. Under
    # `v > 80` key 3 matches probe rows and no pair passes, so the residual shows.
    build, probe = build_side(), probe_side()
    want = on_oracle(JoinType.LEFT_SEMI, build, probe, keyed_and(lambda c: c.v > 80))
    assert sorted(want.label) == ["A", "B", "C"], list(want.label)
    b, p = shuffled_sides(build, probe, (1, 5, 10))
    root = N.unload(
        "unload",
        N.hash_join("join", b, p, JoinType.LEFT_SEMI, ["k"], ["fk"],
                    residual=Binary(">", Col("v"), Lit(80))),
    )
    got, driver = execute(root)
    same(got, want, "filtered LEFT_SEMI over a streamed probe")
    assert probe_calls(driver) > 1, "the probe arrived in one batch, so this asserts nothing"


def test_a_right_semi_join_with_a_residual_filter_answers_as_the_oracle():
    # The probe side is what these two emit, so the residual decides which probe rows keep
    # a match — per batch, with the build complete.
    build, probe = build_side(), probe_side()
    residual = Binary(">", Col("v"), Lit(50))
    for join_type in (JoinType.RIGHT_SEMI, JoinType.RIGHT_ANTI):
        want = on_oracle(join_type, build, probe, keyed_and(lambda c: c.v > 50))
        for config in CONFIGS:
            got, _ = execute(join_plan(build, probe, config, join_type, residual=residual))
            same(got, want, f"residual {join_type.name} at {config}")


def test_a_left_join_pads_the_build_rows_its_residual_rejects():
    # Counted by hand. Under `v > 80`, `k=3` (D) meets ten probe rows but no pair passes
    # (its largest `v` is 80), and `k=9` (E) meets no key at all. Both come back once,
    # padded; a residual applied after the outer gather would have dropped both.
    build, probe = build_side(), probe_side()
    got, _ = execute(join_plan(build, probe, (1, 1000, None), JoinType.LEFT,
                               residual=Binary(">", Col("v"), Lit(80))))
    padded = got[got.fk.isna()]
    assert sorted(padded.label) == ["D", "E"], list(padded.label)
    assert (got[got.fk.notna()].v > 80).all(), "a pair the residual rejects was emitted"


# -- null keys --------------------------------------------------------------------


def null_key_sides():
    """Both sides carry a null key, and the shuffle co-locates them.

    The hash skips null columns (comet-mandated), so an all-null key lands in
    `pmod(seed, N)` on both sides — which is what lets a null build key and a null probe
    key meet at all. `k=2` matches nothing, so the anti result is never empty.
    """
    build = pd.DataFrame({"k": [0.0, 1.0, 2.0, None], "label": list("ABCD")})
    probe = pd.DataFrame({"fk": [0.0, 0.0, 1.0, None, None], "v": [1, 2, 3, 4, 5]})
    return build, probe


def labels(frame):
    return sorted(frame.label) if len(frame) and "label" in frame.columns else []


def test_null_keys_follow_null_equals_null_for_every_type():
    # Counted by hand. Under the SQL default a NULL key matches nothing, so the null-keyed
    # build row D is unmatched: kept by anti, marked false (never NULL) by mark. Under
    # `null_equals_null` it meets the two null probe keys and counts as matched. The flag
    # is one rule for every type, anti and mark included.
    build, probe = null_key_sides()
    got = {}
    for join_type in (JoinType.LEFT_SEMI, JoinType.LEFT_ANTI, JoinType.LEFT_MARK):
        for flag in (False, True):
            frame, _ = execute(
                join_plan(build, probe, (4, 5, 10), join_type, null_equals_null=flag)
            )
            got[(join_type, flag)] = frame

    assert labels(got[(JoinType.LEFT_SEMI, False)]) == ["A", "B"]
    assert labels(got[(JoinType.LEFT_SEMI, True)]) == ["A", "B", "D"]
    assert labels(got[(JoinType.LEFT_ANTI, False)]) == ["C", "D"]
    assert labels(got[(JoinType.LEFT_ANTI, True)]) == ["C"]
    for flag, marks in ((False, [True, True, False, False]), (True, [True, True, False, True])):
        frame = got[(JoinType.LEFT_MARK, flag)].set_index("label").mark
        assert list(frame[["A", "B", "C", "D"]]) == marks, (flag, list(frame))


def test_an_inner_join_honours_null_equals_null():
    # The control for the test above, on the equi path.
    build, probe = null_key_sides()
    rows = {}
    for flag in (False, True):
        frame, _ = execute(
            join_plan(build, probe, (4, 5, 10), JoinType.INNER, null_equals_null=flag)
        )
        rows[flag] = len(frame)
    # Three matching pairs on the non-null keys; NULL=NULL adds the two null probe rows.
    assert rows[False] == 3
    assert rows[True] == 5


# -- the model against the code it models ------------------------------------------

#: The device's join session, whose join types this prototype's enum has to name. Read,
#: never built.
JOIN_SESSION_CPP = (
    pathlib.Path(__file__).resolve().parents[3] / "cpp/src/operators/join_session.cpp"
)


def test_the_model_names_every_join_type_the_session_dispatches_on():
    # No import links the C++ to this prototype, so a type added there would be invisible
    # here. Text-level, so it cannot see a semantic drift — but "a new join type appeared"
    # stops being silent.
    assert JOIN_SESSION_CPP.exists(), f"{JOIN_SESSION_CPP} is missing"
    named = {re.sub(r"(?<!^)(?=[A-Z])", "_", name).upper()
             for name in re.findall(r"fb::JoinType_(\w+)", JOIN_SESSION_CPP.read_text())}
    modelled = {member.name for member in JoinType}
    assert named <= modelled, f"the session dispatches on types the prototype has no member for: {named - modelled}"
    assert modelled <= named, f"the prototype has members the session never names: {modelled - named}"


# -- every layout, every type ------------------------------------------------------


def test_every_join_type_survives_every_layout():
    # `test_tpch.py` does this to whole queries over real tables; this does it to the join
    # matrix. A join is the operator most exposed to layout — its build side must arrive as
    # exactly one batch, its lanes must be co-located, and its finish pass runs per lane —
    # so an empty lane or a hash that puts every row in one place is where it would break.
    # The hash mode rotates with the pair rather than nesting inside it: every preset and
    # every placement still appears against every type across the sweep, at a fifth of the
    # runs a full cross product would cost. Derived from the indices, so it reproduces.
    build, probe = build_side(), probe_side()
    modes = list(HashMode)
    for preset_index, preset in enumerate(LayoutPreset):
        for type_index, (join_type, oracle) in enumerate(ORACLES.items()):
            want = oracle(build, probe)
            plan = join_plan(build, probe, (2, 5, 10), join_type)
            hash_mode = modes[(preset_index + type_index) % len(modes)]
            injector = LayoutInjector(preset, hash_mode, empty_batch_probability=0.2)
            got, _ = execute(injector.apply(plan))
            same(got, want, f"{join_type.name} at {injector.label}")


if __name__ == "__main__":
    raise SystemExit(main(globals()))
