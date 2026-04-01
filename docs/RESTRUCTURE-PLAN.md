# HWID-Privacy Repository Restructuring Plan

## Context

The repo has grown organically and the structure is hard to navigate — both in VS Code and on GitHub. Folder names are vague (`Files/`, `Software-Project/`, `HWID-Checkers/`), guides and app code are tangled together, naming conventions are inconsistent (spaces vs hyphens, mixed casing), and the root is cluttered. This plan restructures everything with clear names, consistent kebab-case naming, and adds a VS Code workspace file for optimized sidebar navigation.

**Constraint:** `HWIDChecker.exe` stays at repo root — the auto-update system downloads from `https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe` and that URL must not break.

---

## Target Structure

```
HWID-Privacy/
├── README.md                          (updated links)
├── CLAUDE.md                          (updated paths)
├── HWIDChecker.exe                    (stays at root — auto-update target)
├── .gitignore                         (merged with Software-Project/.gitignore)
├── HWID-Privacy.code-workspace        NEW — VS Code multi-root workspace
│
├── .github/                           NEW
│   └── workflows/
│       └── release.yml                NEW — optional CI build + GitHub Release
│
├── guides/                            ← was "Files/"
│   ├── mac-spoofing/
│   │   ├── mac-spoofing.md            ← was "MAC-Spoofing.md"
│   │   ├── images/                    ← was "Images/"
│   │   ├── intel/                     ← was "Intel Files/"
│   │   ├── realtek/                   ← was "Realtek Files/"
│   │   ├── usb-ax88179/              ← was "USB AX88179 Files/"
│   │   ├── usb-realtek/              ← was "USB Realtek Files/"
│   │   └── mellanox-connectx-3/      ← was "Mellanox ConnectX-3/"
│   ├── motherboard-spoofing/          ← was "MOBO-Spoofing/"
│   │   ├── motherboard-spoofing.md    ← was "MOBO-Spoofing.md"
│   │   └── tools/                     (3 zip files moved here from parent)
│   ├── ssd-spoofing/                  ← was "SSD-Spoofing/"
│   │   ├── ssd-spoofing.md            ← was "SSD-Spoofing.md"
│   │   ├── m2-nvme/                   ← was "M.2-SSD-Files/"
│   │   └── sata-25/                   ← was "Normal-2.5-SSD-Files/"
│   └── tpm-spoofing/                  ← was "fTPM/"
│       ├── tpm-spoofing.md            NEW — extract fTPM section from README.md
│       └── images/                    (EK OFFLINE INTEL.png → ek-offline-intel.png)
│
├── app/                               ← was "HWID-Checkers/"
│   ├── HWID-CHECKER.sln               (moved up from Software-Project/, path ref updated)
│   ├── src/                            ← was "Software-Project/source/"
│   │   ├── HWIDChecker.csproj         (PostPublish path updated)
│   │   ├── Program.cs
│   │   ├── app.manifest
│   │   ├── .roamignore
│   │   ├── Hardware/                  (unchanged internally)
│   │   ├── Services/                  (unchanged internally)
│   │   ├── UI/                        (unchanged internally)
│   │   └── Resources/                 (unchanged internally)
│   ├── scripts/                        ← was "Bats/"
│   │   ├── hwid-check-w10.bat         ← was "HWID CHECK W10.bat"
│   │   └── hwid-check-w11.bat         ← was "HWID CHECK W11.bat"
│   └── docs/
│       ├── architecture.md             ← was "AI-README.md"
│       ├── auto-update.md              ← was "AUTO-UPDATE-README.md"
│       └── readme.md                   ← was "Software-Project/README.md"
│
└── docs/
    └── plans/                          ← was "plans/" at root
        ├── ui-modernization-plan.md           (unchanged)
        └── ui-modernization-baseline-checklist.md  (unchanged)
```

---

## VS Code Workspace File

Create `HWID-Privacy.code-workspace` at repo root:

```json
{
  "folders": [
    { "name": "Guides",        "path": "guides" },
    { "name": "App Source",    "path": "app/src" },
    { "name": "Scripts",       "path": "app/scripts" },
    { "name": "Docs & Plans",  "path": "docs" },
    { "name": "Project Root",  "path": "." }
  ],
  "settings": {
    "files.exclude": {
      "**/obj": true,
      "**/bin": true,
      "**/.vs": true,
      "**/build": true
    }
  }
}
```

