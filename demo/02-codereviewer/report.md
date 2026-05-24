# Code review — demo-app

## Summary

The codebase contains three blocking errors that must be resolved before any deployment. The most critical finding is the SQL injection vulnerability in app.py: username is concatenated directly into the query string, allowing an attacker to bypass authentication or destroy data — switching to a parameterised query is a one-line fix with no functional trade-offs. A second error in config.py silently shadows Python's built-in list, which can cause subtle, hard-to-diagnose failures anywhere that name is relied upon. Beyond the errors, several warnings compound the risk: the missing __main__ guard causes side effects on import, unbounded input reaches the database layer even after the injection is fixed, and the O(n²) deduplication in utils.py will degrade under load. The handful of info-level notes (terse variable names, hard-coded DB path, SELECT *, unused import, list reallocations) are low-urgency but should be addressed in a follow-up pass to reduce long-term maintenance burden.

## Findings (12)

- [lint/error] app.py — SQL query built by string concatenation — classic injection vector. Use parameterised queries instead.
- [lint/warning] app.py — `main()` is defined but never guarded by `if __name__ == '__main__'`; runs on import.
- [lint/warning] utils.py — O(n²) deduplication; use `dict.fromkeys(items)` or `list(set(items))` for linear time.
- [lint/info] utils.py — Loop variable `x` is too terse; rename to `item` to match the parameter name.
- [lint/error] config.py — `list` shadows the built-in. Rename to `ALLOWED_VALUES` or similar.
- [lint/warning] config.py — `import sys` is unused — remove it.
- [security/error] app.py — SQL injection: `username` is concatenated directly into the query string. An attacker can supply `' OR '1'='1` to bypass authentication or `'; DROP TABLE users; --` to destroy data. Fix: use a parameterised query — `conn.execute('SELECT * FROM users WHERE name = ?', (username,))`.
- [security/warning] app.py — No input validation or length cap on `username` before it reaches the database layer. Even after switching to parameterised queries, unbounded input can be used for enumeration or resource exhaustion. Validate and sanitise at the entry point.
- [security/info] app.py — Database file `app.db` is opened with a hard-coded relative path. In a multi-user or server context this may resolve to an unintended location. Derive the path from a configuration value or an absolute base directory.
- [performance/warning] utils.py — Quadratic membership test on a growable list: `if x not in out` performs a linear scan of `out` for every element of `items`, giving O(n²) time overall. Replace with a shadow set — build a `seen: set` in parallel with `out` and check `if x not in seen` instead. This reduces deduplication to O(n) time and O(n) space with no change in output-order semantics.
- [performance/info] utils.py — Each matched element triggers an `append` call, which may cause repeated list reallocations as `out` grows. The shadow-set approach above eliminates most appends; if allocation pressure is still a concern, pre-allocate with `out = list(dict.fromkeys(items))` which builds the result in a single pass with no intermediate list growth.
- [performance/info] app.py — `SELECT *` fetches every column in the `users` row even though the caller uses only `fetchone()` and likely needs just a subset of fields. Selecting only the required columns reduces the data transferred from SQLite to Python and avoids unnecessary object construction for unused columns.
