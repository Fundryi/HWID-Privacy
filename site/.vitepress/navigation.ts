import type { DefaultTheme } from 'vitepress/theme'
import { guides, type GuideRoute } from './theme/guide-data'
import {
  destinationHref,
  getJourney,
  hubHref,
  journeys,
  type ComponentJourney,
  type JourneyDestination,
} from './theme/journey-data'

export interface NavigationItem {
  text: string
  link?: string
  items?: NavigationItem[]
  collapsed?: boolean
  /** Document context, neutral while a section of this document is selected. */
  contextOnly?: boolean
}

const gettingStarted = '/guides/getting-started/getting-started.html'
const repository = 'https://github.com/Fundryi/HWID-Privacy'
const componentLabels: Record<string, string> = {
  motherboard: 'Motherboard / SMBIOS',
  nvram: 'EFI variables',
  tpm: 'TPM',
  ftpm: 'fTPM evidence',
  storage: 'SSD / storage',
  network: 'MAC address',
  router: 'Router / gateway',
  memory: 'RAM / SPD',
  display: 'Monitor / EDID',
}
const componentGroups = [
  { text: 'Firmware', ids: ['motherboard', 'nvram', 'tpm', 'ftpm'] },
  { text: 'Storage', ids: ['storage'] },
  { text: 'Network', ids: ['network', 'router'] },
  { text: 'Peripherals', ids: ['memory', 'display'] },
]

interface OwnedDestination {
  journey: ComponentJourney
  destination: JourneyDestination
  href: string
  owner: GuideRoute
}

// Resolve ownership from the target chapter, never from the chooser that links
// to it. Prefer the owner's own label when several journeys share a section.
const destinations = new Map<string, OwnedDestination>()
for (const journey of journeys) {
  for (const destination of journey.destinations) {
    const href = destinationHref(journey, destination)
    const owner = guides.find((guide) => guide.href === href.split('#')[0])
    if (!owner) throw new Error(`Sidebar destination has no canonical chapter: ${href}`)
    const previous = destinations.get(href)
    if (!previous || (journey.guideId === owner.id && previous.journey.guideId !== owner.id))
      destinations.set(href, { journey, destination, href, owner })
  }
}

function componentNavigation(guide: GuideRoute, activeOwner?: string): NavigationItem {
  const journey = getJourney(guide.id)
  if (!journey) throw new Error(`Sidebar journey not found: ${guide.id}`)
  const used = new Set<string>([guide.href])
  const items: NavigationItem[] = [{ text: 'Full guide', link: guide.href, contextOnly: true }]
  const add = (target: NavigationItem[], text: string, href: string) => {
    if (href.split('#')[0] !== guide.href || used.has(href)) return
    used.add(href)
    target.push({ text, link: href })
  }

  // Preparation belongs before the route choices. External preparation stays
  // under its source chapter in the global tree and in the guide's controls.
  if (!destinations.has(guide.prepare.href))
    add(items, guide.id === 'router' || guide.id === 'display' ? 'Requirements' : 'Prepare', guide.prepare.href)
  if (guide.id === 'network') add(items, 'Address layers', guide.identify.href)
  const sections = new Map<string, NavigationItem[]>()
  for (const section of journey.sections) {
    const label = guide.id === 'network' && section.label === 'Windows and address layers'
      ? 'Windows'
      : section.label
    const links: NavigationItem[] = []
    for (const id of section.destinationIds) {
      const destination = journey.destinations.find((item) => item.id === id)
      if (!destination) throw new Error(`Sidebar section has an unknown destination: ${guide.id}/${id}`)
      const href = destinationHref(journey, destination)
      const canonical = destinations.get(href)!
      if (canonical.owner.id !== guide.id || canonical.destination !== destination) continue
      add(links, destination.label, href)
    }
    if (links.length) sections.set(label, links)
  }

  // These two cross-chapter destinations have explicit source-owned placement.
  // The other chooser remains available through the chapter's return controls.
  for (const canonical of destinations.values()) {
    if (canonical.owner.id !== guide.id || used.has(canonical.href)) continue
    const label = guide.id === 'tpm' && canonical.destination.id === 'ftpm-intel-z790'
      ? 'Intel board report'
      : guide.id === 'ftpm' && canonical.destination.id === 'tpm-amd'
        ? 'AMD'
        : 'Source sections'
    const links = sections.get(label) ?? []
    add(links, canonical.destination.label, canonical.href)
    sections.set(label, links)
  }

  for (const [text, links] of sections) items.push({ text, collapsed: false, items: links })
  add(items, 'Identify', guide.identify.href)
  add(items, guide.id === 'tpm' || guide.id === 'ftpm' ? 'Verify EK / certs' : 'Verify', guide.verify.href)
  if (guide.troubleshoot) add(items, 'Troubleshooting', guide.troubleshoot.href)
  return {
    text: componentLabels[guide.id],
    link: hubHref(guide.id),
    collapsed: activeOwner !== undefined && activeOwner !== guide.id,
    items,
  }
}

