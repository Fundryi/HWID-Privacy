# AI Instruction Files Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a three-file AI instruction system (AGENTS.md, AI_TOOLS.md, enhanced CLAUDE.md) so all AI agents can effectively navigate the codebase and follow project rules.

**Architecture:** Three markdown files at repo root following the IPS4 hierarchy pattern. AGENTS.md is the entry point for Codex agents, CLAUDE.md is the single authority (enhanced with absolute rules, read routing, and gotchas), AI_TOOLS.md is the complete MCP tool reference with numbered sections and decision tables.

**Tech Stack:** Markdown documentation only — no code changes.

**Spec:** `docs/superpowers/specs/2026-04-01-ai-instruction-files-design.md`

**Repo:** `Fundryi/HWID-Privacy` (GitHub)

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `AGENTS.md` | Entry point for Codex/non-Claude agents |
| Create | `AI_TOOLS.md` | Complete MCP tool reference (11 sections) |
| Modify | `CLAUDE.md` | Add Absolute Rules, Read Routing Table, Project Gotchas sections |

---

### Task 1: Create AGENTS.md

**Files:**
- Create: `AGENTS.md`

- [ ] **Step 1: Write the complete AGENTS.md file**

Create `AGENTS.md` at the repo root with these sections:

```markdown
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
```

- [ ] **Step 2: Verify cross-references**

Confirm that AGENTS.md references:
- `CLAUDE.md` (Start Here section)
- `AI_TOOLS.md` (MCP namespace section, fallback chain)
- `README.md` (fallback chain)

- [ ] **Step 3: Commit**

```bash
git add AGENTS.md
git commit -m "docs: add AGENTS.md entry point for Codex and non-Claude agents"
```

---

### Task 2: Create AI_TOOLS.md

**Files:**
- Create: `AI_TOOLS.md`

- [ ] **Step 1: Write AI_TOOLS.md with all 11 sections**

Create `AI_TOOLS.md` at the repo root. This is the largest file. Full content below:

```markdown
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
```

- [ ] **Step 2: Verify all 11 sections are present and numbered**

Scan the written file and confirm sections 1-11 all exist with correct headers.

- [ ] **Step 3: Verify cross-references**

Confirm that AI_TOOLS.md references:
- `CLAUDE.md` (header, Section 9, Section 11)
- `AGENTS.md` (Section 11)
- `docs/plans/ui-modernization-plan.md` (Section 11)
- `RESTRUCTURE-PLAN.md` (Section 11)

- [ ] **Step 4: Commit**

```bash
git add AI_TOOLS.md
git commit -m "docs: add AI_TOOLS.md MCP tool reference for all AI agents"
```

---

### Task 3: Enhance CLAUDE.md with new sections

**Files:**
- Modify: `CLAUDE.md` (add 3 new sections to existing content)

- [ ] **Step 1: Add Absolute Rules section after Project Overview**

Insert immediately before the `## Tech Stack` header (locate by header text, not line number):

```markdown
## Absolute Rules

> **RULE 1: Always verify build after code changes.**
> Run `dotnet publish -c Release` from `app/src/` and confirm it succeeds before committing.
> A broken build = a broken repo.

> **RULE 2: Never modify AutoUpdateService URLs.**
> The auto-update mechanism in `Services/AutoUpdateService.cs` points to a live endpoint.
> Changing it breaks updates for all deployed copies.

> **RULE 3: Never commit real hardware identifiers in guide examples.**
> Use realistic but fabricated identifiers -- real OUI prefixes, plausible serial formats,
> and manufacturer-consistent patterns. Change the device-specific portion, not the format.
> The result should look like a different real device, not obviously fake like `AA:BB:CC:DD:EE:FF`.

> **RULE 4: Never commit HWIDChecker.exe without rebuilding from source.**
> The repo-root binary must always match committed source. Run `dotnet publish -c Release`
> first -- the MSBuild PostPublish target copies it to repo root automatically.

> **RULE 5: Don't manually copy the published binary.**
> The PostPublish target in `app/src/HWIDChecker.csproj` handles copying to repo root.
> Manual copies risk version mismatch between source and binary.

> **RULE 6: Flag potentially unauthorized tool links in guides.**
> When adding tool or download links, don't silently add or remove questionable links.
> Flag them to the user and let them decide.

> **RULE 7: Guide content must be technically verified.**
> Hardware spoofing instructions affect real systems. Test procedures on actual hardware
> before documenting. Mark untested steps clearly with a warning.
```

