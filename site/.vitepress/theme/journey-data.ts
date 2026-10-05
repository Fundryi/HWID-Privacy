import { guides } from './guide-data'
import { startJourney, startDestinationHref } from './start-journey'

export type DestinationRole =
  | 'procedure'
  | 'identification'
  | 'background'
  | 'preparation'
  | 'verification'
  | 'research'
  | 'limitation'

export interface JourneyDestination {
  id: string
  label: string
  topicIds: string[]
  href?: string
  role: DestinationRole
  status: string
  note?: string
}

export interface JourneyNode {
  id: string
  kind: 'start' | 'decision' | 'destination'
  label: string
  destinationId?: string
  diagramNote?: string
}

export interface JourneyEdge {
  from: string
  to: string
  label?: string
}

export interface JourneySection {
  label: string
  destinationIds: string[]
}

export interface ComponentJourney {
  guideId: string
  title: string
  intro: string
  nodes: JourneyNode[]
  edges: JourneyEdge[]
  destinations: JourneyDestination[]
  sections: JourneySection[]
}

// Authored source sections outside the existing method/Identify/Prepare/Verify links.
// These are explicit canonical anchors, not generated detection or write procedures.
const auxiliaryHrefs = {
  nvramRequirements: '/guides/nvram-spoofing/nvram-spoofing.html#requirements',
  ftpmMeasure: '/guides/resets/ftpm-reset-tutorial.html#measure-properly-or-you-will-fool-yourself',
  routerBaseline: '/guides/arp-spoofing/arp-spoofing.html#1-record-the-baseline-on-windows-a',
  routerTopology: '/guides/arp-spoofing/arp-spoofing.html#2-build-the-routed-topology-a',
  displayPreparation: '/guides/monitor-spoofing/monitor-spoofing.html#prepare-a-safe-edited-edid',
}

const topics = new Map(guides.flatMap((guide) => guide.methods.map((method) => [method.id, method] as const)))
const allowedHrefs = new Set([
  ...guides.flatMap((guide) => [
    guide.href,
    guide.identify.href,
    guide.prepare.href,
    guide.verify.href,
    ...(guide.troubleshoot ? [guide.troubleshoot.href] : []),
    ...guide.methods.map((method) => method.href),
    `/hardware/${guide.id}.html`,
  ]),
  ...Object.values(auxiliaryHrefs),
])

function guideHref(guideId: string, link: 'identify' | 'prepare'): string {
  const guide = guides.find((candidate) => candidate.id === guideId)
  if (!guide) throw new Error(`Unknown guide: ${guideId}`)
  return guide[link].href
}

function topic(
  id: string,
  label: string,
  role: DestinationRole,
  status: string,
  note?: string,
  topicIds: string[] = [id],
): JourneyDestination {
  return { id, label, topicIds, role, status, ...(note ? { note } : {}) }
}

function auxiliary(
  id: string,
  label: string,
  href: string,
  role: DestinationRole,
  status: string,
  note?: string,
): JourneyDestination {
  return { id, label, topicIds: [], href, role, status, ...(note ? { note } : {}) }
}

function question(id: string, label: string): JourneyNode {
  return { id, kind: 'decision', label }
}

function leaf(id: string, label: string, diagramNote?: string, destinationId: string = id): JourneyNode {
  return { id, kind: 'destination', label, destinationId, ...(diagramNote ? { diagramNote } : {}) }
}

function edge(from: string, to: string, label?: string): JourneyEdge {
  return { from, to, ...(label ? { label } : {}) }
}

