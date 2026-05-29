# Code review — demo-app

## Summary

The review surfaces three errors that must be fixed before any deployment: the most critical is the SQL injection in `app.py` — user input is concatenated directly into a query string, allowing an attacker to bypass authentication or exfiltrate data with a trivially crafted username. A second error in `config.py` shadows the built-in `list`, which can cause subtle and hard-to-diagnose failures throughout the codebase. At warning severity, `utils.py` contains an O(n²) deduplication loop that will degrade under load, `app.py` is missing an `if __name__ == '__main__'` guard, and an unused `import sys` adds unnecessary noise. Several informational findings round out the review, covering database file permissions, connection pooling, and over-broad `SELECT *` queries; these are low-urgency but worth addressing before the project scales.

## Findings (12)

- [lint/error] app.py — SQL query built by string concatenation; exposes app to SQL injection. Use parameterised queries.
- [lint/warning] app.py — `main` is never called; missing `if __name__ == '__main__'` guard.
- [lint/warning] utils.py — O(n²) deduplication; use `dict.fromkeys(items)` or `list(set(items))` instead.
- [lint/error] config.py — `list` shadows the built-in; rename to `ALLOWED_VALUES` or similar.
- [lint/warning] config.py — `import sys` is unused; remove it.
- [security/error] app.py — SQL injection: user-supplied input is concatenated directly into the query string. An attacker can craft a username such as `' OR '1'='1` to bypass authentication or exfiltrate data. Fix: use a parameterised query — `conn.execute('SELECT * FROM users WHERE name = ?', (username,))`.
- [security/warning] app.py — No input length or character validation on `username` before it reaches the database layer. Even with parameterised queries, unbounded input can cause excessive memory use or trigger edge-cases in downstream code. Add a maximum-length check and strip control characters.
- [security/info] app.py — Database file `app.db` is opened with no explicit permissions or WAL settings. On a multi-user system the file may be world-readable. Ensure the file is created with restrictive OS permissions (e.g. `chmod 600`) and that the application runs under a dedicated low-privilege user.
- [security/info] config.py — No secrets are hardcoded in this file, but `import os` is present without use. If environment variables holding credentials are later read here (e.g. `os.environ['SECRET_KEY']`), ensure they are never logged or included in error responses.
- [performance/warning] utils.py — O(n²) deduplication: `if x not in out` performs a linear scan of the output list for every input element, making the function quadratic in the size of `items`. For large inputs this becomes a bottleneck. Replace with `list(dict.fromkeys(items))` (preserves order, O(n)) or `list(set(items))` (O(n), unordered).
- [performance/info] app.py — `SELECT *` fetches every column from the `users` table on each call. If the table has wide rows or many columns that are not needed by the caller, this wastes I/O and memory. Prefer selecting only the columns required (e.g. `SELECT id, name`) and add an index on `name` to avoid a full-table scan.
- [performance/info] app.py — A new database connection is opened inside `main` every time it is invoked, with no connection pooling or reuse. In a long-running or frequently-called context, repeatedly opening and closing SQLite connections adds unnecessary overhead. Create the connection once and pass it through, or use a connection pool.
