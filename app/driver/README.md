# HWID Reader: isolated WP-2B source package

Status: **implemented, pending WDK build/signing and real-hardware validation**.
This package is not installed, loaded, certified, or connected to HWIDChecker.

The kernel driver is independently written **C/KMDF**, built by
`HWIDReader.vcxproj`. C uses the WDK's supported KMDF headers, libraries, MSBuild
targets and static-analysis tooling directly. A Rust kernel implementation would
add `wdk`/`wdk-sys`, binding generation and kernel-specific Cargo configuration
without making kernel compilation possible on this SDK-only machine. The small
dependency-free Rust crate here supplies host-verifiable wire layouts and a
validated SPD reply parser for a later Rust client. Its own `[workspace]` keeps
it separate from `app/rust`; neither main-app Cargo files nor collection code
are changed.

## Layout

- `include/hwid_reader.h`: authoritative fixed ABI, constants and C layout assertions.
- `abi/lib.rs`, `Cargo.toml`: isolated `no_std` Rust contract and host tests.
- `src/driver.c`: DriverEntry, EvtDeviceAdd, one exclusive control object,
  PCI-resource binding, PnP/power rundown, file cleanup and WDF unload callback.
- `src/protocol.{h,c}`: portable dispatcher, session tokens and reply sanitization.
- `src/smbus.{h,c}`: portable, bounded Intel I801 / modern AMD FCH read engine.
- `HWIDReader.inf`: hardware-agnostic `ROOT\HWIDReader` shell only.
- `lab/HWIDReaderControllers.inf`: separately selected, explicit lab controller
  function-driver binding. It is not a filter and replaces that device's existing
  function driver. It is never chosen by the default build.
- `build.ps1`, `check-host.ps1`, `scripts/tools.ps1`, `tests/host.c`:
  build/sign tooling and a fake-register harness using the actual C logic.
- `VALIDATION.md`: measured host checks and pending owner gates.

## Version 1 wire contract

All numbers are little endian, fixed width, naturally aligned for x64. The shared
header is 16 bytes: `version=1`, exact structure `size`, two zero reserved DWORDs.
No pointer, process identifier, PCI BDF, physical address, port number, SMBus
address, offset, caller-selected length, MSR number or transaction opcode appears
in a request. The backend owns targets `0x50..0x57` and all offsets.

| IOCTL | Number | Input | Output |
|---|---|---|---|
| GetCapabilities | `0x80002000` | Header, 16 bytes | Capabilities, 40 bytes |
| EnumerateSpdSlots | `0x80002004` | Header, 16 bytes | Status/count + 32 slots, 792 bytes |
| ReadSpdSlot | `0x80002008` | Header + opaque u64 token, 24 bytes | Status/count/token/generation/flags + 1,024-byte capacity, 1,064 bytes |
| Reserved PPIN | `0x80002040` | Header, 16 bytes | Header/status/reserved, 24 bytes; always unsupported |
| Reserved DSN | `0x80002044` | Header, 16 bytes | Same; always unsupported |

Every code uses a private device/function range, `METHOD_BUFFERED` and, as
explicitly requested for this implementation, `FILE_ANY_ACCESS`. The control
device and both INFs grant access only to SYSTEM and built-in Administrators:
`D:P(A;;GA;;;SY)(A;;GA;;;BA)`. `FILE_DEVICE_SECURE_OPEN` applies the security
descriptor to opens with a suffix too. Only one client can open `\\.\HWIDReader`;
dispatch is sequential and runs at PASSIVE_LEVEL. Read/write/internal-control
requests have no handlers and cannot reach a hardware primitive.

Input lengths must match exactly, including reserved requests. Outputs may be
larger but must hold the complete fixed reply; only its exact size is returned.
Unsupported codes and bad buffers/headers fail at the transport boundary with
zero returned bytes. Recognized hardware failures use a bounded status code in
the fixed reply. Every returned structure, unused slot and unused SPD byte is
zeroed. A failed read returns zero SPD bytes, never a partial identifier capture.

Tokens are minted by the driver per open and enumeration, not supplied targets.
Re-enumeration, file cleanup, a different open, power transitions and controller
removal invalidate them. Counter exhaustion refuses further enumeration/opening.
Slot/controller fields are logical source context, not physical DIMM association
or persistent hardware identities. The Rust parser checks the complete fixed
reply, version, reserved bytes, status, byte count, token, generation, flags and
zero tail before exposing SPD bytes.

