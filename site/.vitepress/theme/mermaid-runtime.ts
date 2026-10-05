import type { Mermaid, MermaidConfig } from 'mermaid'

export interface DiagramLink {
  nodeId: string
  destinationId: string
  href: string
  siteHref: string
  label: string
  nodeLabel: string
  status: string
}

export interface RenderedDiagram {
  svg: string
  width: number
  height: number
}

let mermaidPromise: Promise<Mermaid> | undefined
let renderQueue: Promise<unknown> = Promise.resolve()
let renderSequence = 0

function loadMermaid(): Promise<Mermaid> {
  if (!mermaidPromise) {
    mermaidPromise = import('mermaid').then((module) => module.default).catch((error) => {
      mermaidPromise = undefined
      throw error
    })
  }
  return mermaidPromise
}

function configuration(dark: boolean, wrappingWidth = 160, compact = false): MermaidConfig {
  const text = dark ? '#dfdfd6' : '#303038'
  const border = dark ? '#4ba8f5' : '#0067b8'
  return {
    startOnLoad: false,
    securityLevel: 'loose',
    suppressErrorRendering: true,
    look: 'classic',
    layout: 'dagre',
    htmlLabels: false,
    theme: 'base',
    fontFamily: 'Inter, "Segoe UI", Arial, sans-serif',
    themeVariables: {
      darkMode: dark,
      fontSize: '16px',
      background: dark ? '#1b1b1f' : '#ffffff',
      primaryColor: dark ? '#202d3b' : '#edf6ff',
      primaryTextColor: text,
      primaryBorderColor: border,
      secondaryColor: dark ? '#252529' : '#f6f6f7',
      tertiaryColor: dark ? '#252529' : '#f6f6f7',
      lineColor: dark ? '#a8a8b2' : '#67676f',
      textColor: text,
      edgeLabelBackground: dark ? '#1b1b1f' : '#ffffff',
      clusterBkg: dark ? '#252529' : '#f6f6f7',
      clusterBorder: dark ? '#67676f' : '#c2c2c4',
    },
    flowchart: {
      useMaxWidth: false,
      diagramPadding: 24,
      nodeSpacing: 24,
      rankSpacing: 52,
      curve: 'linear',
      wrappingWidth,
      // The Start overview has short linked boxes; retain natural label widths.
      ...(compact ? { minNodeWidth: 0, padding: 8, nodeSpacing: 12, rankSpacing: 32 } : {}),
    },
    themeCSS: `
      .node.journey-research rect, .node.journey-limitation rect { stroke-dasharray: 5 3; }
      .node.journey-background rect, .node.journey-preparation rect, .node.journey-verification rect {
        fill: ${dark ? '#252529' : '#f6f6f7'};
      }
      .edgeLabel text, .nodeLabel { font-size: 16px; }
    `,
  }
}

