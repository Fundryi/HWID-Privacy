# WP-2B validation record

Date: 2026-10-07. **Source/host verification only. No kernel binary has been
built, signed, installed or loaded. No hardware read was attempted.**

## Verified on this SDK-only machine

- `check-host.ps1`: MSVC x64 `/std:c17 /W4 /WX /sdl /GS /guard:cf /MT` compiled
  `tests/host.c` with the actual `src/protocol.c` and `src/smbus.c`.
- C ABI assertions: header 16, capabilities 40, slot 24, enumeration 792,
  read request 24, read reply 1,064, reserved reply 24; SPD offset 40.
- C harness: buffered input/output aliasing, short/oversized input, short output,
  invalid version/size/reserved fields, unknown IOCTLs, unsupported reserved
  slots, zero unused outputs, stale/cross-open/re-enumerated tokens, power epoch,
  excessive backend count, sanitized failed reads and no hardware source.
- Fake Intel/AMD register transactions: fixed read-direction targets, no payload
  writes, no mux/PCI writes, own-semaphore release, busy/absent/unsupported page,
  cancellation, bounded timeout/host abort, poisoned-controller refusal and
  driver-owned PCI/PM base validation. This is a software state-machine check,
  not proof of real firmware arbitration or controller timing.
- Isolated ABI crate: Cargo fmt/clippy (`-D warnings`), two Rust tests, including
  parsing a complete wire reply emitted by the actual C dispatcher.
- PowerShell parse checks and MSBuild XML parse check.
- `build.ps1 -CheckPrerequisites`: detected SDK `10.0.26100.0`, missing matching
  WDK kernel headers/libraries, printed installation guidance and stopped.
- Main app `pwsh -NoProfile -File app/rust/scripts/check.ps1`: all base checks
  passed; 213 tests passed, 39 existing live checks ignored. Release build,
  static-CRT import allowlist, resource versions and manifest checks passed.

## Owner: WDK build and isolated lab gates

- [ ] Install matching complete SDK/WDK and Visual Studio WDK extension; build
  both Debug and Release, including actual KMDF compilation/link and `/analyze`.
- [ ] Run stampinf, Universal InfVerif and Inf2Cat on both root and explicitly
  selected controller packages. Check OS labels, KMDF minimum and installation
  semantics on every intended Windows version. XML parsing cannot prove these.
- [ ] Inspect the .sys imports, PE sections/permissions, 4 KiB alignment, NX/CFG,
  and embedded/catalog signatures. Provision the lab's existing test/preproduction
  trust as an owner action; do not change normal-PC security settings.
- [ ] Record exact Intel/AMD PCI IDs, revisions, board/firmware, assigned raw and
  translated resources, firmware/ACPI ownership, OS version and DIMM generation.
  Restrict supported revisions/boards after measurements; source ID checks are
  candidate bounds, not a verified hardware support matrix.
- [ ] Intel: confirm BAR4/HOSTC mode, HSTS read-acquired INUSE semantics, AUXCTL PEC
  exclusion, 20 ms completion/abort behavior and coexistence with firmware/tools.
- [ ] AMD: establish the supported ACPI/PnP route for exclusive PM DecodeEn and
  matching SMBus port resources; otherwise it stays unsupported. Confirm revision
  0x51+ applicability, semaphore/EC ownership, firmware/IMC behavior, port-zero
  selection and abort semantics. No raw/global mapping or IMC write fallback.
- [ ] Confirm DDR3 type recognition and DDR4 SPA0 read-ACK/page semantics. Missing
  page responders stay unsupported rather than interpreting upper-page bytes as
  DDR3. Detect changing/replaced slots and mixed data under firmware page changes. Verify
  per-byte data and CRC against an independent capture. DDR4 upper manufacturing
  page/DDR5 are intentionally unsupported under the no-page-write constraint.
- [ ] With the root shell only: capabilities, missing-source enumeration,
  unknown IOCTLs, malformed/aliased buffers, all reserved fields, huge lengths,
  reserved requests, one-client open, standard-user denial and SYSTEM/admin access.
- [ ] With an approved controller: enumerate/read then close, cancellation at
  every stage, process termination, low-memory failure, repeated open/close,
  sleep/hibernate, surprise removal and resource rebalance. Prove no leaked
  identifier bytes and stale tokens cannot touch hardware.
- [ ] Close clients, remove ALL owned devnodes, confirm control deletion and
  actual unload; no live handles/requests/maps/interface references remain.
  Verify next-boot cleanup for interrupted lab installation. Demand-start alone
  does not stop PnP from starting an installed devnode at boot.
- [ ] Run Driver Verifier on the isolated lab: standard rules, Code Integrity,
  force-pending I/O, low-resource simulation and IRP logging where applicable.
  Record precise enabled rules, results and dumps. The build script only creates
  artifacts; it never enables Verifier.
- [ ] Current WDK static analysis and CodeQL. Record SDV availability; do not
  label unavailable SDV as passed.
- [ ] Run required HLK/HVCI tests with Secure Boot, Memory Integrity and blocklist
  protections enabled. Submit only through the owner's WHCP workflow; record the
  HLK matrix/submission and source/build artifact hashes for every signed rebuild.
- [ ] Verify Microsoft-returned catalog policy/membership and match its artifacts
  to the recorded submission. A valid signature does not establish certification
  or correspondence to an arbitrary later local rebuild.

## Deferred by this implementation brief

Main-app driver transport, anti-cheat gate integration, package verification,
per-collection PnP ownership/removal, crash guard/next-start recovery, masking,
report output, physical DIMM association and measured useful coverage. Those
require a separate app integration package and validated hardware. No production
readiness, hardware support, reliable SCM-only unload or anti-cheat compatibility
is claimed by these host checks.