## Hardware access and bounds

The root shell has no assigned controller resources, reports no active SPD
feature, and performs no hardware access. A separately selected controller
package must be bound to an owner-approved PCI device. PCI configuration is read
only through that FDO's `BUS_INTERFACE_STANDARD`; there is no PCI scan, CF8/CFC
access, HAL fallback or mapping of unassigned physical memory.

- Intel: `8086:A123`, `8086:9D23`, `8086:A2A3`, SMBus class/header checks,
  enabled I/O decoding and SMBus host mode. BAR4 (`0x20`) supplies SMBBASE;
  HOSTC (`0x40`) must show host enabled without SMI/I2C mode. The corresponding
  raw and translated exclusive PnP I/O resource must match and cover 32 bytes.
- AMD: `1022:790B`, revision **0x51 or later**. Modern FCH uses **PM DecodeEn,
  not an Intel-style PCI BAR**. The driver only reads the documented
  `0xFED80300` PM range if an exclusive translated memory resource assigned to
  this FDO covers it. Decode must already be enabled, port selection must already
  be zero, and an exclusive matching 32-byte I/O resource must also exist.
  A PCI devnode commonly lacks the PM resource, so that board remains
  unsupported. A supported ACPI/resource route needs real board bring-up;
  do not bypass ownership by hardcoding a mapping or using indexed global ports.

The engine acquires Intel INUSE or AMD host/EC semaphore ownership before reads,
refuses a busy/stale/PEC-configured bus, polls without interrupts, and releases
only its own semaphore. Each transaction has a 20 ms deadline; each enumeration
or complete slot read shares one 2,000 ms deadline. Cancellation/closing/power
stop is checked between transactions and during polls. An admitted transaction
can need an additional bounded 20 ms host abort; scheduler latency is not a
hard real-time guarantee. Timeout poisons the backend until removal/rebind.
There is no retry, bus reset, controller enablement, PCI write, mux change or IMC
mailbox programming.

**Read-only means no SMBus slave data-write transactions.** Starting a read
necessarily writes driver-owned host address/command/control registers with the
read bit set. Status acknowledgement, semaphore and abort control writes are
also necessary. No slave payload register is written. EE1004's command phase
selects the byte offset of a read and does not change EEPROM contents.

The first engine returns **256 bytes**. Enumeration requires a successful SPA0
receive-byte acknowledgement before interpreting any memory-type marker. No ACK
is ambiguous (no page responder or DDR4 page one) and stays unsupported, including
legacy DDR3-only buses without a responder. DDR3 is identified by the memory-type
byte; DDR4 also rechecks page zero before and after reading and is returned with
`HR_SPD_PARTIAL`. This is a current
page-zero capture, not a full DDR4 SPD image. **DDR4 manufacturing/serial fields
on the upper page are unavailable**, because changing SPD pages requires a bus
write. DDR5 hubs, other ports/generations, PPIN and DSN remain unsupported. The
slot-generation/page assumptions and arbitration against firmware still need
hardware confirmation; host fixtures cannot establish bus coexistence.

## Build and sign without installing

Run from any directory; scripts are anchored to this directory:

```powershell
pwsh -NoProfile -File D:\GIT\HWID-Privacy\app\driver\check-host.ps1
pwsh -NoProfile -File D:\GIT\HWID-Privacy\app\driver\build.ps1 -CheckPrerequisites
pwsh -NoProfile -File D:\GIT\HWID-Privacy\app\driver\build.ps1
# Existing owner-provisioned certificate; no certificate creation/trust changes:
pwsh -NoProfile -File D:\GIT\HWID-Privacy\app\driver\build.ps1 -Signing Test -CertificateThumbprint <thumbprint>
# Explicitly build the lab controller INF instead of the ROOT shell:
pwsh -NoProfile -File D:\GIT\HWID-Privacy\app\driver\build.ps1 -ControllerPackage
# Verify files returned after the owner's HLK/WHCP submission:
pwsh -NoProfile -File D:\GIT\HWID-Privacy\app\driver\build.ps1 -Signing Whcp -SignedPackage <returned-package-directory>
```

