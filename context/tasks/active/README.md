# Active tasks

Create one Markdown task card for each meaningful unit of work. Link the active card from `context/STATE.yaml`.

Each active card must declare:

```yaml
owner: agent-or-human
execution_mode: shared_readonly | isolated_write | serial_write
allowed_write_set: "repository-relative paths or an explicitly named external scope"
base_commit: "commit SHA or null for serial/shared work"
worktree_path: "absolute path or null for serial/shared work"
coordinator: agent-or-human
```

Use `isolated_write` only with a separate worktree or equivalent copy. If no isolation is available, use `serial_write`; a branch in the same physical directory is not isolation.
