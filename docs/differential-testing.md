# Differential testing

`temiya::differential` evaluates deterministic, terminating generated programs
through engine-independent adapters. `CommandEngine` uses only a process's
stdin/stdout/stderr and enforces a wall-clock timeout, avoiding dependencies on
another engine's internal API.

Results normalize line endings, trailing whitespace, and configured regexes for
timestamps, addresses, or other non-deterministic text before comparison.
Reviewed source patterns classify known differences by stable identifier.
Unexpected mismatches can be reduced line-by-line with `minimize_source` and
written with `save_reproducer`. Generator seeds, adapter commands, normalization
rules, and known-difference identifiers should accompany stored reproducers.