Host checks need only the existing MSVC C++ tools, user-mode SDK and Rust. They
compile the actual dispatcher/bus engine into `out/host/host.exe`, run security
boundary checks, then run Cargo fmt/clippy/test. Rust tests parse a response
produced by that C executable, so run `check-host.ps1` before standalone
`cargo test`. No WDK dependency is involved.

Kernel builds require matching complete SDK/WDK versions, the Visual Studio WDK
extension and x64 C++ tools. Missing WDK produces explicit installation guidance
and exit code 2. The script rebuilds with `/W4 /WX /sdl /GS /guard:cf /kernel`,
static analysis, NX data and 4 KiB image section alignment. It stamps a copied
INF, runs InfVerif and Inf2Cat, and creates a unique local package directory.
KMDF 1.15 is the declared minimum for Windows 10/11. Catalog OS selection is
explicit through `-Inf2CatOs`; catalog generation is not OS/hardware certification.

Unsigned is the default. Test mode signs the .sys before generating/signing its
catalog, using an existing certificate from the current user's My store
(`-MachineCertificate` selects the machine store). WHCP mode verifies a returned
catalog's kernel-policy signature and its .sys/.inf membership. It does not
submit an HLK project, equate `.hlkx` with signed output, prove WHCP certification,
or prove that returned bytes correspond to this local rebuild. The owner must
check the recorded submission/build hashes separately. No mode installs, starts,
enables Driver Verifier, changes security settings, submits, publishes, or copies
anything to the repo-root `HWIDChecker.exe`.

## Lifetime and remaining integration

`StartType=3` is demand start, with no boot/system-start entry. Because this is a
PnP package, **SCM stop alone is not its unload contract**. After the client
closes and pending work drains, the owner must remove all root/controller
devnodes. Last-FDO cleanup deletes the control object, allowing WDF/Windows to
unload; all PCI interface references and PM mappings are released first.
Keeping a PnP devnode installed can let PnP start it on the next boot even with
demand start, so the future per-collection installer must remove owned devnodes.
There are no persistent worker threads/timers or process/image/registry callbacks.

The brief restricts edits to `app/driver/` and WP-2B checklist state. Accordingly,
app transport, service/devnode ownership, anti-cheat gates, guard/recovery and
physical-slot association remain later work. `app/rust/COLLECTION.md` still
accurately describes today's user-mode app; no collector was changed.

## Primary references

- [Plan and research inputs](../../docs/plans/kernel-reader-plan.md), section 8,
  WP-2B; the task's path/access/reserved-slot/backend scope overrides are recorded
  in that package's implementation state.
- [Microsoft PCI configuration access](https://learn.microsoft.com/en-us/windows-hardware/drivers/pci/accessing-pci-device-configuration-space):
  supported device-stack access; unsupported PCI methods can bugcheck under SDEV/VBS.
- [Microsoft KMDF control-object lifetime](https://learn.microsoft.com/en-us/windows-hardware/drivers/wdf/using-control-device-objects).
- [Intel 100-series register datasheet](https://www.intel.com/content/www/us/en/content-details/332691/intel-100-series-chipset-family-platform-controller-hub-pch-datasheet-volume-2.html),
  [Intel 200-series register datasheet](https://cdrdv2-public.intel.com/335193/200-series-chipset-pch-datasheet-vol-2.pdf), SMBus registers.
- [AMD Family 16h BKDG](https://www.amd.com/content/dam/amd/en/documents/archived-tech-docs/programmer-references/52740_16h_Models_30h-3Fh_BKDG.pdf),
  SMBus/PM register descriptions. Newer revision/resource applicability is pending.
- [Linux I801](https://github.com/torvalds/linux/blob/master/drivers/i2c/busses/i2c-i801.c)
  and [PIIX4](https://github.com/torvalds/linux/blob/master/drivers/i2c/busses/i2c-piix4.c)
  register/revision references; independently written code, no source copied.
- [Official PawnIO module inventory](https://github.com/namazso/PawnIO.Modules)
  is a plan research input, not a build dependency or imported implementation.
- [Microsoft WDK installation](https://learn.microsoft.com/en-us/windows-hardware/drivers/download-the-wdk),
  [HVCI requirements](https://learn.microsoft.com/en-us/windows-hardware/test/hlk/testref/driver-compatibility-with-device-guard).