function navigationFor(activeOwner?: string): NavigationItem[] {
  return [
    {
      text: 'Start',
      items: [
        { text: 'Start here', link: '/start.html', contextOnly: true },
        { text: 'Guide map', link: '/start.html#guide-map' },
        { text: 'Work order', link: '/start.html#work-order' },
        { text: 'All topics', link: '/devices.html' },
        { text: 'Fundamentals', link: gettingStarted, contextOnly: true },
        { text: 'What an HWID is', link: `${gettingStarted}#what-an-hwid-is` },
        { text: 'Identifier groups', link: `${gettingStarted}#identifier-groups` },
        { text: 'Device restrictions', link: `${gettingStarted}#device-restrictions` },
        { text: 'Safety checklist', link: `${gettingStarted}#safety-checklist` },
        { text: 'Reinstall Windows', link: `${gettingStarted}#clean-windows-reinstall-checklist` },
      ],
    },
    ...componentGroups.map((group) => ({
      text: group.text,
      items: group.ids.map((id) => componentNavigation(guides.find((guide) => guide.id === id)!, activeOwner)),
    })),
    {
      text: 'Reference',
      items: [
        { text: 'Reference index', link: '/reference.html', contextOnly: true },
        { text: 'Tools & downloads', link: '/reference.html#tools-and-downloads' },
        { text: 'Overview', link: '/overview.html' },
        { text: 'Evidence grades', link: `${gettingStarted}#how-to-read-these-guides` },
        { text: 'Plausible values', link: `${gettingStarted}#keep-changed-values-plausible` },
        { text: 'Per-tool traps', link: `${gettingStarted}#per-tool-traps` },
        { text: 'Known-spoofable hardware', link: `${gettingStarted}#known-spoofable-hardware` },
        { text: 'Reported anti-cheat status', link: `${gettingStarted}#reported-anti-cheat-status` },
      ],
    },
    {
      text: 'HWIDChecker',
      items: [
        { text: 'Inspect identifiers', link: `${gettingStarted}#hwidcheckerexe` },
        { text: 'Before / after snapshots', link: `${gettingStarted}#take-before-and-after-snapshots` },
      ],
    },
  ]
}

// Installed VitePress matches these keys against rewritten page.relativePath,
// including .md. Full source stems therefore match both .md and public .html.
export const siteSidebars: Record<string, NavigationItem[]> = { '/': navigationFor() }
for (const guide of guides) {
  const navigation = navigationFor(guide.id)
  siteSidebars[guide.href.replace(/\.html$/, '')] = navigation
  siteSidebars[hubHref(guide.id).replace(/\.html$/, '')] = navigation
}

