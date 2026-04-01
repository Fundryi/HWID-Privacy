# AI Tool Reference (MCP + CLI)

This document maps available MCP tools and CLI commands to project tasks.
Read `CLAUDE.md` first for routing. This file is the tool reference.

**MCP architecture:** All MCP servers are aggregated through **MCP Router** (`mcp-router`), a single stdio entry point configured globally in `~/.claude.json` (Claude Code) and `~/.codex/config.toml` (Codex). In Codex, the server name is normalized to `mcp_router`, so all callable tools appear as `mcp__mcp_router__*` (not repeated per section below).

**MCP surfaces available via mcp-router:**

| # | Surface | Tool prefix/pattern | What it provides |
|---|---------|-------------------|-----------------|
| 1 | **roam-code** | `roam_*` | Codebase semantic graph: symbols, dependencies, impact analysis, health scores |
| 2 | **context7** | `resolve-library-id`, `query-docs` | Library documentation (.NET, WinForms, NuGet packages) |
| 3 | **context-mode** | `ctx_*` | Sandboxed code execution, output indexing, BM25 search over indexed content |
| 4 | **Web search** | `search`, `fetch`, `fetch_content` | DuckDuckGo search, URL fetching, documentation lookup |
| 5 | **Knowledge Graph** | `create_entities`, `search_nodes`, etc. | Persistent memory across sessions |
| 6 | **draw.io** | `open_drawio_*` | Architecture diagrams (XML, Mermaid, CSV formats) |

---

## 1. GitHub (Issues, PRs, Code) -- gh CLI

Use the `gh` CLI (installed globally) for all GitHub operations.

| Task | Command | When to use |
|------|---------|-------------|
| List open issues | `gh issue list` | Start-of-session triage |
| Create issue | `gh issue create` | Tracking new bugs or features |
| Read issue details | `gh issue view <number>` | Before starting work on an issue |
| Close an issue | `gh issue close <number>` | After fix is committed and verified |
| Comment on an issue | `gh issue comment <number>` | Status updates, linking commits |
| Search issues | `gh issue list --search "<query>"` | Finding duplicates or related work |
| Create a PR | `gh pr create` | After committing a feature/fix branch |
| Read PR diff/status | `gh pr view <number>` | Reviewing changes before merge |
| Merge PR | `gh pr merge <number>` | After review passes |
| List PR checks | `gh pr checks <number>` | Checking CI status |
| Search code on GitHub | `gh search code "<query>"` | Finding patterns across remote |
| View PR comments | `gh api repos/{owner}/{repo}/pulls/{number}/comments` | Reading review feedback |

**Owner/repo for this project:** `Fundryi` / `HWID-Privacy`

---

## 2. Roam (Codebase Navigation via mcp-router)

Prefer Roam for codebase understanding and impact checks before manual file-by-file exploration.

| Situation | MCP Tool | When to use |
|-----------|----------|-------------|
| First time in repo | `roam_understand` then `roam_explore` | Full codebase briefing: stack, architecture, health |
| Need to modify a symbol | `roam_preflight` | Blast radius + risk check before edits |
| Pre-change safety bundle | `roam_prepare_change` | Preflight + context + effects in one call |
| Debugging a failure | `roam_diagnose` | Root-cause ranking |
| Debug bundle (one call) | `roam_diagnose_issue` | Root cause + side effects combined |
| Need focused files | `roam_context` | Pull relevant files + line ranges |
| Find symbol | `roam_search_symbol` | Locate symbol definitions/usages |
| Batch find symbols | `roam_batch_search` | Search up to 10 patterns in one call |
| Get symbol details (batch) | `roam_batch_get` | Details for up to 50 symbols in one call |
| What breaks if changed | `roam_impact` | Downstream dependency check |
| File skeleton | `roam_file_info` | All symbols with signatures, kinds, line ranges |
| File dependencies | `roam_deps` | Imports and importers for a file |
| All consumers of symbol | `roam_uses` | Callers, importers, inheritors |
| Trace dependency path | `roam_trace` | Shortest path between two symbols |
| Pre-PR risk check | `roam_pr_risk` | Risk score (0-100) for pending changes |
| Review changes bundle | `roam_review_change` | PR risk + breaking changes + structural diff |
| Current change impact | `roam_diff` | Blast radius of uncommitted edits |
| Affected tests | `roam_affected_tests` | Test files that exercise changed code |
| Dead code | `roam_dead_code` | Unreferenced exported symbols |
| Complexity report | `roam_complexity_report` | Functions ranked by cognitive complexity |
| Codebase health snapshot | `roam_health` | Overall quality score (0-100) |
| Syntax check | `roam_syntax_check` | Tree-sitter validation, no index needed |
| List tool presets | `roam_expand_toolset` | Show available presets: core, review, refactor, debug, architecture, full |

