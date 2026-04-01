# AI Instruction Files for HWID-Privacy

**Date:** 2026-04-01
**Status:** Approved
**Purpose:** Create a three-file AI instruction system (AGENTS.md, enhanced CLAUDE.md, AI_TOOLS.md) so that Claude Code, Codex, and other AI agents can effectively navigate the codebase, select the right MCP tools, and follow project safety rules.

## Background

The IPS4 project (`ips4-docker-stack-dev`) uses a proven three-file hierarchy for AI agent instructions:
- **AGENTS.md** — entry point for non-Claude agents, delegates to CLAUDE.md
- **CLAUDE.md** — single authority: project rules, constraints, component registry
- **AI_TOOLS.md** — complete MCP tool reference with decision tables

HWID-Privacy currently has only a basic CLAUDE.md covering project overview, tech stack, architecture, and conventions. It lacks tool routing, safety constraints, and decision tables that help agents work efficiently.

## Design Decision

**Approach:** Full IPS4 mirror — same three-file hierarchy, same depth of documentation, adapted to the HWID-Privacy project's C#/.NET stack and dual-nature (guides + application).

**Rationale:** The HWIDChecker application is non-trivial (WMI queries, P/Invoke, parallel providers, auto-update, device cleaning, full WinForms UI) and will receive ongoing improvements. Full-depth AI instructions are justified.

## File Hierarchy

```
AGENTS.md (entry point for Codex/other agents)
  |
  v delegates to
CLAUDE.md (single authority — enhanced with new sections)
  |
  v cross-refs
AI_TOOLS.md (complete MCP tool reference)
```

---

## File 1: AGENTS.md (NEW)

~60 lines. Entry point for Codex and non-Claude agents.

### Sections

1. **Start Here** — mandatory read-first pattern
   - Points to CLAUDE.md as single authority
   - Fallback chain: AI_TOOLS.md -> README.md
   - Claude Code auto-loads CLAUDE.md; Codex must read it explicitly

2. **Codex MCP Tool Namespace** — explains mcp-router aggregation
   - All tools accessible as `mcp__mcp_router__*`
   - Points to AI_TOOLS.md for full reference

3. **Quick Decision Table** — task-to-tool mapping (6 rows)
   | Task | Use | Instead of |
   |------|-----|------------|
   | Codebase navigation | roam_* tools | Manual grep/glob |
   | .NET/WinForms docs | context7 (resolve -> query) | Web search |
   | Large output commands | ctx_execute / ctx_batch | Bash (floods context) |
   | Symbol search | roam_search_symbol | grep for class names |
   | Impact analysis | roam_impact / roam_preflight | Reading files to guess |
   | C# build verification | dotnet publish via Bash | Manual file inspection |

4. **Roam Quick Decision Table** — situation-to-roam-tool mapping (12 rows)
   Covers: first time in repo, modify symbol, debugging, need files, find symbol, what breaks, pre-PR check, after changes, affected tests, dead code, complexity, health.

5. **Context7 Quick Decision Table** — .NET documentation lookups (4 rows)
   Covers: .NET API docs, WinForms components, System.Management (WMI), NuGet packages.

6. **Troubleshooting** — common MCP/tool issues and fixes
   - mcp-router not loading
   - Roam not found / index stale
   - context7 library not found
   - dotnet publish fails

---

## File 2: CLAUDE.md (ENHANCED)

Existing sections preserved. Three new sections added.

### New Section: Absolute Rules (after Project Overview)

7 rules in bold-box format:

| # | Rule | Rationale |
|---|------|-----------|
| 1 | Always verify build with `dotnet publish -c Release` after code changes | Broken build = broken repo |
| 2 | Never modify AutoUpdateService URLs | Breaks updates for all deployed copies |
| 3 | Never commit sensitive real hardware IDs in guides | Use realistic but fabricated identifiers — real OUI prefixes, plausible serial formats, manufacturer-consistent patterns. Change device-specific portion, not format |
| 4 | Never commit HWIDChecker.exe without rebuilding from source | Binary must match committed source — trust issue for security tool |
| 5 | Don't manually copy the published binary | PostPublish MSBuild target handles it — manual copies risk version mismatch |
| 6 | Flag potentially unauthorized tool links | Don't silently add or remove — flag to user and let them decide |
| 7 | Guide content must be technically verified | Hardware spoofing instructions affect real systems — mark untested steps clearly |

### New Section: Read Routing Table (after Conventions)

9-row matrix mapping tasks to documents:

| Task | Read |
|------|------|
| Any code change | CLAUDE.md (conventions) + source architecture |
| Tool/MCP selection | AI_TOOLS.md |
| Agent entry (Codex) | AGENTS.md -> CLAUDE.md |
| UI modernization work | docs/plans/ui-modernization-plan.md |
| Hardware provider changes | Hardware/IHardwareInfo.cs (interface contract) |
| WMI query patterns | Existing providers (NetworkInfo, DiskDriveInfo) |
| Guide content changes | guides/ (mac-spoofing, ssd-spoofing, etc.) + README.md |
| Batch script changes | app/scripts/ + CLAUDE.md conventions |
| Build/publish workflow | CLAUDE.md Build & Publish section |
| Project restructure status | RESTRUCTURE-PLAN.md |

