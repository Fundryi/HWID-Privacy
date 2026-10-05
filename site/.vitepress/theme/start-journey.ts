import type { ComponentJourney, JourneyDestination, DestinationRole } from './journey-data'
import { guides, workflowSteps } from './guide-data'

// This is navigation through existing guides, not a new hardware procedure.
// The full ten-stage workflow and its evidence warning remain on Start here.
function stageLink(id: string, index = 0): string {
  const href = workflowSteps.find((step) => step.id === id)?.links[index]?.href
  if (!href) throw new Error('Missing workflow link: ' + id)
  return href
}
function chooserHref(id: string): string {
  if (!guides.some((guide) => guide.id === id)) throw new Error('Unknown component: ' + id)
  return '/hardware/' + id + '.html'
}
const fundamentals = stageLink('stage-1').split('#')[0]
const startHrefs = {
  inspect: fundamentals + '#hwidcheckerexe',
  nvram: chooserHref('nvram'),
  'stage-2': stageLink('stage-2'),
  'stage-3': stageLink('stage-3'),
  'stage-4': stageLink('stage-4'),
  motherboard: chooserHref('motherboard'),
  tpm: chooserHref('tpm'),
  ftpm: chooserHref('ftpm'),
  storage: chooserHref('storage'),
  network: chooserHref('network'),
  memory: chooserHref('memory'),
  display: chooserHref('display'),
  router: chooserHref('router'),
  'stage-8': '/start.html#stage-8',
  'stage-9': stageLink('stage-9'),
  'stage-10': stageLink('stage-10', 1),
} satisfies Record<string, string>

type DestinationId = keyof typeof startHrefs
function destination(id: DestinationId, label: string, role: DestinationRole, status: string): JourneyDestination {
  return { id, label, topicIds: [], href: startHrefs[id], role, status }
}
function step(id: DestinationId, label: string, diagramNote?: string) {
  return { id, kind: 'destination' as const, label, destinationId: id, ...(diagramNote ? { diagramNote } : {}) }
}

export const startJourney: ComponentJourney = {
  guideId: 'start',
  title: 'Overall guide map',
  intro: 'Save your current IDs, prepare, choose a part and check the result.',
  nodes: [
    { id: 'start', kind: 'start', label: 'Start here' },
    step('inspect', 'HWIDChecker', 'Read-only inspection'),
    step('nvram', 'EFI variables', 'Read-only'),
    step('stage-2', 'Save current IDs'),
    step('stage-3', 'Backup and recovery'),
    step('stage-4', 'Check device limits'),
    { id: 'component', kind: 'decision', label: 'Which part?' },
    step('motherboard', 'Motherboard'),
    step('tpm', 'TPM'),
    step('ftpm', 'fTPM'),
    step('storage', 'SSD / storage'),
    step('network', 'MAC address'),
    step('memory', 'RAM'),
    step('display', 'Monitor'),
    step('router', 'Router'),
    step('stage-8', 'Verification by part'),
    step('stage-10', 'Save after IDs'),
    step('stage-9', 'Reinstall Windows', 'Optional'),
  ],
  edges: [
    { from: 'start', to: 'inspect', label: 'View IDs' },
    { from: 'start', to: 'nvram', label: 'Inspect EFI' },
    { from: 'start', to: 'stage-2', label: 'Change IDs' },
    { from: 'stage-2', to: 'stage-3' },
    { from: 'stage-3', to: 'stage-4' },
    { from: 'stage-4', to: 'component' },
    ...['motherboard', 'tpm', 'ftpm', 'storage', 'network', 'memory', 'display', 'router'].flatMap((id) => [
      { from: 'component', to: id },
      { from: id, to: 'stage-8' },
    ]),
    { from: 'stage-8', to: 'component', label: 'Another part' },
    { from: 'stage-8', to: 'stage-10' },
    { from: 'stage-8', to: 'stage-9', label: 'If planned' },
    { from: 'stage-9', to: 'stage-10' },
  ],
  destinations: [
    destination('inspect', 'View IDs with HWIDChecker', 'identification', 'Collect a read-only baseline using the existing HWIDChecker guide.'),
    destination('nvram', 'Inspect EFI variables', 'identification', 'The EFI guide covers read-only inspection; it is separate from HWIDChecker.'),
    destination('stage-2', 'Save current IDs', 'preparation', 'Keep a private before snapshot, as described in the existing comparison guide.'),
    destination('stage-3', 'Backup and recovery', 'preparation', 'Use the shared checklist and the exact-device recovery instructions before writing.'),
    destination('stage-4', 'Check device limits', 'preparation', 'The device-restrictions section explains fixed identifiers and hardware-specific limits.'),
    ...(['motherboard', 'tpm', 'ftpm', 'storage', 'network', 'memory', 'display', 'router'] as const).map((id) => {
      const guide = guides.find((item) => item.id === id)!
      return destination(id, guide.shortTitle + ' guide', 'background', 'Open its chooser, then use the selected method and its preparation instructions.')
    }),
    destination('stage-8', 'Verification links by component', 'verification', 'Choose the relevant guide in the verification list. Reboot or remove power where that guide requires it, and compare before moving to another part.'),
    destination('stage-10', 'Save after IDs', 'verification', 'Export and compare the result using the same collection guide.'),
    destination('stage-9', 'Clean Windows installation', 'procedure', 'Use the existing reinstall checklist only when reinstalling is part of the planned work, after verifying the final hardware state.'),
  ],
  sections: [],
}

export function startDestinationHref(value: JourneyDestination): string {
  const href = startHrefs[value.id as DestinationId]
  if (!startJourney.destinations.includes(value) || !href || value.href !== href || value.topicIds.length) {
    throw new Error('Unknown Start overview destination: ' + value.id)
  }
  return href
}
