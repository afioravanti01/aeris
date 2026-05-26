# Code review — demo-app

## Summary

The codebase has three blocking errors that must be resolved before any release: a SQL injection vulnerability in `app.py` where raw user input is concatenated directly into a query string, and a builtin-shadowing error in `config.py` where `list` masks the built-in type. The SQL injection finding — 'SQL injection: raw user input from `input()` is concatenated into the SQL string in `find_user`' — is the single most critical issue, as it exposes the application to full database compromise with no exploit sophistication required. At warning severity, `utils.py` contains an O(n²) deduplication routine, `app.py` lacks an `if __name__ == '__main__'` guard, and `config.py` carries an unused import; these are straightforward to fix but should not be deferred. Several informational findings around input validation, hardcoded paths, and query column selection round out a defence-in-depth backlog that improves robustness once the errors are addressed.

## Findings (13)

- [lint/error] app.py — SQL injection: username concatenated directly into query. Use parameterised query instead.
- [lint/warning] app.py — `main()` is never called; missing `if __name__ == '__main__'` guard.
- [lint/warning] utils.py — O(n²) deduplication; use `dict.fromkeys(items)` or `list(set(items))` for linear time.
- [lint/error] config.py — `list` shadows the builtin. Rename to avoid masking built-in type.
- [lint/warning] config.py — `import sys` is unused.
- [security/error] app.py — SQL injection: raw user input from `input()` is concatenated into the SQL string in `find_user`. An attacker can terminate the string and append arbitrary SQL (e.g. `' OR '1'='1`). Fix: use a parameterised query — `conn.execute('SELECT * FROM users WHERE name = ?', (username,))`.
- [security/warning] app.py — No input length or character validation on the value returned by `input()` before it reaches the database layer. Even with parameterisation, enforcing a maximum length and rejecting unexpected characters is a defence-in-depth measure against denial-of-service via oversized strings.
- [security/info] app.py — Database file `app.db` is opened with a hardcoded relative path. If the working directory is attacker-controlled or world-writable, the connection may open an unexpected file. Prefer an absolute, configuration-supplied path.
- [security/info] config.py — No secrets are present in this file, but `os` is imported without use. Confirm it was not removed along with secret-loading logic (e.g. `os.environ` reads) that should still be present; absent secret management is itself a risk if credentials are later hardcoded here.
- [performance/warning] utils.py — O(n²) membership test in `deduplicate`: `x not in out` scans the output list linearly on every iteration, making the function O(n²) in the number of items. Replace with a set-based approach — `list(dict.fromkeys(items))` preserves insertion order in O(n), or `list(set(items))` if order is irrelevant.
- [performance/info] utils.py — Repeated `out.append(x)` calls on a list that grows incrementally are fine in isolation, but combined with the O(n) membership check above, each append is preceded by a full scan. Switching to `dict.fromkeys` eliminates both the redundant scan and the incremental growth overhead in a single change.
- [performance/info] app.py — `SELECT *` retrieves all columns from `users` on every call to `find_user`. If the table has wide rows or many columns, this transfers unnecessary data from SQLite into Python. Select only the columns the caller actually uses to reduce I/O and deserialization overhead.
- [performance/info] app.py — A new `sqlite3.connect` is opened in `main` on every invocation with no connection pooling or reuse. For a long-running process or repeated calls, reusing a single connection (or a pool) avoids the repeated file-open and journal-setup cost.
