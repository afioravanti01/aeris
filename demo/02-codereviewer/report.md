# Code review — demo-app

## Summary

The codebase has two blocking errors that must be fixed before any deployment: raw string concatenation of user input into a SQL query in `app.py` (the single most critical finding — 'SQL injection: raw user input is concatenated into the SQL string') and shadowing of the built-in `list` name in `config.py`. Several warnings compound the risk: missing input validation on `username` even after the SQL fix, an O(n²) deduplication algorithm in `utils.py` that should be replaced with `dict.fromkeys`, a missing `if __name__ == '__main__'` guard, and an unused `import sys`. The remaining informational findings — database file permissions, connection reuse, `SELECT *` column projection, and a generic loop variable — are lower priority but worth addressing before the code scales.

## Findings (14)

- [lint/error] app.py — SQL injection: username concatenated directly into query. Use parameterised queries instead.
- [lint/warning] app.py — `main()` is never called; missing `if __name__ == '__main__'` guard.
- [lint/warning] utils.py — O(n²) deduplication; use `dict.fromkeys(items)` to preserve order in O(n).
- [lint/info] utils.py — Variable `x` is too generic; use a descriptive name matching the domain.
- [lint/error] config.py — `list` shadows the built-in; rename to `ALLOWED_VALUES` or similar.
- [lint/warning] config.py — `import sys` is unused; remove it.
- [security/error] app.py — SQL injection: raw user input is concatenated into the SQL string. An attacker can supply `' OR '1'='1` to bypass authentication or `'; DROP TABLE users; --` to destroy data. Fix: use a parameterised query — `conn.execute('SELECT * FROM users WHERE name = ?', (username,))`.
- [security/warning] app.py — No input length or character validation on `username` before it reaches the database layer. Even with parameterisation, enforcing a maximum length (e.g. 64 chars) and rejecting unexpected characters reduces attack surface and prevents resource exhaustion.
- [security/info] app.py — Database file `app.db` is opened with no explicit permissions or WAL configuration. On a multi-user system the file inherits the process umask and may be world-readable. Consider opening the connection after verifying file permissions or storing the DB outside the working directory with restricted access.
- [security/info] config.py — No secrets are visible here, but `API_TIMEOUT` and future config values are defined as module-level constants rather than being loaded from environment variables or a secrets manager. Ensure that credentials, tokens, and keys are never added to this file; use `os.getenv` with a required-value check instead.
- [performance/warning] utils.py — O(n²) membership test in `deduplicate`: `if x not in out` performs a linear scan of the growing output list on every iteration, yielding O(n²) time and O(n) redundant comparisons. Replace with `dict.fromkeys(items)` (O(n) time, preserves insertion order since Python 3.7) or a `seen = set()` guard if a list output is required.
- [performance/info] utils.py — Each failed membership check in the current loop also causes repeated list reallocation as `out.append(x)` may trigger dynamic resizing. While CPython amortises appends, the combined cost of linear scans plus occasional copies makes worst-case behaviour worse than the asymptotic bound alone suggests. Pre-allocating with `out = []` and switching to a set-based approach eliminates both costs at once.
- [performance/warning] app.py — A new database connection is opened on every call to `main()` with no connection pooling or reuse. `sqlite3.connect` performs file-system and OS-level work each time. For any scenario where `main` is called in a loop or the pattern is adopted in a server context, the connection should be created once and passed in (as `find_user` already expects) rather than recreated on each invocation.
- [performance/info] app.py — `SELECT *` retrieves every column from the `users` table. If the table has wide rows or large blob columns, this transfers more data than necessary over the SQLite page cache. Selecting only the columns actually used (e.g. `SELECT id, name`) reduces I/O and memory pressure, and also makes the query plan more predictable if an index exists on `name`.