function prepareSvg(markup: string, id: string, links: DiagramLink[], title: string, description: string, dark: boolean): RenderedDiagram {
  const parsed = new DOMParser().parseFromString(markup, 'image/svg+xml')
  const svg = parsed.documentElement
  if (svg.localName !== 'svg' || parsed.querySelector('parsererror, script, foreignObject')) {
    throw new Error('The renderer did not return a plain SVG flowchart.')
  }
  for (const element of svg.querySelectorAll('*')) {
    if ([...element.attributes].some((attribute) => /^on/i.test(attribute.name))) {
      throw new Error('Unexpected interactive code in the flowchart.')
    }
  }
  const anchors = [...svg.querySelectorAll('a')]
  if (anchors.length !== links.length) throw new Error('Some flowchart destinations are missing links.')
  const consumed = new Set<string>()
  for (const anchor of anchors) {
    const link = links.find((candidate) => [...anchor.querySelectorAll('[id]')]
      .some((node) => node.id.startsWith(`${id}-flowchart-${candidate.nodeId}-`)))
    const href = anchor.getAttribute('href') || anchor.getAttributeNS('http://www.w3.org/1999/xlink', 'href')
    if (!link || href !== link.href || consumed.has(link.nodeId) || !link.siteHref.startsWith('/') || link.siteHref.startsWith('//')) {
      throw new Error('A flowchart link does not match an authored destination.')
    }
    consumed.add(link.nodeId)
    anchor.setAttribute('href', link.siteHref)
    anchor.setAttributeNS('http://www.w3.org/1999/xlink', 'xlink:href', link.siteHref)
    anchor.setAttribute('target', '_self')
    anchor.setAttribute('tabindex', '0')
    anchor.setAttribute('aria-label', `${link.label}. ${link.status}`)
    anchor.setAttribute('data-destination-id', link.destinationId)
    // Mermaid's plain text renderer keeps authored line breaks as SVG rows.
    // Style only an exact, bounded title match; leave uncertain rows unchanged.
    const rows = [...anchor.querySelectorAll<SVGTSpanElement>('text > tspan.row')]
    const expectedTitle = link.nodeLabel.replace(/\s+/g, '')
    let titleText = ''
    let titleRows = 0
    for (const row of rows) {
      titleText += (row.textContent || '').replace(/\s+/g, '')
      if (!expectedTitle.startsWith(titleText)) break
      ++titleRows
      if (titleText === expectedTitle) break
    }
    if (titleText === expectedTitle && titleRows > 0) {
      rows.forEach((row, index) => {
        const titleRow = index < titleRows
        for (const span of [row, ...row.querySelectorAll('tspan')]) {
          span.style.fontWeight = titleRow ? '600' : '400'
          span.style.fill = titleRow ? (dark ? '#dfdfd6' : '#303038') : (dark ? '#b3b3bd' : '#595960')
        }
      })
    }
  }
  const viewBox = (svg.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number)
  if (viewBox.length !== 4 || !viewBox.every(Number.isFinite) || viewBox[2] <= 0 || viewBox[3] <= 0) {
    throw new Error('The flowchart has no usable dimensions.')
  }
  for (const [tag, value, suffix] of [['title', title, 'title'], ['desc', description, 'description']]) {
    let node = [...svg.children].find((child) => child.localName === tag)
    if (!node) {
      node = parsed.createElementNS('http://www.w3.org/2000/svg', tag)
      svg.prepend(node)
    }
    node.id = `${id}-${suffix}`
    node.textContent = value
  }
  svg.setAttribute('role', 'graphics-document')
  svg.setAttribute('aria-roledescription', 'decision flowchart')
  svg.setAttribute('aria-labelledby', `${id}-title`)
  svg.setAttribute('aria-describedby', `${id}-description`)
  svg.setAttribute('width', '100%')
  svg.setAttribute('height', '100%')
  svg.setAttribute('style', 'display:block;max-width:none;width:100%;height:100%')
  return { svg: new XMLSerializer().serializeToString(svg), width: viewBox[2], height: viewBox[3] }
}

// Mermaid configuration is global. Keep initialize + render together so concurrent
// choosers and theme changes cannot render with one another's configuration.
export function renderDecisionDiagram(options: {
  source: string
  dark: boolean
  title: string
  description: string
  links: DiagramLink[]
  wrappingWidth?: number
  compact?: boolean
  isCurrent: () => boolean
}): Promise<RenderedDiagram | undefined> {
  const render = async (): Promise<RenderedDiagram | undefined> => {
    if (!options.isCurrent()) return
    const mermaid = await loadMermaid()
    if (!options.isCurrent()) return
    const id = `hwid-decision-${++renderSequence}`
    const staging = document.createElement('div')
    staging.className = 'decision-tree-render-staging'
    staging.setAttribute('aria-hidden', 'true')
    document.body.append(staging)
    try {
      mermaid.initialize(configuration(options.dark, options.wrappingWidth, options.compact))
      const result = await mermaid.render(id, options.source, staging)
      if (!options.isCurrent()) return
      // No bindFunctions call: the only interactions are real destination anchors.
      return prepareSvg(result.svg, id, options.links, options.title, options.description, options.dark)
    } finally {
      staging.remove()
    }
  }
  const result = renderQueue.then(render, render)
  renderQueue = result.catch(() => undefined)
  return result
}