This gives a clean VS Code sidebar with logical sections regardless of the physical folder layout.

---

## Critical Path Changes (MUST NOT BREAK)

### 1. Auto-Update URL — NO CHANGES NEEDED
- Hardcoded in `Services/AutoUpdateService.cs` line 21:
  ```
  private const string GITHUB_RAW_URL = "https://github.com/Fundryi/HWID-Privacy/raw/main/HWIDChecker.exe";
  ```
- The `.exe` stays at repo root, so this URL continues to work

### 2. PostPublish Target in .csproj
- **Current** (`source/HWIDChecker.csproj`): `DestinationFolder="../../.."` (3 levels up: source → Software-Project → HWID-Checkers → root)
- **New** (`src/HWIDChecker.csproj`): `DestinationFolder="../.."` (2 levels up: src → app → root)

### 3. Solution File (.sln)
- Update project reference path from `source\HWIDChecker.csproj` to `src\HWIDChecker.csproj`

---

## Complete File Move Table (use `git mv`)

| Old Path | New Path |
|----------|----------|
| `Files/MAC-Spoofing/MAC-Spoofing.md` | `guides/mac-spoofing/mac-spoofing.md` |
| `Files/MAC-Spoofing/Images/` | `guides/mac-spoofing/images/` |
| `Files/MAC-Spoofing/Intel Files/` | `guides/mac-spoofing/intel/` |
| `Files/MAC-Spoofing/Realtek Files/` | `guides/mac-spoofing/realtek/` |
| `Files/MAC-Spoofing/USB AX88179 Files/` | `guides/mac-spoofing/usb-ax88179/` |
| `Files/MAC-Spoofing/USB Realtek Files/` | `guides/mac-spoofing/usb-realtek/` |
| `Files/MAC-Spoofing/Mellanox ConnectX-3/` | `guides/mac-spoofing/mellanox-connectx-3/` |
| `Files/MOBO-Spoofing/MOBO-Spoofing.md` | `guides/motherboard-spoofing/motherboard-spoofing.md` |
| `Files/MOBO-Spoofing/*.zip` (3 files) | `guides/motherboard-spoofing/tools/*.zip` |
| `Files/SSD-Spoofing/SSD-Spoofing.md` | `guides/ssd-spoofing/ssd-spoofing.md` |
| `Files/SSD-Spoofing/M.2-SSD-Files/` | `guides/ssd-spoofing/m2-nvme/` |
| `Files/SSD-Spoofing/Normal-2.5-SSD-Files/` | `guides/ssd-spoofing/sata-25/` |
| `Files/fTPM/EK OFFLINE INTEL.png` | `guides/tpm-spoofing/images/ek-offline-intel.png` |
| `HWID-Checkers/Software-Project/HWID-CHECKER.sln` | `app/HWID-CHECKER.sln` |
| `HWID-Checkers/Software-Project/source/` (entire tree) | `app/src/` |
| `HWID-Checkers/Software-Project/README.md` | `app/docs/readme.md` |
| `HWID-Checkers/Software-Project/AI-README.md` | `app/docs/architecture.md` |
| `HWID-Checkers/Software-Project/AUTO-UPDATE-README.md` | `app/docs/auto-update.md` |
| `HWID-Checkers/Bats/HWID CHECK W10.bat` | `app/scripts/hwid-check-w10.bat` |
| `HWID-Checkers/Bats/HWID CHECK W11.bat` | `app/scripts/hwid-check-w11.bat` |
| `plans/` (entire tree) | `docs/plans/` |

---

## Content Updates Required

### README.md (root)
- [ ] All `Files/MAC-Spoofing/...` links → `guides/mac-spoofing/...`
- [ ] All `Files/MOBO-Spoofing/...` links → `guides/motherboard-spoofing/...`
- [ ] All `Files/SSD-Spoofing/...` links → `guides/ssd-spoofing/...`
- [ ] All `HWID-Checkers/...` references → `app/...`
- [ ] Extract inline fTPM section into `guides/tpm-spoofing/tpm-spoofing.md`, replace with link
- [ ] Update batch script references to `app/scripts/...`
- [ ] **TOC anchor cleanup**: update/remove `#-ftpm-spoofing` and `#️-dtpm-not-recommended` anchors (~line 28-29) after the inline fTPM section is extracted — point them to the new guide or remove if section is replaced with a link

