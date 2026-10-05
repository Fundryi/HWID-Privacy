<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useData, withBase } from 'vitepress'
import { destinationHref, getJourney, mermaidSource } from '../journey-data'
import { renderDecisionDiagram, type DiagramLink } from '../mermaid-runtime'
import '../decision-tree.css'

const props = defineProps<{ guideId: string }>()
const { isDark } = useData()
const journey = computed(() => getJourney(props.guideId))
const chartDestinations = computed(() => {
  const ids = new Set(journey.value?.nodes.filter((node) => node.kind === 'destination').map((node) => node.destinationId))
  return journey.value?.destinations.filter((destination) => ids.has(destination.id)) || []
})
const fallbackLinks = computed(() => chartDestinations.value.map((destination) => ({
  ...destination,
  href: withBase(destinationHref(journey.value!, destination)),
})))
const state = ref<'idle' | 'loading' | 'ready' | 'error'>('idle')
const svg = ref('')
const width = ref(0)
const height = ref(0)
const scale = ref(1)
const expanded = ref(false)
const shell = ref<HTMLElement>()
const viewport = ref<HTMLElement>()
const enlargeButton = ref<HTMLButtonElement>()
const closeButton = ref<HTMLButtonElement>()
const svgContainer = ref<HTMLElement>()
const highlightedLabel = ref<string | null>(null)
const hasChart = computed(() => state.value === 'ready' && !!svg.value)
const isOverview = computed(() => hasChart.value && scale.value < minimumReadableScale)
const diagramSize = computed(() => ({ width: `${width.value * scale.value}px`, height: `${height.value * scale.value}px` }))
const status = computed(() => {
  if (state.value === 'error') return 'The flowchart could not be displayed. Retry it or use the destination links below.'
  if (state.value === 'loading') return 'Rendering the flowchart. Destination links are available below.'
  if (state.value === 'idle') return 'The interactive flowchart loads here. Destination links are available below.'
  const hint = isOverview.value
    ? 'Overview: zoom in to read, or select Readable size. Destinations are clickable at every zoom.'
    : 'Follow the labeled answers, then open a linked destination. Scroll to see wider branches.'
  return highlightedLabel.value ? `${hint} Current return: ${highlightedLabel.value}.` : hint
})
// Leave a small margin above 14px for fractional SVG sizing and font metrics.
const minimumReadableScale = 14.1 / 16
const minimumScale = 0.05
const maximumScale = 3
let mounted = false
let renderVersion = 0
let autoFit: 'readable' | 'overview' | undefined = 'readable'
let resizeObserver: ResizeObserver | undefined
let returnFocus: HTMLElement | SVGElement | null = null
let previousOverflow: string | undefined
let background: { element: HTMLElement; inert: boolean }[] = []

function fit(readable = false) {
  autoFit = readable ? 'readable' : 'overview'
  if (!viewport.value || !width.value) return
  const widthScale = (viewport.value.clientWidth - 32) / width.value
  scale.value = readable
    ? Math.max(minimumReadableScale, Math.min(1, widthScale))
    : Math.max(minimumScale, Math.min(1, widthScale, (viewport.value.clientHeight - 32) / height.value))
  void nextTick(() => {
    if (!viewport.value) return
    viewport.value.scrollLeft = Math.max(0, (width.value * scale.value - viewport.value.clientWidth) / 2)
    viewport.value.scrollTop = 0
    highlightHash()
  })
}

function highlightHash() {
  highlightedLabel.value = null
  const nodes = [...(svgContainer.value?.querySelectorAll<SVGGElement>('.node') || [])]
  for (const node of nodes) node.classList.remove('decision-tree-node--target')
  let hash: string
  try {
    hash = decodeURIComponent(window.location.hash)
  } catch {
    return
  }
  const selected = journey.value?.nodes.find((node) => hash === `#node-${node.id}`)
  if (!selected || !journey.value || !viewport.value) return
  const renderId = svgContainer.value?.querySelector('svg')?.id
  const id = `${renderId}-flowchart-j_${journey.value.guideId.replace(/-/g, '_')}_${selected.id.replace(/-/g, '_')}-`
  const target = nodes.find((node) => node.id.startsWith(id))
  if (!target) return
  target.classList.add('decision-tree-node--target')
  highlightedLabel.value = selected.label
  const bounds = target.getBoundingClientRect()
  const frame = viewport.value.getBoundingClientRect()
  viewport.value.scrollLeft += bounds.left + bounds.width / 2 - frame.left - frame.width / 2
  viewport.value.scrollTop += bounds.top + bounds.height / 2 - frame.top - frame.height / 2
}

