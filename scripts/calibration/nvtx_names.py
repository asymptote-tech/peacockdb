"""The names peacockdb pushes into its own NVTX domain, and how to read them back.

Three levels, told apart by name rather than by nesting depth:

    "tpch.sf40 q6 tp1-single"      a case — <dataset>.sf<sf> <query> <mode>
      "3.0 CudfCoalescePartitions"    a call — <seq>.<call_index> <FbKind>
        "p0"                          a partition, nested inside it

The benchmark harness pushes the first, `NodeSession` the second, `ScopedNodeTimer` the
third. Both readers of a capture need the same split, and this module is where it lives:
two implementations of one naming convention drift the moment the convention moves, and
the symptom would be a reader that silently classifies a call as a partition and reports
a query as having done nothing.

Why the case level exists. A call range names its `seq`, and seq numbering restarts with
every plan — q6 and q19 both open with `0.0 CudfScan`. Without an enclosing range a
capture of several queries cannot say which one a call was in, and a reader can only be
told, on a command line, which is a thing a human gets wrong in silence.

The call index is part of the name and not derivable from it: a batched run drives one seq
many times, so without it every repeat is the same string and no row of a record can be
matched to the range that describes it. A capture predating that is refused rather than
half-read — see `call_of`.
"""

import sys

# NVTX_EVENTS.eventType. 59 is a push/pop range, 75 the domain's own name record.
PUSHPOP_RANGE = 59
DOMAIN_CREATE = 75

# The two ABI calls that publish no step of their own: they are handed a handle and named
# by the seq of the node that produced it — the same seq that node's own call carries. So
# one seq holds two kinds in a capture, and a reader checking a seq against the plan has to
# leave these out of that comparison. The spellings are `AbiSymbol::name()`'s.
BARE_CALLS = frozenset({"slice_handle", "result_from_handle"})


def is_case(text):
    """A case range: `<dataset>.sf<sf> <query> <mode>`, pushed by the harness.

    Recognised by what it is not: a call range starts with `<digits>.<digits>` and a
    partition range is `p<k>`. Positive matching on the case's own shape would tie this
    module to how the harness spells a dataset, which is not its business.
    """
    return not is_partition(text) and _call_address(text) is None


def _call_address(text):
    head = text.split(" ", 1)[0]
    seq, dot, call = head.partition(".")
    if not dot or not seq.isdigit() or not call.isdigit():
        return None
    return int(seq), int(call)


def is_partition(text):
    """A partition range: `p<k>`, and nothing else in our domain looks like it.

    A call range always carries a space — the kind follows the address — so `p` plus
    digits with no space is unambiguous even against a future kind beginning with p.
    """
    return text.startswith("p") and " " not in text


def partition_of(text):
    return int(text[1:])


def is_bare_call(kind):
    """Is this call range's kind one of the two that publish no step of their own?"""
    return kind in BARE_CALLS


def call_of(text):
    """`"3.0 CudfCoalescePartitions"` -> `(3, 0, "CudfCoalescePartitions")`.

    Exits on a name from before the call index existed. Reading it as call 0 would make
    every repeat of a seq the same call, which is exactly the confusion the index was
    added to end, and nothing downstream could notice.
    """
    address = _call_address(text)
    if address is None:
        sys.exit(
            f"call range {text!r} has no call index. The capture predates the name "
            "carrying one, and its repeats cannot be told apart -- retake it."
        )
    _, _, kind = text.partition(" ")
    return address[0], address[1], kind


def case_of(text):
    """`"tpch.sf40 q6 tp1-single"` -> `("tpch", "40", "q6", "tp1-single")`.

    The record's first four columns, in the order it writes them, so a reader can key one
    against the other without restating the convention.
    """
    parts = text.split()
    if len(parts) != 3 or ".sf" not in parts[0]:
        sys.exit(
            f"case range {text!r} is not `<dataset>.sf<sf> <query> <mode>`. The harness "
            "builds it from the case's own identifiers; a capture whose outer range says "
            "something else came from other code."
        )
    dataset, _, sf = parts[0].partition(".sf")
    return dataset, sf, parts[1], parts[2]


def domain_id(conn, name):
    row = conn.execute(
        f"""select e.domainId from NVTX_EVENTS e left join StringIds s on s.id = e.textId
            where e.eventType = {DOMAIN_CREATE} and coalesce(e.text, s.value) = ?""",
        (name,),
    ).fetchone()
    if row is None:
        sys.exit(f"capture has no NVTX domain {name!r} -- was the binary run with ranges on?")
    return row[0]