export const topNavigation: DefaultTheme.NavItem[] = [
  { text: 'Home', link: '/', activeMatch: '^/$' },
  {
    text: 'Start',
    activeMatch: '^/(start|guides/getting-started)',
    items: [
      { text: 'Start here', link: '/start.html' },
      { text: 'Guide map', link: '/start.html#guide-map' },
      { text: 'Work order', link: '/start.html#work-order' },
      { text: 'Fundamentals', link: gettingStarted },
      { text: 'What an HWID is', link: `${gettingStarted}#what-an-hwid-is` },
      { text: 'Device restrictions', link: `${gettingStarted}#device-restrictions` },
      { text: 'Safety checklist', link: `${gettingStarted}#safety-checklist` },
    ],
  },
  {
    text: 'Hardware',
    activeMatch: '^/(hardware|devices|guides/(?!getting-started))',
    items: [
      ...componentGroups.map((group) => ({
        text: group.text,
        items: group.ids.map((id) => ({ text: componentLabels[id], link: hubHref(id) })),
      })),
      { items: [{ text: 'All topics', link: '/devices.html' }] },
    ],
  },
  { text: 'Reinstall', link: `${gettingStarted}#clean-windows-reinstall-checklist` },
  {
    text: 'Reference',
    activeMatch: '^/(reference|overview)',
    items: [
      { text: 'Reference index', link: '/reference.html' },
      { text: 'Tools & downloads', link: '/reference.html#tools-and-downloads' },
      { text: 'Overview', link: '/overview.html' },
      { text: 'Evidence grades', link: `${gettingStarted}#how-to-read-these-guides` },
      { text: 'Identifier groups', link: `${gettingStarted}#identifier-groups` },
      { text: 'Keep values plausible', link: `${gettingStarted}#keep-changed-values-plausible` },
      { text: 'Per-tool traps', link: `${gettingStarted}#per-tool-traps` },
      { text: 'Known-spoofable hardware', link: `${gettingStarted}#known-spoofable-hardware` },
      { text: 'Reported anti-cheat status', link: `${gettingStarted}#reported-anti-cheat-status` },
    ],
  },
  {
    text: 'HWIDChecker',
    items: [
      { text: 'Download HWIDChecker.exe', link: `${repository}/raw/main/HWIDChecker.exe` },
      { text: 'Before and after snapshots', link: `${gettingStarted}#take-before-and-after-snapshots` },
      { text: 'App on the home page', link: '/#hwidchecker' },
      { text: 'Source code', link: `${repository}/tree/main/app/rust` },
    ],
  },
]

export function navigationLinks(items: NavigationItem[]): string[] {
  return items.flatMap((item) => [...(item.link ? [item.link] : []), ...navigationLinks(item.items ?? [])])
}

export const allNavigationLinks = [...new Set([
  ...navigationLinks(siteSidebars['/']),
  ...navigationLinks(topNavigation as NavigationItem[]),
  ...guides.flatMap((guide) => {
    const journey = getJourney(guide.id)!
    return [
      `${hubHref(guide.id)}#decision-tree`,
      ...journey.destinations
        .filter((destination) => destinationHref(journey, destination).split('#')[0] === guide.href)
        .map((destination) => canonicalChooserHref(guide.id, `#${destinationHref(journey, destination).split('#')[1]}`)),
    ]
  }),
])]

/** Return to this chapter's chooser, highlighting only a matching owned node. */
export function canonicalChooserHref(guideId: string, hash: string): string {
  const guide = guides.find((item) => item.id === guideId)
  const journey = getJourney(guideId)
  if (!guide || !journey) throw new Error(`Canonical chooser not found: ${guideId}`)
  const target = `${guide.href}${hash}`
  const destination = hash ? journey.destinations.find((item) => destinationHref(journey, item) === target) : undefined
  const node = destination && journey.nodes.find((item) => item.destinationId === destination.id)
  return `${hubHref(guideId)}#${node ? `node-${node.id}` : 'decision-tree'}`
}