function hashChanged() {
  void nextTick(highlightHash)
}

function zoom(multiplier: number) {
  if (!viewport.value || !hasChart.value) return
  autoFit = undefined
  const oldScale = scale.value
  const centerX = viewport.value.scrollLeft + viewport.value.clientWidth / 2
  const centerY = viewport.value.scrollTop + viewport.value.clientHeight / 2
  scale.value = Math.max(minimumScale, Math.min(maximumScale, oldScale * multiplier))
  void nextTick(() => {
    if (!viewport.value) return
    viewport.value.scrollLeft = centerX * scale.value / oldScale - viewport.value.clientWidth / 2
    viewport.value.scrollTop = centerY * scale.value / oldScale - viewport.value.clientHeight / 2
  })
}

function releaseBackground() {
  for (const item of background) item.element.inert = item.inert
  background = []
  if (previousOverflow !== undefined) document.body.style.overflow = previousOverflow
  previousOverflow = undefined
}

async function enlarge() {
  if (!hasChart.value || expanded.value) return
  returnFocus = document.activeElement as HTMLElement | SVGElement | null
  expanded.value = true
  await nextTick()
  if (!mounted || !expanded.value || !shell.value) return
  previousOverflow = document.body.style.overflow
  document.body.style.overflow = 'hidden'
  for (const child of document.body.children) {
    if (child instanceof HTMLElement && !child.contains(shell.value) && !child.classList.contains('decision-tree__backdrop')) {
      background.push({ element: child, inert: child.inert })
      child.inert = true
    }
  }
  fit()
  closeButton.value?.focus()
}

function close() {
  if (!expanded.value) return
  expanded.value = false
  releaseBackground()
  const focus = returnFocus
  returnFocus = null
  void nextTick(() => {
    if (!mounted) return
    fit(true)
    if (focus?.isConnected && 'focus' in focus) focus.focus()
    else enlargeButton.value?.focus()
  })
}

function modalKeys(event: KeyboardEvent) {
  if (!expanded.value || !shell.value) return
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    close()
    return
  }
  if (event.key !== 'Tab') return
  const controls = [...shell.value.querySelectorAll<HTMLElement | SVGElement>(
    'button:not([disabled]), a[href], summary, [tabindex]:not([tabindex="-1"])',
  )].filter((element) => element.getClientRects().length > 0
    && (!element.closest('details:not([open])') || element.localName === 'summary'))
  const first = controls[0]
  const last = controls[controls.length - 1]
  if (!first || !last) {
    event.preventDefault()
    shell.value.focus()
  } else if (!shell.value.contains(document.activeElement) || (event.shiftKey && document.activeElement === first)) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}

async function refresh() {
  const version = ++renderVersion
  svg.value = ''
  highlightedLabel.value = null
  width.value = 0
  height.value = 0
  state.value = 'loading'
  const current = journey.value
  if (!current) {
    state.value = 'error'
    return
  }
  const isCurrent = () => mounted && version === renderVersion
  try {
    const links: DiagramLink[] = current.nodes.filter((node) => node.kind === 'destination').map((node) => {
      const destination = current.destinations.find((candidate) => candidate.id === node.destinationId)
      if (!destination) throw new Error(`Missing destination: ${node.id}`)
      const href = destinationHref(current, destination)
      return {
        nodeId: `j_${current.guideId.replace(/-/g, '_')}_${node.id.replace(/-/g, '_')}`,
        destinationId: destination.id,
        href,
        siteHref: withBase(href),
        label: destination.label,
        nodeLabel: node.label,
        status: destination.status,
      }
    })
    const rendered = await renderDecisionDiagram({
      source: mermaidSource(current, 'site'),
      dark: isDark.value,
      title: current.title,
      description: current.intro,
      links,
      wrappingWidth: current.guideId === 'memory' || current.guideId === 'router' ? 220 : 160,
      compact: current.guideId === 'start',
      isCurrent,
    })
    if (!rendered || !isCurrent()) return
    svg.value = rendered.svg
    width.value = rendered.width
    height.value = rendered.height
    state.value = 'ready'
    await nextTick()
    if (isCurrent()) {
      if (autoFit) fit(autoFit === 'readable')
      else highlightHash()
    }
  } catch (error) {
    if (isCurrent()) {
      console.error('Decision chart could not be rendered', error)
      state.value = 'error'
    }
  }
}

function resized() {
  if (autoFit && hasChart.value) fit(autoFit === 'readable')
}

