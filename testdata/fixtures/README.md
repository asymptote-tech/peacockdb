# fixtures

Committed samples of a format two readers read independently. Nothing regenerates them: they are
samples of a convention, not goldens of a run. The numbers are a real q6 section's shape, shortened.

`sectioned-cost.txt` is a two-section `.cost.txt`. `cost-report` reads a total out of one section;
the test side's parser reads the same sections. `cost-report` has no dependencies on purpose, so the
two readers share no code — this file is what holds them to one convention, and both assert the same
values off it so they diverge red rather than silently.

`two-row-registry.csv` is loaded through `Registry::load` so a widget test starts on the far side of
the loader. Every other widget test builds its rows by hand, downstream of the seam where a column
`load` does not read renders as a plausible em-dash rather than as an error.

`row-range-clamp.txt` is the row-range clamp's cases, `offset length rows -> begin end`. The rule
is written twice, `RowRange::clamp` in Rust and `clamp_row_range` in C++, because the cpu backend
never crosses the ABI. Each side's test reads this file and asserts its own clamp, so a case added
here reaches both, and a drift fails the drifting side on the shared line.
