# Fuzz Corpus Policy

Retain minimized seeds under `fuzz/corpus/<target>/` when a timed T1 fuzz run
finds coverage-increasing inputs or crash regressions.

Generated crash artifacts belong under `fuzz/artifacts/` and are ignored by Git
until a minimized reproducer is promoted into the corpus or a regression test.