watch([() => props.guideId, () => isDark.value], ([id], previous) => {
  if (!mounted) return
  if (id !== previous[0]) {
    close()
    autoFit = 'readable'
  }
  void refresh()
})

onMounted(() => {
  mounted = true
  document.addEventListener('keydown', modalKeys, true)
  window.addEventListener('hashchange', hashChanged)
  window.addEventListener('popstate', hashChanged)
  if (typeof ResizeObserver !== 'undefined' && viewport.value) {
    resizeObserver = new ResizeObserver(resized)
    resizeObserver.observe(viewport.value)
  } else {
    window.addEventListener('resize', resized)
  }
  void refresh()
})

onBeforeUnmount(() => {
  mounted = false
  ++renderVersion
  resizeObserver?.disconnect()
  window.removeEventListener('resize', resized)
  window.removeEventListener('hashchange', hashChanged)
  window.removeEventListener('popstate', hashChanged)
  document.removeEventListener('keydown', modalKeys, true)
  releaseBackground()
})
</script>

<template>
  <section id="decision-tree" class="decision-tree" :data-guide-id="guideId" :aria-label="journey?.title || 'Hardware route chooser'">
    <span v-for="node in journey?.nodes || []" :id="`node-${node.id}`" :key="node.id" class="decision-tree__return-anchor" aria-hidden="true" />
    <Teleport to="body" :disabled="!expanded">
      <div v-if="expanded" class="decision-tree__backdrop" aria-hidden="true" @click="close" />
      <div
        ref="shell"
        class="decision-tree__shell"
        :class="{ 'decision-tree__shell--expanded': expanded }"
        :role="expanded ? 'dialog' : undefined"
        :aria-modal="expanded ? true : undefined"
        :aria-label="expanded ? `${journey?.title || 'Hardware route'} enlarged flowchart` : undefined"
        tabindex="-1"
      >
        <div class="decision-tree__toolbar" aria-label="Flowchart controls">
          <span class="decision-tree__heading">{{ journey?.title || 'Hardware route chooser' }}</span>
          <div class="decision-tree__controls">
            <button type="button" :disabled="!hasChart || scale <= minimumScale" aria-label="Zoom out" @click="zoom(1 / 1.25)">−</button>
            <output class="decision-tree__scale" aria-label="Flowchart zoom">{{ Math.round(scale * 100) }}%</output>
            <button type="button" :disabled="!hasChart || scale >= maximumScale" aria-label="Zoom in" @click="zoom(1.25)">+</button>
            <button type="button" :disabled="!hasChart" title="Show the complete chart overview" @click="fit(false)">Fit</button>
            <button type="button" :disabled="!hasChart" @click="fit(true)">Readable size</button>
            <button v-if="!expanded" ref="enlargeButton" type="button" :disabled="!hasChart" @click="enlarge">Enlarge</button>
            <button v-else ref="closeButton" type="button" aria-label="Close enlarged flowchart" @click="close">Close <kbd>Esc</kbd></button>
          </div>
        </div>
        <p class="decision-tree__status" :class="{ 'decision-tree__status--error': state === 'error' }" role="status" aria-live="polite">
          {{ status }}
          <button v-if="state === 'error'" type="button" @click="refresh">Retry chart</button>
        </p>
        <p class="decision-tree__legend">Dashed outline: research or limitation. Amber outline: returned node.</p>
        <div ref="viewport" class="decision-tree__viewport" :class="{ 'decision-tree__viewport--empty': !hasChart }" :aria-busy="state === 'loading'" tabindex="0" aria-label="Flowchart scroll area">
          <div v-if="hasChart" ref="svgContainer" class="decision-tree__svg" :style="diagramSize" v-html="svg" />
          <div v-else class="decision-tree__placeholder" aria-hidden="true">{{ state === 'error' ? 'Chart unavailable' : 'Loading flowchart…' }}</div>
        </div>
        <details class="decision-tree__fallback" :open="!hasChart">
          <summary>Destination links {{ hasChart ? '(text alternative)' : '(available now)' }}</summary>
          <ul v-if="fallbackLinks.length">
            <li v-for="destination in fallbackLinks" :key="destination.id">
              <a :href="destination.href">{{ destination.label }}</a>
              <span>{{ destination.status }}</span>
            </li>
          </ul>
          <p v-else>No route is available for this page. Open the <a :href="withBase('/devices.html')">hardware topic index</a>.</p>
        </details>
        <noscript><p class="decision-tree__no-script">JavaScript is off. Use the destination links above to open the full guide sections.</p></noscript>
      </div>
    </Teleport>
  </section>
</template>