### New Section: Project Gotchas (at end)

10-row gotchas table covering:
- WMI AdapterType filtering (Mellanox issue)
- WMI query permissions (admin elevation)
- Single-file publish (P/Invoke extraction)
- PostPublish copy path (relative path fragility)
- Parallel provider failures (AggregateException)
- SetupAPI P/Invoke (signature mismatch = crash)
- Dark theme assumption (ThemeColors.cs)
- Auto-update URL (hardcoded, breaking)
- .NET version targeting (.NET 10 SDK required; old obj/ files from prior SDK versions v8/v9 are harmless leftovers — only the csproj TargetFramework is authoritative)
- Framework-dependent deploy (runtime required on target)
- PostPublish copy destination (`DestinationFolder="../.."` is relative to `app/src/` and correctly resolves two levels up to the repo root. If the source directory depth ever changes, this path must be updated accordingly)

---

## File 3: AI_TOOLS.md (NEW)

~250 lines. Complete MCP tool reference with numbered sections.

### Sections

**Header** — MCP architecture overview, surface table (6 surfaces: roam, context7, context-mode, web search, knowledge graph, draw.io)

**Section 1: GitHub (gh CLI)** — all gh commands mapped for issues, PRs, code search. Owner/repo reference.

**Section 2: Roam (Codebase Navigation)** — full table of all roam_* tools (~20 tools) with situation -> tool mapping. Timeout fallback guidance.

**Section 3: Context7 (Library Documentation)** — resolve-library-id -> query-docs workflow. Common lookups table for System.Management, WinForms, .NET APIs, P/Invoke. Decision rule: context7 first, web search fallback.

**Section 4: Context Mode (Sandboxed Execution)** — all ctx_* tools. Decision rule: use over Bash for >20 lines output. Applicable commands: dotnet build, git log, git diff, WMI dumps.

**Section 5: Web Search (Fallback)** — search, fetch, ctx_fetch_and_index, resolve-library-id, get-library-docs. Common lookup URLs: MSDN WMI classes, pinvoke.net, .NET API reference, SetupAPI.

**Section 6: Knowledge Graph** — entity CRUD operations. Recommended entity types for this project: Bug, Feature, ArchDecision (architectural decisions with rationale), HardwareProvider.

**Section 7: draw.io Diagrams** — XML, Mermaid, CSV formats. Use cases: architecture diagrams, provider data flow, UI form hierarchy.

**Section 8: CLI Commands (Bash)** — project-specific commands: dotnet publish, dotnet clean, dotnet restore, run app, git status, check SDK version.

**Section 9: General Principles** — 7 principles covering tool priority order.

**Section 10: Troubleshooting** — MCP router, Roam, context7, dotnet SDK, build failures.

**Section 11: Cross-Reference Quick Links** — maps topics to documents and sections.

---

## Implementation Notes

- **Path references:** The repository restructure has already executed. CLAUDE.md uses post-restructure paths (`app/src/`, `app/HWID-CHECKER.sln`, `guides/`). All new sections must use these same paths. The old paths (`HWID-Checkers/Software-Project/source/`, `Files/`) no longer exist.
- **Tool name convention:** Decision tables use bare tool names (e.g., `roam_understand`, `ctx_execute`) for readability. In Codex, these are called as `mcp__mcp_router__<tool_name>`. AGENTS.md must note this convention explicitly in the MCP namespace section, with a fallback note: "If mcp-router is unavailable, fall back to direct Bash commands in AI_TOOLS.md Section 8."
- **AI_TOOLS.md tool coverage:** Only tools relevant to this project are documented. IPS4-specific surfaces (Stripe, Playwright, IPS docs server, UniFi) are excluded.
- **RESTRUCTURE-PLAN.md lifecycle:** RESTRUCTURE-PLAN.md is a temporary artifact. Remove cross-references from AI_TOOLS.md and CLAUDE.md when the restructure is fully archived.
- **Maintenance:** When new MCP tools become available or project structure changes, update AI_TOOLS.md and CLAUDE.md respectively.

## Cross-Reference Structure

```
AGENTS.md
  -> CLAUDE.md (delegates authority)
  -> AI_TOOLS.md (full tool reference)

CLAUDE.md
  -> AI_TOOLS.md (read routing for tool selection)
  -> AGENTS.md (read routing for agent entry)
  -> docs/plans/ui-modernization-plan.md
  -> RESTRUCTURE-PLAN.md

AI_TOOLS.md
  -> CLAUDE.md (project rules, architecture)
  -> AGENTS.md (agent entry point)
  -> docs/plans/ui-modernization-plan.md
  -> RESTRUCTURE-PLAN.md
```