// Navigation only. Canonical guides retain the full procedures, warnings and grades.
// Each status names its supported claim; no grade is extracted from a method note.
export const journeys: ComponentJourney[] = [
  {
    guideId: 'motherboard',
    title: 'Motherboard and SMBIOS route',
    intro: 'Identify the fields, exact firmware and recovery route before choosing board-specific work.',
    // Source: motherboard-spoofing.md#what-this-changes, #requirements-and-recovery-preparation,
    // #instructions, #amidewin-and-dmiedit-workflow-notes and the Insyde/ASUS boundaries.
    nodes: [
      { id: 'start', kind: 'start', label: 'Motherboard / SMBIOS' },
      question('firmware', 'Firmware family?'),
      leaf('motherboard-ami-dmiedit', 'AMI DMIEdit', 'Owner report'),
      leaf('motherboard-insyde', 'Insyde H2OSDE', 'Research'),
      leaf('motherboard-asus', 'ASUS ROM / FlashBack', 'Research'),
      leaf('motherboard-unknown', 'Board requirements', 'Identify'),
    ],
    edges: [
      edge('start', 'firmware'),
      edge('start', 'motherboard-asus', 'ASUS ROM work'),
      edge('firmware', 'motherboard-ami-dmiedit'),
      edge('firmware', 'motherboard-insyde'),
      edge('firmware', 'motherboard-unknown', 'Other / unknown'),
    ],
    destinations: [
      auxiliary('fields', 'SMBIOS fields', guideHref('motherboard', 'identify'), 'background', 'SMBIOS structure and field explanation'),
      topic('motherboard-ami-dmiedit', 'AMI DMIEdit', 'procedure', 'Owner hardware report; other-board compatibility unverified', 'Read recovery preparation and review every command before writing.'),
      topic('motherboard-amidewin', 'AMI scope & limits', 'background', 'Command scope and compatibility limits', 'Supporting notes for the AMI workflow, not a second independent procedure.'),
      topic('motherboard-insyde', 'Insyde / H2OSDE', 'research', 'Tool existence documented; end-user availability unverified', 'Model-specific OEM provisioning; no general Phoenix procedure is confirmed.'),
      topic('motherboard-asus', 'ASUS ROM boundary', 'research', 'Official recovery documented; modified-ROM procedure untested', 'A readable dump does not validate a modified image or a flash.'),
      topic('motherboard-unknown', 'Identify & recover', 'preparation', 'Exact model, revision, BIOS and recovery are required', 'A rejected write or unsupported board remains a stop condition.'),
    ],
    sections: [
      { label: 'Identify & recover', destinationIds: ['fields', 'motherboard-unknown'] },
      { label: 'AMI tools', destinationIds: ['motherboard-ami-dmiedit', 'motherboard-amidewin'] },
      { label: 'OEM / board research', destinationIds: ['motherboard-insyde', 'motherboard-asus'] },
    ],
  },
  {
    guideId: 'nvram',
    title: 'EFI-variable inspection route',
    intro: 'Inspect variables read-only and keep boot configuration separate from reported identity-like names.',
    // Source: nvram-spoofing.md#requirements, #read-only-inspection-steps,
    // #standard-boot-variables, #identifier-like-variables-reported-in-third-party-research,
    // #why-this-guide-does-not-delete-variables.
    nodes: [
      { id: 'start', kind: 'start', label: 'EFI variables / NVRAM' },
      question('uefi', 'Live Linux booted via UEFI?'),
      leaf('nvram-read-only', 'List EFI variables', 'Read-only'),
      leaf('requirements', 'UEFI requirements'),
    ],
    edges: [
      edge('start', 'uefi'),
      edge('uefi', 'nvram-read-only', 'Yes'),
      edge('uefi', 'requirements', 'No / unknown'),
    ],
    destinations: [
      auxiliary('requirements', 'Requirements', auxiliaryHrefs.nvramRequirements, 'preparation', 'Native UEFI and read-only evidence preparation'),
      topic('nvram-read-only', 'UEFI inventory', 'procedure', 'Documented read-only inspection', 'HWIDChecker does not enumerate arbitrary EFI variables.'),
      topic('nvram-boot-variables', 'Boot variables', 'background', 'Standard boot configuration, not motherboard serials'),
      topic('nvram-research-variables', 'Reported ID names', 'research', 'Third-party reported meanings remain unverified', 'A variable name does not establish its contents or purpose.'),
      topic('nvram-unsupported-write', 'Write / delete limits', 'limitation', 'No verified generic deletion or restore procedure', 'Only an exact vendor-supported procedure could establish a supported change.'),
    ],
    sections: [
      { label: 'Inspect', destinationIds: ['requirements', 'nvram-read-only'] },
      { label: 'Interpret the inventory', destinationIds: ['nvram-boot-variables', 'nvram-research-variables'] },
      { label: 'Write / delete limits', destinationIds: ['nvram-unsupported-write'] },
    ],
  },
  {
    guideId: 'tpm',
    title: 'TPM identity and implementation route',
    intro: 'Identify the active implementation and distinguish protected state, endorsement keys and certificates.',
    // Source: tpm-spoofing.md#tpm-implementation-types, #what-clearing-the-tpm-changes,
    // #firmware-updates-and-ek-continuity. AMD/Intel evidence belongs to the fTPM chapter.
    nodes: [
      { id: 'start', kind: 'start', label: 'TPM identity' },
      question('technology', 'Which TPM topic?'),
      leaf('tpm-clear', 'TPM clear', 'Read effects'),
      leaf('tpm-update-continuity', 'Firmware update', 'EK continuity'),
      leaf('tpm-amd', 'AMD fTPM observations', 'Reports'),
      leaf('tpm-intel', 'Intel PTT reports', 'Limits'),
      leaf('tpm-types', 'Identify TPM type'),
    ],
    edges: [
      edge('start', 'technology'),
      edge('technology', 'tpm-amd'),
      edge('technology', 'tpm-intel'),
      edge('technology', 'tpm-types'),
      edge('technology', 'tpm-clear'),
      edge('technology', 'tpm-update-continuity'),
    ],
    destinations: [
      topic('tpm-amd', 'AMD fTPM evidence', 'background', 'AMD observations and derivation limits; fTPM chapter', 'Opens the fTPM guide. Exact derivation inputs remain undocumented.'),
      topic('tpm-intel', 'Intel generation limits', 'limitation', 'Generation-specific evidence; fTPM chapter', 'Shared canonical section with ftpm-intel-z890. Do not generalize the MSI Z790 report.'),
      topic('tpm-types', 'dTPM / Pluton types', 'identification', 'Implementation table, not separate rotation procedures', 'Confirm the active implementation in firmware or device documentation.', ['tpm-discrete', 'tpm-pluton']),
      topic('tpm-clear', 'Clear limits', 'limitation', 'Standard clear changes storage state, not EPS / default EK', 'Read the key, recovery and alternate sign-in precautions.'),
      topic('tpm-update-continuity', 'Update continuity', 'background', 'Firmware-version change alone does not prove a new standard EK'),
    ],
    sections: [
      { label: 'Implementation', destinationIds: ['tpm-types'] },
      { label: 'State & identity', destinationIds: ['tpm-clear', 'tpm-update-continuity'] },
      { label: 'Related fTPM chapter', destinationIds: ['tpm-amd', 'tpm-intel'] },
    ],
  },
  {
    guideId: 'ftpm',
    title: 'fTPM platform evidence route',
    intro: 'Choose platform-specific reported evidence, then measure the active key, matching certificate and trust separately.',
    // Source: ftpm-reset-tutorial.md#before-you-reset, #method-a-tpm-b-firmware-flash-cycle,
    // #method-b-pluton-toggle-unverified, #method-c-dtpm-module, #what-does-not-change-the-ek,
    // #intel-z790-vs-z790-era-method-vs-z890. The MSI Z790 report is tpm-spoofing.md#-ftpm-spoofing.
    nodes: [
      { id: 'start', kind: 'start', label: 'fTPM platform evidence' },
      question('technology', 'Which active TPM?'),
      question('intel-generation', 'Intel platform?'),
      leaf('ftpm-amd-tpmb', 'TPM-B BIOS pair', 'Reports'),
      leaf('ftpm-pluton-toggle', 'Pluton toggle', 'Untested'),
      leaf('ftpm-discrete-replacement', 'dTPM module', 'Not recommended'),
      leaf('ftpm-intel-z790', 'MSI Z790 Flash BIOS', 'One report'),
      leaf('ftpm-intel-z890', 'Intel PTT limits', 'Limits'),
      leaf('types', 'Identify TPM type'),
    ],
    edges: [
      edge('start', 'technology'),
      edge('technology', 'ftpm-amd-tpmb', 'AMD fTPM'),
      edge('technology', 'intel-generation', 'Intel PTT'),
      edge('technology', 'ftpm-pluton-toggle', 'Pluton as TPM'),
      edge('technology', 'ftpm-discrete-replacement'),
      edge('technology', 'types', 'Unknown'),
      edge('intel-generation', 'ftpm-intel-z790'),
      edge('intel-generation', 'ftpm-intel-z890', 'Z890 / Arrow Lake'),
      edge('intel-generation', 'ftpm-intel-z890', 'Other / unknown'),
    ],
    destinations: [
      auxiliary('types', 'Identify active TPM', guideHref('ftpm', 'identify'), 'identification', 'Implementation table; TPM chapter'),
      auxiliary('prepare', 'Before any reset', guideHref('ftpm', 'prepare'), 'preparation', 'Recovery, alternate sign-in and paired baseline required'),
      auxiliary('measure', 'Measure first', auxiliaryHrefs.ftpmMeasure, 'verification', 'Separate key, certificate and cached-view measurements', 'A persistent handle or certificate serial is not itself a trust test.'),
      topic('ftpm-amd-tpmb', 'TPM-B reports', 'research', 'Reported identity changes; post-rotation certificate unproven', 'A changelog or AMI prompt alone is not proof. Board rows without individual sources remain unverified.'),
      topic('ftpm-pluton-toggle', 'Pluton research', 'research', 'Overall procedure unverified; one displayed-key report', 'No demonstrated fresh trusted certificate or cross-board repeatability.'),
      topic('ftpm-discrete-replacement', 'dTPM substitution', 'research', 'Different chip identity; not recommended by the guide', 'Modules are board-specific. Read compatibility and attestation limits.'),
      topic('ftpm-intel-z790', 'MSI Z790 report', 'research', 'One MSI Z790 hardware report; TPM chapter', 'Other-board compatibility and the seed-regeneration explanation remain unverified.'),
      topic('ftpm-intel-z890', 'Intel generation limits', 'limitation', 'No verified Z890 rotation in cited 2026-08-21 research', 'Other Intel generations also land at the status section; this does not classify them as Z890. Shared href with tpm-intel.'),
      topic('ftpm-non-rotation', 'Reset / reinstall limits', 'limitation', 'Clear, settings reset and reinstall are not proven EK rotation', 'The source keeps standard-command facts, observations and confounded reinstall reports distinct.'),
    ],
    sections: [
      { label: 'Identify, prepare & measure', destinationIds: ['types', 'prepare', 'measure'] },
      { label: 'AMD', destinationIds: ['ftpm-amd-tpmb', 'ftpm-pluton-toggle'] },
      { label: 'Intel', destinationIds: ['ftpm-intel-z890', 'ftpm-intel-z790'] },
      { label: 'Other paths & limits', destinationIds: ['ftpm-discrete-replacement', 'ftpm-non-rotation'] },
    ],
  },
  {
    guideId: 'storage',
    title: 'Storage identity route',
    intro: 'Choose your SSD, enclosure or volume. Match the controller, NAND, revision and connection before SSD work.',
    // Source: ssd-spoofing.md#which-controller-do-i-have, #m2-ssd-spoofing,
    // #normal-25-ssd-spoofing, #silicon-motion-sm2263xt-notes, #research-candidates,
    // #usb-nvme-enclosures-and-bridge-serials, #raid-disk-identity-and-volume-identity.
    nodes: [
      { id: 'start', kind: 'start', label: 'Storage identity' },
      question('layer', 'SSD, USB enclosure or volume?'),
      question('controller', 'Which SSD / controller?'),
      leaf('storage-map1202', 'MAP1202: MXMPTool', 'Exact controller + NAND'),
      leaf('storage-yansen-kingspec', 'YANSEN / KingSpec: SSDToolKits', 'Exact tested setup'),
      leaf('storage-sm2263xt', 'SM2263XT MP tool', 'Untested'),
      leaf('storage-bridges', 'RTL9210B / TUSB926x', 'Bridge only · RTL untested'),
      leaf('storage-raid-volume', 'RAID / partitions / volumes'),
      leaf('storage-research', 'MAP1602 / SM2269XT / IG5236', 'Research only'),
      leaf('storage-unknown', 'Check controller + NAND'),
    ],
    edges: [
      edge('start', 'layer'),
      edge('layer', 'controller', 'SSD'),
      edge('layer', 'storage-bridges', 'USB enclosure'),
      edge('layer', 'storage-raid-volume'),
      edge('layer', 'storage-unknown', 'Not sure'),
      edge('controller', 'storage-map1202'),
      edge('controller', 'storage-yansen-kingspec', 'ASMT 2115 bridge'),
      edge('controller', 'storage-sm2263xt'),
      edge('controller', 'storage-research', 'Other controller'),
      edge('controller', 'storage-unknown', 'Not sure'),
    ],
    destinations: [
      topic('storage-map1202', 'MAP1202', 'procedure', 'Owner-tested MAP1202 setup; not independently repeated', 'Read the destructive-operation warning and local Prerequisites. Match controller and NAND, not just the retail name.'),
      topic('storage-yansen-kingspec', 'YANSEN / KingSpec SATA', 'procedure', 'Owner-tested drive / ASMT 2115 setup; other stock unverified', 'Read the SATA warning and Prerequisites; the retail listing does not establish a controller or NAND match.'),
      topic('storage-sm2263xt', 'SM2263XT research', 'research', 'Controller-specific programming procedure untested', 'Controller, NAND and exact PCB revision must match the package.'),
      topic('storage-bridges', 'RTL9210B / TUSB926x', 'research', 'RTL bridge workflow untested; TI descriptor example documented', 'One shared section. Neither changes the native SSD behind the bridge.', ['storage-rtl9210b', 'storage-tusb926x']),
      topic('storage-raid-volume', 'RAID / volumes', 'background', 'Logical identities are separate from native drive identity'),
      topic('storage-research', 'MAP1602 / SM2269XT / IG5236 / limits', 'research', 'Bring-up / recovery evidence; no guide-supported identity change', 'Shared table for MAP1602, SM2269XT, IG5236, Realtek NVMe and Phison. No separate identity-write recipes.', ['storage-map1602', 'storage-sm2269xt', 'storage-ig5236', 'storage-unsupported']),
      topic('storage-unknown', 'Identify controller', 'identification', 'Exact controller, NAND, revision, firmware and transport required', 'Stop if the package is hidden or the match cannot be confirmed.'),
    ],
    sections: [
      { label: 'Native drive routes', destinationIds: ['storage-map1202', 'storage-yansen-kingspec', 'storage-sm2263xt'] },
      { label: 'Bridge / logical layers', destinationIds: ['storage-bridges', 'storage-raid-volume'] },
      { label: 'Other controllers', destinationIds: ['storage-research'] },
      { label: 'Identify controller', destinationIds: ['storage-unknown'] },
    ],
  },
  {
    guideId: 'network',
    title: 'MAC address route',
    intro: 'Choose a Windows override or your NIC. Device programming depends on the exact controller, tool and storage.',
    // Source: mac-spoofing.md#current-mac-permanent-mac-and-burned-in-storage,
    // Windows, Intel, Realtek, USB and ConnectX-3 sections; shared limits are
    // #controller-storage-efuse-eeprom-or-flash, not a generic NIC-detection tutorial.
    nodes: [
      { id: 'start', kind: 'start', label: 'MAC address' },
      question('connection', 'Windows setting or adapter?'),
      question('internal', 'Which onboard / PCIe NIC?'),
      question('usb', 'Which USB adapter?'),
      leaf('network-windows', 'Windows MAC override', 'Software only'),
      leaf('network-intel', 'Intel EEUPDATE', 'Exact controller · Untested'),
      leaf('network-realtek-pcie', 'Realtek PG', 'Exact CFG · Untested'),
      leaf('network-realtek-usb', 'Realtek USB PG', 'Single RTL8153 report'),
      leaf('network-tplink-ue300', 'TP-Link UE300', 'Single report · check revision'),
      leaf('network-asix-original', 'AX88179: ASIXFlash / Captain', 'Exact tool / storage'),
      leaf('network-asix-ab', 'AX88179A / B', 'Exact tool / storage'),
      leaf('network-connectx3', 'CX311A: WinMFT flint', 'Exact model'),
      leaf('network-storage-limits', 'RTL8126 / AQC113 / unknown', 'Limits'),
    ],
    edges: [
      edge('start', 'connection'),
      edge('connection', 'network-windows'),
      edge('connection', 'internal', 'Onboard / PCIe NIC'),
      edge('connection', 'usb', 'USB NIC'),
      edge('connection', 'network-storage-limits', 'Not sure'),
      edge('internal', 'network-intel'),
      edge('internal', 'network-realtek-pcie', 'Realtek (not RTL8126)'),
      edge('internal', 'network-connectx3'),
      edge('internal', 'network-storage-limits'),
      edge('usb', 'network-tplink-ue300'),
      edge('usb', 'network-realtek-usb', 'Realtek RTL8153 / RTL8156'),
      edge('usb', 'network-asix-original'),
      edge('usb', 'network-asix-ab'),
      edge('usb', 'network-storage-limits', 'Other / not sure'),
    ],
    destinations: [
      auxiliary('layers', 'Address layers', guideHref('network', 'identify'), 'background', 'Current, permanent and stored addresses are separate views'),
      topic('network-windows', 'Software override', 'procedure', 'Documented Windows mechanism; driver support varies', 'Changes the current address without rewriting NIC storage; applying it restarts the adapter.'),
      topic('network-intel', 'Intel EEUPDATE', 'procedure', 'General programming procedure untested', 'Exact controller, utility version, locks and backup caveats apply.'),
      topic('network-realtek-pcie', 'Realtek PG', 'procedure', 'General workflow untested; exact silicon / configuration required', 'RTL8125-family configuration does not establish RTL8126 support.'),
      topic('network-realtek-usb', 'RTL8153 / RTL8156', 'research', 'Named Belkin RTL8153 contributor report; not repeated here', 'The tool-version and OTP/eFuse limits remain explicit; do not extend the report to every RTL8156 adapter.'),
      topic('network-tplink-ue300', 'TP-Link UE300', 'research', 'One report; revision and active storage unconfirmed', 'Record the printed revision and matched hardware before considering its cautious Realtek workflow.'),
      topic('network-asix-original', 'ASIX AX88179', 'research', 'Third-party Captain report; exact controller / storage required', 'External EEPROM and embedded eFuse are different storage paths.'),
      topic('network-asix-ab', 'AX88179A / B', 'background', 'Vendor storage model; exact tooling / adapter support required', 'A/B bundled-tool limits are not a vendor-wide rule. eFuse cannot be erased.'),
      topic('network-connectx3', 'ConnectX-3', 'procedure', 'Named CX311A single-port hardware test', 'Read WinOF / WinMFT Prerequisites, firmware backup and image-file checks before flashing.'),
      topic('network-storage-limits', 'Controller storage & limits', 'limitation', 'RTL8126 factory provisioning / AQC113 recovery / unknown storage', 'One shared section. Exact controller/card, adapter revision, hardware IDs, tool version and storage mode are needed; no generic detection commands are added.', ['network-rtl8126', 'network-aquantia', 'network-unknown']),
    ],
    sections: [
      { label: 'Windows and address layers', destinationIds: ['layers', 'network-windows'] },
      { label: 'Internal / PCIe', destinationIds: ['network-intel', 'network-realtek-pcie', 'network-connectx3'] },
      { label: 'USB adapters', destinationIds: ['network-realtek-usb', 'network-tplink-ue300', 'network-asix-original', 'network-asix-ab'] },
      { label: 'RTL8126 / AQC113 / other', destinationIds: ['network-storage-limits'] },
    ],
  },
  {
    guideId: 'router',
    title: 'Router and first-hop isolation route',
    intro: 'Use a router you own or administer in routed mode. Choose its setup, then check the gateway and IPv6 if enabled.',
    // Source: arp-spoofing.md#requirements, #1-record-the-baseline-on-windows-a,
    // #2-build-the-routed-topology-a,
    // #3a-configure-a-glinet-travel-router-a, #3b-configure-another-openwrt-router-a,
    // #3c-configure-a-raspberry-pi-4-model-b-a, #4-decide-how-to-handle-ipv6-a.
    nodes: [
      { id: 'start', kind: 'start', label: 'Router / gateway isolation' },
      question('device', 'Which router / setup?'),
      leaf('router-unknown', 'Other / unknown router', 'Check routed mode'),
      leaf('router-glinet', 'GL.iNet', 'Router / WISP'),
      leaf('router-openwrt', 'OpenWrt router', 'Routed LAN'),
      leaf('router-pi4', 'Pi 4: NetworkManager', 'Shared IPv4 · Bookworm+'),
      leaf('router-ipv6', 'Check IPv6 route', 'If enabled'),
    ],
    edges: [
      edge('start', 'device'),
      edge('device', 'router-glinet'),
      edge('device', 'router-openwrt'),
      edge('device', 'router-pi4'),
      edge('device', 'router-unknown'),
      edge('router-glinet', 'router-ipv6'),
      edge('router-openwrt', 'router-ipv6'),
      edge('router-pi4', 'router-ipv6'),
    ],
    destinations: [
      auxiliary('baseline', 'Record Windows baseline', auxiliaryHrefs.routerBaseline, 'preparation', 'Record routes, gateway and IPv4 / IPv6 neighbors', 'For Wi-Fi also record the current association. Keep real network identifiers outside the repository.'),
      auxiliary('topology', 'Routed topology', auxiliaryHrefs.routerTopology, 'preparation', 'Separate routed uplink / downstream required', 'Read Requirements before choosing a device configuration.'),
      topic('router-glinet', 'GL.iNet', 'procedure', 'Vendor-documented Router / WISP configuration', 'The uplink MAC control alone does not change the PC-visible LAN gateway MAC.'),
      topic('router-openwrt', 'OpenWrt', 'procedure', 'Documented exact LAN-device configuration', 'Device names and port layouts vary; do not assume an anonymous interface index.'),
      topic('router-pi4', 'Raspberry Pi 4', 'procedure', 'Documented Wi-Fi-uplink / Ethernet-downstream IPv4 example', 'An all-wired topology needs a second supported interface. This is routing, not bridging.'),
      topic('router-ipv6', 'IPv6 boundary', 'verification', 'Separate required after-route IPv6 check', 'No universal GL.iNet / OpenWrt IPv6 toggle is established.'),
      topic('router-unknown', 'Routed vs bridge', 'identification', 'Routing boundary explanation; not a generic router recipe'),
    ],
    sections: [
      { label: 'Before configuration', destinationIds: ['baseline', 'topology'] },
      { label: 'Configure', destinationIds: ['router-glinet', 'router-openwrt', 'router-pi4'] },
      { label: 'Boundary & after-route checks', destinationIds: ['router-unknown', 'router-ipv6'] },
    ],
  },
  {
    guideId: 'memory',
    title: 'RAM and SPD route',
    intro: 'Measure the chosen observation layer, review exact-module options, and treat programmer support and protection as prerequisites.',
    // Source: ram-spoofing.md#read-only-baseline, #lowest-risk-options, #requirements,
    // #ddr4-ee1004-protection, #ddr5-spd5118-protection, #external-programmer-procedure, #tools.
    nodes: [
      { id: 'start', kind: 'start', label: 'RAM / SPD' },
      leaf('baseline', 'Windows RAM serials', 'Read-only'),
      question('action', 'Keep / replace, or write SPD?'),
      question('generation', 'DDR4 or DDR5?'),
      leaf('memory-no-write', 'Keep / replace RAM', 'Review options'),
      leaf('memory-ddr4', 'DDR4 EE1004', 'Protection'),
      leaf('memory-ddr5', 'DDR5 SPD5118', 'Protection'),
      leaf('memory-programmer', 'SPD programmer', 'Untested'),
      leaf('memory-unknown', 'Exact SPD-device requirements'),
    ],
    edges: [
      edge('start', 'baseline'),
      edge('baseline', 'action'),
      edge('action', 'memory-no-write'),
      edge('action', 'generation', 'Write SPD'),
      edge('action', 'memory-unknown', 'Not sure'),
      edge('generation', 'memory-ddr4'),
      edge('generation', 'memory-ddr5'),
      edge('generation', 'memory-unknown', 'Unknown'),
      edge('memory-ddr4', 'memory-programmer', 'Read exact prerequisites'),
      edge('memory-ddr5', 'memory-programmer', 'Read exact prerequisites'),
    ],
    destinations: [
      auxiliary('baseline', 'Read-only baseline', guideHref('memory', 'identify'), 'identification', 'Windows / SMBIOS view, not independent raw SPD'),
      topic('memory-no-write', 'Existing / replacement', 'background', 'Review options; measure the exact module in the chosen layer', 'A willingness to replace or one blank Windows field does not complete verification. Retail families are not a guarantee.'),
      topic('memory-ddr4', 'DDR4 protection', 'background', 'Documented device protection; prerequisite, not a separate write route'),
      topic('memory-ddr5', 'DDR5 protection', 'background', 'Documented hub protection; prerequisite, not a separate write route', 'Offline-tester support is needed to clear protected block 8, not universally for an already writable block.'),
      topic('memory-programmer', 'External workflow [S]', 'research', 'Common external-programmer procedure untested', 'Requires exact device support, two matching full reads and external recovery. A read does not establish write permission.'),
      topic('memory-tools', 'Tools & support', 'background', 'Read-only views and DDR4 programmer example', 'The documented DDR4 example does not guarantee third-party-module or DDR5 support.'),
      topic('memory-unknown', 'Programmer requirements', 'preparation', 'Missing, failed or unsupported prerequisites are stop conditions'),
    ],
    sections: [
      { label: 'Inspect & review options', destinationIds: ['baseline', 'memory-no-write'] },
      { label: 'Before writing', destinationIds: ['memory-ddr4', 'memory-ddr5', 'memory-unknown', 'memory-tools'] },
      { label: 'Common external workflow', destinationIds: ['memory-programmer'] },
    ],
  },
  {
    guideId: 'display',
    title: 'Monitor and EDID route',
    intro: 'Choose Windows settings, an HDMI emulator or the monitor EEPROM. Keep the full EDID and signal features.',
    // Source: monitor-spoofing.md#how-monitor-identity-is-stored, #prepare-a-safe-edited-edid,
    // #option-1-windows-software-override, #dr-hdmi-4k, #dr-hdmi-8k,
    // #generic-hdmi-edid-adapters, #dichen-5-programmable-fuser,
    // #option-4-direct-monitor-eeprom-modification, #ddc-ddcci-hdmi-and-displayport.
    nodes: [
      { id: 'start', kind: 'start', label: 'Monitor / EDID' },
      question('scope', 'Which EDID method?'),
      question('device', 'Which HDMI emulator?'),
      leaf('identity', 'Check monitor / input'),
      leaf('display-windows', 'CRU / monitor INF', 'Windows only'),
      leaf('display-drhdmi4k', 'Dr HDMI 4K', '256-byte EDID'),
      leaf('display-drhdmi8k', 'Dr HDMI 8K', 'Check firmware / capacity'),
      leaf('display-generic', 'Other HDMI adapter', 'Programming unverified'),
      leaf('display-dichen', 'Dichen 5', 'Untested'),
      leaf('display-eeprom', 'VG248QE EEPROM', '2013 report · untested here'),
      leaf('display-displayport', 'Native DisplayPort / DDC', 'HDMI-only limit'),
    ],
    edges: [
      edge('start', 'scope'),
      edge('scope', 'display-windows'),
      edge('scope', 'device', 'HDMI emulator'),
      edge('scope', 'display-eeprom'),
      edge('scope', 'display-displayport'),
      edge('scope', 'identity', 'Input / method unknown'),
      edge('device', 'display-drhdmi4k'),
      edge('device', 'display-drhdmi8k'),
      edge('device', 'display-dichen'),
      edge('device', 'display-generic'),
    ],
    destinations: [
      auxiliary('identity', 'Identify EDID / input', guideHref('display', 'identify'), 'identification', 'Complete descriptor and exact connection-path scope'),
      auxiliary('prepare-edid', 'Prepare EDID', auxiliaryHrefs.displayPreparation, 'preparation', 'Capture, edit minimally and validate every descriptor block'),
      topic('display-windows', 'CRU / INF override', 'procedure', 'Documented Windows override; driver / path support varies', 'Does not rewrite monitor hardware.'),
      topic('display-drhdmi4k', 'Dr HDMI 4K', 'procedure', 'Vendor mechanism; 256-byte capacity and signal fit required', 'Do not truncate a larger EDID to fit the device.'),
      topic('display-drhdmi8k', 'Dr HDMI 8K', 'procedure', 'Vendor mechanism; firmware and complete EDID capacity required', 'Firmware 1.4 adds documented 384 / 512-byte support. Signal-feature support is a separate device requirement.'),
      topic('display-generic', 'Generic adapter limits', 'limitation', 'No generic programming model or software verified', 'Presets or sink copy do not establish user-programmable EDID.'),
      topic('display-dichen', 'Dichen research', 'research', 'Dichen-specific actions remain untested', 'Authoritative capacity, utility and recovery evidence is missing.'),
      topic('display-eeprom', 'Direct EEPROM research', 'research', 'ASUS VG248QE 2013 third-party report; untested here', 'Does not establish another board revision, monitor model or DisplayPort path.'),
      topic('display-displayport', 'DisplayPort / DDC limits', 'limitation', 'HDMI-only emulation does not intercept native DisplayPort', 'Working DDC/CI controls do not establish EEPROM write access.'),
    ],
    sections: [
      { label: 'Identify & prepare', destinationIds: ['identity', 'prepare-edid'] },
      { label: 'Windows', destinationIds: ['display-windows'] },
      { label: 'Inline HDMI', destinationIds: ['display-drhdmi4k', 'display-drhdmi8k', 'display-generic', 'display-dichen'] },
      { label: 'Other paths', destinationIds: ['display-eeprom', 'display-displayport'] },
    ],
  },
]