If Roam is missing: install with `pip install roam-code`.
If a repo-wide Roam call times out, retry once with a narrower tool such as `roam_context`, `roam_search_symbol`, or `roam_prepare_change`. If it still fails, fall back to `ctx_*` plus `rg`.

---

## 3. Context7 (Library Documentation via mcp-router)

Use Context7 as the **first stop** for .NET and library documentation before web search.

| Task | Tool | When to use |
|------|------|-------------|
| Find library doc ID | `resolve-library-id` | Before any `query-docs` call |
| Get library documentation | `query-docs` | .NET APIs, WinForms, NuGet packages |

**Workflow:** Always call `resolve-library-id` first to find the correct library ID, then `query-docs` with that ID and your topic.

**Common lookups:**

| Topic | resolve-library-id query | Covers |
|-------|--------------------------|--------|
| .NET runtime APIs | `"dotnet"` | String, collections, IO, threading, etc. |
| Windows Forms | `"dotnet winforms"` | Controls, Form lifecycle, events, layout |
| System.Management (WMI) | `"dotnet system.management"` | ManagementObjectSearcher, WMI queries |
| P/Invoke patterns | `"pinvoke.net"` or `"dotnet interop"` | Win32 API marshalling, SetupAPI |
| NuGet packages | `"<package name>"` | Third-party package APIs |

**When to use context7 vs web search:**
- **context7 FIRST** for any .NET, C#, or library documentation question
- **Web search FALLBACK** if context7 doesn't have the library indexed or returns insufficient results

---

## 4. Context Mode (Sandboxed Execution via mcp-router)

Sandboxed code execution that keeps large output out of context. Output is indexed into a BM25 searchable knowledge base.

| Task | Tool | When to use |
|------|------|-------------|
| Execute code in sandbox | `ctx_execute` | Run shell/JS/Python, only stdout enters context |
| Batch execute + search | `ctx_batch_execute` | Multiple commands + queries in one call (primary tool) |
| Process a file without loading it | `ctx_execute_file` | Analyze logs, CSV, large files -- print summary only |
| Index content for search | `ctx_index` | Store docs/API refs in searchable KB |
| Search indexed content | `ctx_search` | Retrieve specific sections from indexed content |
| Fetch URL + index | `ctx_fetch_and_index` | Fetch page, convert to markdown, index for search |
| Diagnostics | `ctx_doctor` | Check context-mode installation health |
| Upgrade | `ctx_upgrade` | Update context-mode to latest version |
| Usage stats | `ctx_stats` | Context consumption statistics for current session |

**When to use context-mode over Bash:** Prefer `ctx_execute` / `ctx_batch_execute` for commands with large output (>20 lines): `dotnet publish` output, `git log`, `git diff`, WMI query dumps, dependency listings. Use Bash only for file mutations, git writes, and navigation.

---

## 5. Web Search & Documentation Lookup (Fallback via mcp-router)

Fallback when Context7 doesn't cover a topic (WMI class references, community answers, P/Invoke signatures).

| Task | Tool | When to use |
|------|------|-------------|
| Search the web (DuckDuckGo) | `search` | MSDN docs, Stack Overflow, WMI class references |
| Fetch a URL as markdown | `fetch` | Read specific documentation pages |
| Fetch webpage content | `fetch_content` | Parse structured content from a URL |
| Fetch + index into searchable KB | `ctx_fetch_and_index` | Large pages -- indexes content, returns preview, search later |
| Find a library's doc ID | `resolve-library-id` | Before calling `query-docs` |
| Get library documentation | `query-docs` | Framework patterns, library usage |

**Common lookups:**
- WMI Win32 classes: `https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/`
- P/Invoke signatures: `https://pinvoke.net/`
- .NET API reference: `https://learn.microsoft.com/en-us/dotnet/api/`
- SetupAPI reference: `https://learn.microsoft.com/en-us/windows/win32/api/setupapi/`
- Windows Forms reference: `https://learn.microsoft.com/en-us/dotnet/desktop/winforms/`

---

## 6. Knowledge Graph (Persistent Memory via mcp-router)

Persists across sessions. Store entities, relationships, and observations about project state beyond what markdown docs capture.

| Task | Tool | When to use |
|------|------|-------------|
| Store a new entity | `create_entities` | Record a bug, feature, decision, or pattern |
| Add observation to entity | `add_observations` | Append new findings to an existing entity |
| Search the graph | `search_nodes` | Start of session -- check what's been recorded |
| Read full graph | `read_graph` | Full context dump at session start |
| Open specific nodes | `open_nodes` | Retrieve specific entities by name |
| Create relationships | `create_relations` | Link bugs to files, features to components |
| Clean up stale data | `delete_entities`, `delete_observations`, `delete_relations` | Remove outdated info |

