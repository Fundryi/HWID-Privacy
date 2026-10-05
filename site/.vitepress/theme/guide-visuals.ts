import type { IconName } from './icons'

export interface GuideVisualData {
  id: string
  afterHeading: string
  title: string
  kind: 'comparison' | 'flow'
  items: { icon: IconName; title: string; text: string }[]
  note: string
  sources: { label: string; href: string }[]
}

// These summaries follow the canonical guide sections and their evidence limits.
// Refresh the matching entry whenever those sections change. A visual is not
// an additional procedure, a hardware test, or an independent verification.
export const guideVisuals: Record<string, GuideVisualData> = {
  'guides/nvram-spoofing/nvram-spoofing.md': {
    id: 'nvram-evidence-sources',
    afterHeading: 'Verify with HWIDChecker',
    title: 'Two separate evidence sources',
    kind: 'comparison',
    items: [
      {
        icon: 'nvram',
        title: 'EFI-variable hash',
        text: 'Private sha256sum output shows whether a selected EFI variable changed.',
      },
      {
        icon: 'inspect',
        title: 'HWIDChecker view',
        text: '(SM)BIOS, MOTHERBOARD and CHASSIS values show whether those hardware views changed.',
      },
    ],
    note: 'HWIDChecker does not enumerate arbitrary EFI variables. A changed variable hash alone does not prove a hardware-identity change; unchanged HWIDChecker output does not prove NVRAM is unchanged. Neither source is complete. Keep inspection read-only unless a vendor publishes a model-specific procedure and recovery instructions.',
    sources: [
      { label: 'Verification boundary', href: '/guides/nvram-spoofing/nvram-spoofing.html#verify-with-hwidchecker' },
      { label: 'Read-only scope', href: '/guides/nvram-spoofing/nvram-spoofing.html#why-this-guide-does-not-delete-variables' },
    ],
  },
  'guides/tpm-spoofing/tpm-spoofing.md': {
    id: 'tpm-clear-boundary',
    afterHeading: 'What clearing the TPM changes',
    title: 'Standard clear and endorsement identity',
    kind: 'comparison',
    items: [
      {
        icon: 'nvram',
        title: 'Storage state: SPS / SRK',
        text: 'Standard clear changes storage state and invalidates previous-owner keys.',
      },
      {
        icon: 'tpm',
        title: 'Endorsement identity: EPS / EK',
        text: 'Standard clear does not replace EPS or its default EK.',
      },
    ],
    note: 'TPM2_Clear does not call TPM2_ChangeEPS. TPM readiness, EK public-key hash and certificate details are different measurements. Clearing can destroy TPM-created keys and access to data protected only by them; follow the existing recovery and sign-in precautions before clearing.',
    sources: [
      { label: 'Clear behavior and precautions', href: '/guides/tpm-spoofing/tpm-spoofing.html#what-clearing-the-tpm-changes' },
      { label: 'Keys and certificates', href: '/guides/tpm-spoofing/tpm-spoofing.html#tpm-identity-and-terminology' },
      { label: 'Windows measurements', href: '/guides/tpm-spoofing/tpm-spoofing.html#inspect-the-tpm-in-windows' },
    ],
  },
  'guides/ssd-spoofing/ssd-spoofing.md': {
    id: 'storage-identity-layers',
    afterHeading: 'What a storage device can expose',
    title: 'Separate storage identities',
    kind: 'comparison',
    items: [
      {
        icon: 'storage',
        title: 'Native NVMe',
        text: 'Subsystem SN; namespace EUI-64, NGUID or UUID.',
      },
      {
        icon: 'method',
        title: 'USB bridge',
        text: 'Enclosure USB/SCSI strings and bridge serial.',
      },
      {
        icon: 'overview',
        title: 'Disk layout',
        text: 'GPT disk GUID or MBR signature; partition GUIDs.',
      },
      {
        icon: 'sources',
        title: 'Formatted volume',
        text: 'Volume serial, separate from the drive serial.',
      },
    ],
    note: 'A bridge or volume change does not rewrite the SSD. Compare the same connection path; a blank field can mean missing pass-through. The USB bridge workflow remains untested [S]. A green PASS indicator is not proof by itself.',
    sources: [
      { label: 'Storage identity layers', href: '/guides/ssd-spoofing/ssd-spoofing.html#what-a-storage-device-can-expose' },
      { label: 'USB bridge limits', href: '/guides/ssd-spoofing/ssd-spoofing.html#usb-nvme-enclosures-and-bridge-serials' },
      { label: 'Verification', href: '/guides/ssd-spoofing/ssd-spoofing.html#verify-the-result' },
    ],
  },
  'guides/mac-spoofing/mac-spoofing.md': {
    id: 'mac-address-layers',
    afterHeading: 'Current MAC, Permanent MAC, and Burned-In Storage',
    title: 'Storage and two Windows attributes',
    kind: 'comparison',
    items: [
      {
        icon: 'nvram',
        title: 'Device storage',
        text: 'EEPROM, flash or eFuse; board-specific.',
      },
      {
        icon: 'network',
        title: 'Permanent MAC',
        text: 'Driver-visible permanent attribute; may be unavailable.',
      },
      {
        icon: 'inspect',
        title: 'Current MAC',
        text: 'Active Windows address; NetworkAddress can override it.',
      },
    ],
    note: 'A Windows override can survive reboot without rewriting device storage. Driver readbacks do not prove a firmware write. Permanent MAC: Unavailable remains a possible result; a controller name alone does not identify the storage used on a finished adapter.',
    sources: [
      { label: 'MAC address layers', href: '/guides/mac-spoofing/mac-spoofing.html#current-mac-permanent-mac-and-burned-in-storage' },
      { label: 'Windows override', href: '/guides/mac-spoofing/mac-spoofing.html#windows-networkaddress-override-software-only' },
      { label: 'Driver readback limits', href: '/guides/mac-spoofing/mac-spoofing.html#3-check-with-hwidchecker' },
    ],
  },
  'guides/ram-spoofing/ram-spoofing.md': {
    id: 'ram-observation-path',
    afterHeading: 'Scope and data path',
    title: 'RAM identity data path',
    kind: 'flow',
    items: [
      {
        icon: 'memory',
        title: 'Module SPD',
        text: 'Configuration and manufacturing data.',
      },
      {
        icon: 'motherboard',
        title: 'Firmware: Type 17',
        text: 'Firmware builds the SMBIOS memory-device record.',
      },
      {
        icon: 'overview',
        title: 'Windows: WMI',
        text: 'Win32_PhysicalMemory exposes the SMBIOS values.',
      },
      {
        icon: 'inspect',
        title: 'HWIDChecker',
        text: 'Locator, manufacturer, part, capacity and serial.',
      },
    ],
    note: 'PowerShell and HWIDChecker share this observation layer. Agreement is not an independent raw-SPD comparison. Firmware need not expose every SPD byte or preserve its raw formatting. The external-programmer write and raw-readback workflow remains untested [S].',
    sources: [
      { label: 'Observation path', href: '/guides/ram-spoofing/ram-spoofing.html#scope-and-data-path' },
      { label: 'External readback boundary', href: '/guides/ram-spoofing/ram-spoofing.html#verify-the-result' },
    ],
  },
  'guides/monitor-spoofing/monitor-spoofing.md': {
    id: 'monitor-change-scope',
    afterHeading: 'What this changes',
    title: 'Where the EDID change lives',
    kind: 'comparison',
    items: [
      {
        icon: 'overview',
        title: 'Windows override',
        text: 'One Windows display stack; monitor unchanged.',
      },
      {
        icon: 'method',
        title: 'Inline HDMI emulator',
        text: 'Sources connected through the emulator.',
      },
      {
        icon: 'nvram',
        title: 'Monitor EEPROM',
        text: 'Modified input or board path; model-specific.',
      },
    ],
    note: 'Visibility follows the method and connection path. An HDMI emulator does not automatically intercept native DisplayPort. The direct-EEPROM example is a first-hand 2013 ASUS VG248QE report [C]. It is untested by this project and retains the existing hardware and recovery warning. Preserve every timing and capability block unless you intentionally want different behavior.',
    sources: [
      { label: 'Method scope', href: '/guides/monitor-spoofing/monitor-spoofing.html#what-this-changes' },
      { label: 'HDMI and DisplayPort paths', href: '/guides/monitor-spoofing/monitor-spoofing.html#ddc-ddcci-hdmi-and-displayport' },
      { label: 'EEPROM research boundary', href: '/guides/monitor-spoofing/monitor-spoofing.html#option-4-direct-monitor-eeprom-modification' },
    ],
  },
}