- [ ] **Step 2: Add Read Routing Table after Conventions section**

Insert immediately before the `## Known Issues` header (locate by header text, not line number — line numbers will have shifted from Step 1):

```markdown
## Read Routing Table

| Task | Read |
|---|---|
| Any code change | `CLAUDE.md` (conventions + architecture) |
| Tool/MCP selection | `AI_TOOLS.md` |
| Agent entry (Codex) | `AGENTS.md` -> `CLAUDE.md` |
| UI modernization work | `docs/plans/ui-modernization-plan.md` |
| Hardware provider changes | `app/src/Hardware/IHardwareInfo.cs` (interface contract) |
| WMI query patterns | Existing providers (`NetworkInfo.cs`, `DiskDriveInfo.cs`) |
| Guide content changes | `guides/` (mac-spoofing, ssd-spoofing, etc.) + `README.md` |
| Batch script changes | `app/scripts/` + `CLAUDE.md` conventions |
| Build/publish workflow | `CLAUDE.md` Build & Publish section |
| Project restructure status | `RESTRUCTURE-PLAN.md` (temporary artifact -- remove when archived) |
```

- [ ] **Step 3: Add Project Gotchas section at the end of the file**

Append after the `## Active Work` section:

```markdown
## Project Gotchas

| Topic | Gotcha |
|---|---|
| WMI AdapterType filtering | `NetworkInfo.IsRealNetworkAdapter()` rejects NICs that don't report "Ethernet 802.3" -- enterprise/PCIe NICs (Mellanox ConnectX, etc.) get filtered out |
| WMI query permissions | Some WMI classes require admin elevation -- app should handle `ManagementException` gracefully |
| Single-file publish | `PublishSingleFile` bundles everything -- P/Invoke DLLs must be marked for extraction if needed |
| PostPublish copy path | `DestinationFolder="../.."` in the csproj is relative to `app/src/` and correctly resolves to repo root. If source directory depth changes, this path breaks silently |
| Parallel provider failures | `HardwareInfoManager` runs all providers via `Task.WhenAll` -- one provider throwing can surface as `AggregateException` |
| SetupAPI P/Invoke | `Services/Win32/SetupApi.cs` uses direct Win32 calls -- signature mismatches crash the entire app, not just the feature |
| Dark theme assumption | All UI components assume `ThemeColors.cs` dark palette -- adding new forms without applying the theme creates visual inconsistency |
| Auto-update URL | Hardcoded in `AutoUpdateService.cs` -- changing it breaks all deployed copies (see Absolute Rule 2) |
| .NET version targeting | Project targets .NET 10 -- build machines need the matching SDK. Old `obj/` artifacts from prior SDK versions (v8, v9) are harmless leftovers |
| Framework-dependent deploy | Published binary requires .NET runtime on target machine -- not self-contained |
```

- [ ] **Step 4: Verify the complete CLAUDE.md reads correctly**

Read the modified file end-to-end and confirm:
- All existing sections are intact (Project Overview, Tech Stack, Build & Publish, Architecture, Key Pattern, Conventions, Known Issues, Active Work)
- New sections are in the correct positions:
  - Absolute Rules: between Project Overview and Tech Stack
  - Read Routing Table: between Conventions and Known Issues
  - Project Gotchas: after Active Work (end of file)
- All paths use post-restructure form (`app/src/`, `guides/`, etc.)
- Cross-references to `AI_TOOLS.md` and `AGENTS.md` are present

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add absolute rules, read routing, and gotchas to CLAUDE.md"
```

---

### Task 4: Final verification

- [ ] **Step 1: Verify all three files exist and cross-reference correctly**

Check that:
- `AGENTS.md` exists and references `CLAUDE.md` and `AI_TOOLS.md`
- `AI_TOOLS.md` exists and references `CLAUDE.md` and `AGENTS.md`
- `CLAUDE.md` Read Routing Table references `AI_TOOLS.md` and `AGENTS.md`

- [ ] **Step 2: Verify no broken cross-references**

Confirm these files referenced in cross-reference tables exist:
- `docs/plans/ui-modernization-plan.md`
- `RESTRUCTURE-PLAN.md`
- `README.md`
- `app/src/Hardware/IHardwareInfo.cs`
- `app/src/HWIDChecker.csproj`

- [ ] **Step 3: Final commit (if any fixups needed)**

```bash
git add AGENTS.md AI_TOOLS.md CLAUDE.md
git commit -m "docs: finalize AI instruction files cross-references"
```