export function getJourney(id: string): ComponentJourney | undefined {
  return id === 'start' ? startJourney : journeys.find((journey) => journey.guideId === id)
}

export function hubHref(guideId: string): string {
  if (!guides.some((guide) => guide.id === guideId)) throw new Error(`Unknown guide: ${guideId}`)
  return `/hardware/${guideId}.html`
}

export function destinationHref(journey: ComponentJourney, destination: JourneyDestination): string {
  if (journey === startJourney) return startDestinationHref(destination)
  const topicHrefs = destination.topicIds.map((id) => {
    const method = topics.get(id)
    if (!method) throw new Error(`Unknown topic ${id} in ${journey.guideId}/${destination.id}`)
    return method.href
  })
  const href = topicHrefs[0] ?? destination.href
  if (!href || !allowedHrefs.has(href)) {
    throw new Error(`Unknown destination href in ${journey.guideId}/${destination.id}`)
  }
  if (topicHrefs.some((candidate) => candidate !== href) || (destination.href && destination.href !== href)) {
    throw new Error(`Shared destination href mismatch in ${journey.guideId}/${destination.id}`)
  }
  return href
}

function mermaidLabel(value: string): string {
  return value
    .replace(/&/g, '#amp;')
    .replace(/"/g, '#quot;')
    .replace(/</g, '#60;')
    .replace(/>/g, '#62;')
    .replace(/\\/g, '#92;')
    .replace(/\|/g, '#124;')
    .replace(/`/g, '#96;')
    .replace(/[\r\n]+/g, ' ')
}

export function mermaidSource(journey: ComponentJourney, linkMode: 'site' | 'github' = 'site'): string {
  if (linkMode !== 'site' && linkMode !== 'github') throw new Error(`Unknown link mode: ${linkMode}`)
  const safeId = (id: string): string => {
    if (!/^[a-z][a-z0-9-]*$/.test(id)) throw new Error(`Invalid journey ID: ${id}`)
    return id.replace(/-/g, '_')
  }
  const prefix = `j_${safeId(journey.guideId)}_`
  const nodeId = (id: string): string => `${prefix}${safeId(id)}`
  const nodes = new Map(journey.nodes.map((node) => [node.id, node]))
  const destinations = new Map(journey.destinations.map((destination) => [destination.id, destination]))
  if (nodes.size !== journey.nodes.length || destinations.size !== journey.destinations.length) {
    throw new Error(`Duplicate node or destination ID in ${journey.guideId}`)
  }
  const lines = [
    'flowchart TD',
    `  accTitle: ${mermaidLabel(journey.title)}`,
    `  accDescr: ${mermaidLabel(journey.intro)}`,
  ]
  const links: string[] = []
  for (const node of journey.nodes) {
    const id = nodeId(node.id)
    const label = mermaidLabel(node.label)
    if (node.kind === 'start') {
      lines.push(`  ${id}(["${label}"])`)
    } else if (node.kind === 'decision') {
      lines.push(`  ${id}{"${label}"}`)
    } else {
      const destination = node.destinationId ? destinations.get(node.destinationId) : undefined
      if (!destination) throw new Error(`Missing destination for ${journey.guideId}/${node.id}`)
      const href = destinationHref(journey, destination)
      const target = linkMode === 'github' ? `https://hwid.idkzal.cc${href}` : href
      // Mermaid's plain-SVG text path recognizes literal \n without HTML labels.
      // The map names the destination and, when needed, one short limitation.
      // Full evidence notes stay in the supporting links and the guide itself.
      const destinationLabel = node.diagramNote ? `${label}\\n${mermaidLabel(node.diagramNote)}` : label
      lines.push(`  ${id}["${destinationLabel}"]`)
      lines.push(`  class ${id} journey-${destination.role}`)
      links.push(`  click ${id} href "${target}" "${mermaidLabel(destination.label)}" _self`)
    }
  }
  for (const connection of journey.edges) {
    if (!nodes.has(connection.from) || !nodes.has(connection.to)) {
      throw new Error(`Dangling edge in ${journey.guideId}: ${connection.from} -> ${connection.to}`)
    }
    if (nodes.get(connection.from)?.kind === 'decision' && !(connection.label || nodes.get(connection.to)?.label)) {
      throw new Error(`Unlabeled decision edge in ${journey.guideId}/${connection.from}`)
    }
    const label = connection.label ? `|"${mermaidLabel(connection.label)}"|` : ''
    lines.push(`  ${nodeId(connection.from)} -->${label} ${nodeId(connection.to)}`)
  }
  return [...lines, ...links].join('\n')
}
