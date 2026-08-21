# CodeGraph development index

CodeGraph is optional local developer tooling. Its SQLite index lives in the
gitignored `.codegraph/` directory; this repository commits only the exclusions
and validation scripts.

## Initialize and query

From the repository root:

```bash
codegraph init
codegraph sync
codegraph status
codegraph explore "dispatch_tool_call bridge path"
python3 scripts/codegraph/test_verify_index.py
python3 scripts/codegraph/verify_index.py
python3 scripts/codegraph/query_smoke.py
```

Run `sync` after changing branches or if the watcher was not running. The
validators are manual development checks rather than mandatory CI because the
repository does not install or pin CodeGraph. `verify_index.py` requires exact
agreement between tracked, non-ignored, non-excluded supported source paths and
the index. It also fails when the measured resolver-error ratios exceed their
checked-in ceilings. Those ceilings are regression baselines, not accuracy
guarantees.
`query_smoke.py` checks known BioRouter facts through the CLI and fails on any
nonzero or malformed CLI response.

## MCP and Codex

Register the MCP server through CodeGraph's installer, or add the equivalent to
the user's Codex configuration:

```toml
[mcp_servers.codegraph]
command = "codegraph"
args = ["serve", "--mcp"]
```

The MCP tool is `codegraph_explore`. When querying a worktree or a second
repository, pass its absolute path as `projectPath`; CodeGraph selects the
nearest `.codegraph/` index. MCP registration only exposes the tool. Codex also
needs a user-global instruction telling it to use CodeGraph before grep or file
reads when a repository has a `.codegraph/` directory. User-global Codex
configuration and instructions deliberately do not live in this repository.

CodeGraph 1.5.0 does not index shell scripts, and name-based cross-file
resolution can produce false edges for common names. Treat `explore`, `impact`,
and `affected` as navigation aids, then rely on source inspection, compilers,
and tests for correctness. Keep `affected` depth low when selecting tests; the
default depth can expand through false cross-language edges.