### CLAUDE.md (root)
- [ ] All `HWID-Checkers/Software-Project/source/` → `app/src/`
- [ ] Solution path: `HWID-Checkers/Software-Project/HWID-CHECKER.sln` → `app/HWID-CHECKER.sln`
- [ ] Build command: update path to `app/src/`
- [ ] Project path: update `.csproj` path
- [ ] **Guides path** (~line 7): `Files/` → `guides/`
- [ ] **Batch scripts path** (~line 8): `HWID-Checkers/Bats/` → `app/scripts/`  (also referenced as `Bats/`)
- [ ] **Published binary path** (~line 8): update any `HWIDChecker.exe` path references that mention the old tree
- [ ] **Plans path** (~line 84 area): `plans/` → `docs/plans/`

### HWIDChecker.csproj (app/src/)
- [ ] PostPublish target: `DestinationFolder="../../.."` → `DestinationFolder="../.."`

### HWID-CHECKER.sln (app/)
- [ ] Project reference: `source\HWIDChecker.csproj` → `src\HWIDChecker.csproj`

### guides/tpm-spoofing/tpm-spoofing.md (NEW file)
- [ ] Create from extracted README.md fTPM section
- [ ] **Image path**: reference the renamed image as `./images/ek-offline-intel.png` (moved from `EK OFFLINE INTEL.png`)

### Guide markdown files
- [ ] Check all relative image/tool paths still resolve after the folder renames
- [ ] `mac-spoofing.md`: image refs from `Images/` → `images/`
- [ ] `ssd-spoofing.md`: refs from `M.2-SSD-Files/` → `m2-nvme/`, `Normal-2.5-SSD-Files/` → `sata-25/`
- [ ] `motherboard-spoofing.md`: zip refs may need updating if they referenced parent-relative paths

### .gitignore (root)
- [ ] Merge any unique rules from `HWID-Checkers/Software-Project/.gitignore`
- [ ] Delete `HWID-Checkers/Software-Project/.gitignore` after merge

### app/docs/ files
- [ ] `architecture.md` (was AI-README.md): update any internal path references
- [ ] `auto-update.md` (was AUTO-UPDATE-README.md): update deployment workflow paths
- [ ] `readme.md` (was Software-Project/README.md): **extensive path updates needed** — lines 27, 33, 38, 125, 135, 161, 162 contain raw path/document-name references (`source/`, `AI-README.md`, `AUTO-UPDATE-README.md`, etc.) that must be updated to reflect the new `app/src/`, `app/docs/architecture.md`, `app/docs/auto-update.md` layout

### docs/plans/ files
- [ ] `ui-modernization-plan.md` (~line 9): update internal ref from `plans/ui-modernization-baseline-checklist.md` to `ui-modernization-baseline-checklist.md` (now a sibling in the same directory) or `docs/plans/...` if using repo-root-relative paths

---

## Cleanup After Moves
- [ ] Remove empty `Files/` directory tree
- [ ] Remove empty `HWID-Checkers/` directory tree
- [ ] Remove empty `build/` directory at root (if it exists)
- [ ] Remove empty `plans/` directory at root (after move to docs/)

---

## Verification Checklist

1. [ ] **Build works**: `dotnet publish -c Release` from `app/src/` — exe appears at repo root
2. [ ] **VS Code workspace**: open `HWID-Privacy.code-workspace` — sidebar shows multi-root view
3. [ ] **GitHub markdown links**: all links in README.md resolve correctly on GitHub
4. [ ] **Auto-update**: URL `raw/main/HWIDChecker.exe` still serves the binary
5. [ ] **Git history**: `git log --follow` on moved files shows history preserved
6. [ ] **Guide images**: all image embeds in guide `.md` files render correctly
7. [ ] **Cross-references**: no broken links between guide files or to HWIDChecker.exe
8. [ ] **No leftover empty dirs**: `Files/`, `HWID-Checkers/`, `build/`, `plans/` all removed

---

## Execution Order

1. Create target directory structure (`guides/`, `app/`, `docs/`, etc.)
2. `git mv` all files per the move table above
3. Create new files (`tpm-spoofing.md`, `.code-workspace`)
4. Update content in all affected files (README, CLAUDE.md, .csproj, .sln, guides)
5. Merge `.gitignore` files
6. Clean up empty directories
7. Build and verify
8. Commit
