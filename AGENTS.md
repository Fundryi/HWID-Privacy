# AI Agent Instructions

## Start Here

**First action in every session:** Read `CLAUDE.md` in the project root. It is the single authority for all project rules — architecture, conventions, safety rules, gotchas, and read routing. Everything below is Codex-specific supplemental information.

- **Claude Code**: `CLAUDE.md` is loaded automatically via project instructions.
- **Codex / other agents**: You **MUST** read `CLAUDE.md` with your file reading tool before taking any action.

**Fallback only** (if `CLAUDE.md` is missing or unavailable):
- Tool selection: `AI_TOOLS.md`
- Project overview: `README.md`

## Codex MCP Tool Namespace

All MCP tools are accessed through **MCP Router**. In Codex, the server name is normalized to `mcp_router`, so tools appear as `mcp__mcp_router__*`. Full tool reference with all parameters: `AI_TOOLS.md`.

Decision tables below use bare tool names (e.g., `roam_understand`, `ctx_execute`) for readability. In Codex, call these as `mcp__mcp_router__<tool_name>`.

If mcp-router is unavailable, fall back to direct Bash commands documented in `AI_TOOLS.md` Section 8.

**Quick decision order:**

| Task | Use MCP | Instead of |
|---|---|---|
| Codebase navigation | `roam_*` tools | Manual grep/glob/file reading |
| .NET/WinForms docs | `context7` (resolve-library-id -> query-docs) | Web search |
| Large output commands | `ctx_execute` / `ctx_batch_execute` | Bash (which floods context) |
| Symbol search | `roam_search_symbol` / `roam_batch_search` | grep for class names |
| Impact analysis | `roam_impact` / `roam_preflight` | Reading files to guess dependencies |
| C# build verification | `dotnet publish -c Release` via Bash | Manual file inspection |

**Roam quick decision:**

| Situation | MCP Tool |
|---|---|
| First time in this repo | `roam_understand` then `roam_explore` |
| Need to modify a symbol | `roam_preflight` or `roam_prepare_change` |
| Debugging a failure | `roam_diagnose` or `roam_diagnose_issue` |
| Need files to read | `roam_context` (files + line ranges) |
| Find a symbol | `roam_search_symbol` or `roam_batch_search` (up to 10) |
| What breaks if I change X? | `roam_impact` |
| Pre-PR check | `roam_pr_risk` or `roam_review_change` |
| After making changes | `roam_diff` (blast radius of uncommitted changes) |
| Affected tests | `roam_affected_tests` |
| Dead code | `roam_dead_code` |
| Complexity hotspots | `roam_complexity_report` |
| Codebase health | `roam_health` |

**Context7 quick decision:**

| Need | Action |
|---|---|
| .NET API docs | `resolve-library-id` query "dotnet" -> `query-docs` |
| WinForms component usage | `resolve-library-id` query "dotnet winforms" -> `query-docs` |
| System.Management (WMI) | `resolve-library-id` query "dotnet system.management" -> `query-docs` |
| NuGet package docs | `resolve-library-id` query "<package name>" -> `query-docs` |

**Context Mode:** Use `ctx_execute` / `ctx_batch_execute` instead of Bash for commands with large output (>20 lines).

**GitHub**: Use `gh` CLI (installed globally), NOT MCP.

## Troubleshooting

- **mcp-router not loading** -> Check `~/.claude.json` (Claude Code) or `~/.codex/config.toml` (Codex) has the `mcp-router` entry. Restart the client fully after config changes.
- **Roam not found** -> `pip install roam-code`
- **Roam index stale** -> `roam index` (incremental) or `roam index --force` after major refactors.
- **Roam times out** -> Retry with a narrower call (`roam_context`, `roam_search_symbol`); if still failing, use `ctx_batch_execute` and `rg`.
- **context7 library not found** -> Call `resolve-library-id` first to find the correct library ID before calling `query-docs`.
- **dotnet publish fails** -> Check .NET 10 SDK is installed (`dotnet --version`). Only `Release|x64` configuration exists.
- **context-mode issues** -> Run `ctx_doctor` to diagnose.