**Recommended entity types for this project:**
- `Bug::{description}` -- known bugs with file/line references
- `Feature::{name}` -- feature status and implementation notes
- `ArchDecision::{topic}` -- architectural decisions with rationale (e.g., why parallel providers, why single-file publish)
- `HardwareProvider::{name}` -- provider status, WMI classes used, known quirks

---

## 7. draw.io Diagrams (via mcp-router)

Create and open diagrams directly in draw.io for architecture and data flow visualization.

| Task | Tool | When to use |
|------|------|-------------|
| Create XML diagram | `open_drawio_xml` | Full control over diagram layout |
| Create Mermaid diagram | `open_drawio_mermaid` | Quick diagrams using Mermaid syntax |
| Create CSV diagram | `open_drawio_csv` | Tabular/hierarchical diagrams from CSV |

**Use cases for this project:**
- HWIDChecker architecture (providers -> manager -> UI)
- Hardware provider data flow (WMI query -> parse -> format)
- UI form hierarchy (main form -> sub-forms -> dialogs)
- WMI class relationships (Win32_NetworkAdapter, Win32_DiskDrive, etc.)

---

## 8. CLI Commands (Not MCP -- Run via Bash)

Project-specific commands run through the terminal.

| Task | Command | When to use |
|------|---------|-------------|
| Build & publish | `cd app/src && dotnet publish -c Release` | After any code change (MANDATORY -- see CLAUDE.md Absolute Rule 1) |
| Clean build artifacts | `cd app/src && dotnet clean` | When build is stuck or stale |
| NuGet restore | `cd app/src && dotnet restore` | After adding/changing dependencies |
| Check .NET SDK version | `dotnet --version` | Verify .NET 10 SDK installed |
| Run the app | `./HWIDChecker.exe` | Manual testing after build |
| Git status | `git status` | Before commit |
| Git log | `git log --oneline -20` | Check recent history |

---

## 9. General Principles

1. **All MCP tools go through mcp-router** -- a single aggregator. In Codex: `mcp__mcp_router__*`. Config in `~/.claude.json` (Claude Code) and `~/.codex/config.toml` (Codex).
2. **Use Context7** as first stop for .NET/library documentation before web search.
3. **Use Roam (`roam_*`)** for understanding code structure and impact analysis before making changes.
4. **Use context-mode (`ctx_*`)** for large output commands -- keeps context clean.
5. **Use `gh` CLI** for all GitHub operations (issues, PRs, branches, code search).
6. **Use Knowledge Graph** to persist important decisions and project-specific knowledge across sessions.
7. **Always verify build** after code changes -- `dotnet publish -c Release` from `app/src/` (Absolute Rule 1).

---

## 10. Troubleshooting

If an MCP tool fails or is unavailable:

- **mcp-router not loading** -> Check `~/.claude.json` (Claude Code) or `~/.codex/config.toml` (Codex) has the `mcp-router` entry. Restart the client fully after config changes.
- **Roam not found** -> `pip install roam-code`
- **Roam index stale** -> `roam index` (incremental) or `roam index --force` after major refactors.
- **Roam times out** -> Retry with a narrower call such as `roam_context` or `roam_search_symbol`; if still failing, use `ctx_batch_execute` and `rg`.
- **context7 library not found** -> Call `resolve-library-id` first; if library isn't indexed, fall back to web search (Section 5).
- **context-mode issues** -> Run `ctx_doctor` to diagnose installation health.
- **dotnet publish fails** -> Verify .NET 10 SDK is installed (`dotnet --version`). Only `Release|x64` configuration exists. Check that you're running from `app/src/`.
- **HWIDChecker.exe not updated at repo root** -> Check the PostPublish target in `app/src/HWIDChecker.csproj` -- `DestinationFolder="../.."` must resolve to repo root.

---

## 11. Cross-Reference Quick Links

| Topic | Document | Section |
|-------|----------|---------|
| Agent instructions (Codex entry) | [AGENTS.md](AGENTS.md) | Start Here |
| Project rules & safety | [CLAUDE.md](CLAUDE.md) | Absolute Rules |
| Architecture & source layout | [CLAUDE.md](CLAUDE.md) | Architecture |
| Build workflow | [CLAUDE.md](CLAUDE.md) | Build & Publish |
| Known issues & gotchas | [CLAUDE.md](CLAUDE.md) | Known Issues, Project Gotchas |
| Read routing (task -> doc) | [CLAUDE.md](CLAUDE.md) | Read Routing Table |
| UI modernization plan | [docs/plans/ui-modernization-plan.md](docs/plans/ui-modernization-plan.md) | Full plan |
| Project restructure | [RESTRUCTURE-PLAN.md](RESTRUCTURE-PLAN.md) | Full plan (temporary artifact) |
