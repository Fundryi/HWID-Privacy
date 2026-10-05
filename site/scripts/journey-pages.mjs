import { readFile, access } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { build } from 'esbuild'

const defaultRoot = fileURLToPath(new URL('../..', import.meta.url))
const metadataOwner = 'site/.vitepress/theme/journey-data.ts'
const startBegin = '<!-- start-overview:begin -->'
const startEnd = '<!-- start-overview:end -->'
const website = 'https://hwid.idkzal.cc'
const roles = new Set(['procedure', 'identification', 'background', 'preparation', 'verification', 'research', 'limitation'])

export async function loadJourneyData(root = defaultRoot) {
  const result = await build({
    stdin: {
      contents: "export * from './journey-data'; export { guides } from './guide-data'; export { startJourney, startDestinationHref } from './start-journey';",
      resolveDir: path.join(root, 'site/.vitepress/theme'),
      sourcefile: 'journey-pages-entry.ts',
      loader: 'ts',
    },
    bundle: true,
    platform: 'node',
    format: 'esm',
    write: false,
    logLevel: 'silent',
  })
  return import(`data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`)
}

function sourcePath(href) {
  if (typeof href !== 'string' || !href.startsWith('/') || href.startsWith('//') || /[\\\s\x00-\x1f?"'<>`]/.test(href)) {
    throw new Error(`Destination must be an internal route: ${href}`)
  }
  const [route, ...fragment] = href.split('#')
  if (fragment.length > 1) throw new Error(`Invalid destination fragment: ${href}`)
  const decoded = decodeURIComponent(route)
  if (decoded !== '/' && decoded.slice(1).split('/').some((part) => !part || part === '.' || part === '..')) {
    throw new Error(`Invalid destination path: ${href}`)
  }
  let file
  if (['/', '/index.html', '/home', '/home.html'].includes(decoded)) file = 'site/home.md'
  else if (/^\/(start|devices|reference)(?:\.html)?$/.test(decoded)) file = `site/${decoded.slice(1).replace(/\.html$/, '')}.md`
  else if (/^\/hardware\/[a-z][a-z0-9-]*(?:\.html)?$/.test(decoded)) file = `site${decoded.replace(/\.html$/, '')}.md`
  else if (decoded.startsWith('/guides/') && decoded.endsWith('.html')) file = decoded.slice(1).replace(/\.html$/, '.md')
  else throw new Error(`No Markdown source registered for ${href}`)
  return { file, fragment: fragment.length ? `#${fragment[0]}` : '' }
}

export function githubHref(href, fromPath) {
  const source = sourcePath(href)
  return path.posix.relative(path.posix.dirname(fromPath), source.file) + source.fragment
}

function plainText(value, context) {
  if (typeof value !== 'string' || !value.trim() || /[\x00-\x1f]/.test(value)) throw new Error(`Missing or multiline text: ${context}`)
}

function uniqueMap(items, context) {
  const result = new Map()
  for (const item of items) {
    if (!/^[a-z][a-z0-9-]*$/.test(item.id) || result.has(item.id)) throw new Error(`Invalid or repeated ID: ${context}/${item.id}`)
    result.set(item.id, item)
  }
  return result
}

export async function validateJourneys(data, root = defaultRoot) {
  const { guides, journeys, destinationHref } = data
  const guideMap = uniqueMap(guides, 'guides')
  const topicMap = uniqueMap(guides.flatMap((guide) => guide.methods), 'topics')
  const journeyIds = new Set()
  const coveredTopics = new Set()
  const sectionTopics = new Set()
  const sourceFiles = new Set()
  const stats = { journeys: 0, topics: topicMap.size, nodes: 0, edges: 0, decisions: 0, destinations: 0, chartDestinations: 0 }
  const validateHref = (href) => sourceFiles.add(sourcePath(href).file)

  for (const journey of journeys) {
    const id = journey.guideId
    if (!guideMap.has(id) || journeyIds.has(id)) throw new Error(`Unknown or repeated journey: ${id}`)
    journeyIds.add(id)
    plainText(journey.title, `${id}/title`)
    plainText(journey.intro, `${id}/intro`)
    const nodes = uniqueMap(journey.nodes, `${id}/nodes`)
    const destinations = uniqueMap(journey.destinations, `${id}/destinations`)
    const outgoing = new Map([...nodes.keys()].map((nodeId) => [nodeId, []]))
    const starts = journey.nodes.filter((node) => node.kind === 'start')
    if (starts.length !== 1) throw new Error(`Journey must have one start: ${id}`)

    for (const destination of journey.destinations) {
      plainText(destination.label, `${id}/${destination.id}/label`)
      plainText(destination.status, `${id}/${destination.id}/status`)
      if (destination.note) plainText(destination.note, `${id}/${destination.id}/note`)
      if (!roles.has(destination.role)) throw new Error(`Unknown destination role: ${id}/${destination.id}`)
      const topicHrefs = destination.topicIds.map((topicId) => {
        if (!topicMap.has(topicId)) throw new Error(`Unknown topic: ${id}/${topicId}`)
        if (coveredTopics.has(topicId)) throw new Error(`Repeated topic destination: ${id}/${topicId}`)
        coveredTopics.add(topicId)
        return topicMap.get(topicId).href
      })
      const href = destinationHref(journey, destination)
      if (topicHrefs.some((topicHref) => topicHref !== href) || (destination.href && destination.href !== href)) {
        throw new Error(`Alias href mismatch: ${id}/${destination.id}`)
      }
      validateHref(href)
    }

    for (const node of journey.nodes) {
      plainText(node.label, `${id}/${node.id}/label`)
      if (!['start', 'decision', 'destination'].includes(node.kind)) throw new Error(`Unknown node kind: ${id}/${node.id}`)
      if (node.kind === 'destination' ? !destinations.has(node.destinationId) : node.destinationId !== undefined) {
        throw new Error(`Invalid node destination: ${id}/${node.id}`)
      }
    }
    const seenEdges = new Set()
    for (const edge of journey.edges) {
      if (!nodes.has(edge.from) || !nodes.has(edge.to)) throw new Error(`Dangling edge: ${id}/${edge.from} -> ${edge.to}`)
      const key = JSON.stringify([edge.from, edge.to, edge.label])
      if (seenEdges.has(key)) throw new Error(`Repeated edge: ${id}/${edge.from} -> ${edge.to}`)
      seenEdges.add(key)
      outgoing.get(edge.from).push(edge)
      if (edge.label !== undefined) plainText(edge.label, `${id}/${edge.from}/answer`)
    }
    for (const node of journey.nodes.filter((candidate) => candidate.kind === 'decision')) {
      const answers = outgoing.get(node.id)
      // A device/tool box can supply the answer without repeating it on its arrow.
      const labels = answers.map((edge) => edge.label ?? nodes.get(edge.to).label)
      if (answers.length < 2 || labels.some((label) => !label.trim())) throw new Error(`Decision needs named alternatives: ${id}/${node.id}`)
      if (new Set(labels).size !== answers.length) throw new Error(`Repeated decision answer: ${id}/${node.id}`)
    }
    const reachable = new Set([starts[0].id])
    for (const nodeId of reachable) for (const edge of outgoing.get(nodeId)) reachable.add(edge.to)
    const unreachable = [...nodes.keys()].filter((nodeId) => !reachable.has(nodeId))
    if (unreachable.length) throw new Error(`Unreachable nodes: ${id}/${unreachable.join(', ')}`)

    const sectionDestinations = new Set()
    for (const section of journey.sections) {
      plainText(section.label, `${id}/section`)
      for (const destinationId of section.destinationIds) {
        if (!destinations.has(destinationId) || sectionDestinations.has(destinationId)) throw new Error(`Unknown or repeated section destination: ${id}/${destinationId}`)
        sectionDestinations.add(destinationId)
        for (const topicId of destinations.get(destinationId).topicIds) sectionTopics.add(topicId)
      }
    }
    const omitted = [...destinations.keys()].filter((destinationId) => !sectionDestinations.has(destinationId))
    if (omitted.length) throw new Error(`Destination missing from supporting sections: ${id}/${omitted.join(', ')}`)
    const guide = guideMap.get(id)
    for (const href of [guide.href, guide.identify.href, guide.prepare.href, guide.verify.href, data.hubHref(id)]) validateHref(href)
    if (data.getJourney(id) !== journey) throw new Error(`Journey lookup mismatch: ${id}`)
    stats.nodes += journey.nodes.length
    stats.edges += journey.edges.length
    stats.decisions += journey.nodes.filter((node) => node.kind === 'decision').length
    stats.destinations += journey.destinations.length
    stats.chartDestinations += new Set(journey.nodes.filter((node) => node.kind === 'destination').map((node) => node.destinationId)).size
  }
  const missingGuides = [...guideMap.keys()].filter((id) => !journeyIds.has(id))
  if (missingGuides.length) throw new Error(`Missing journeys: ${missingGuides.join(', ')}`)
  const missingTopics = [...topicMap.keys()].filter((id) => !coveredTopics.has(id) || !sectionTopics.has(id))
  if (missingTopics.length) throw new Error(`Topics missing from destinations or supporting sections: ${missingTopics.join(', ')}`)
  const generatedFiles = new Set(journeys.map((journey) => `site/hardware/${journey.guideId}.md`))
  for (const file of sourceFiles) {
    if (!generatedFiles.has(file)) {
      try { await access(path.join(root, file)) }
      catch { throw new Error(`Markdown destination does not exist: ${file}`) }
    }
  }
  return { ...stats, journeys: journeys.length }
}

export async function validateStartJourney(data, root = defaultRoot) {
  const journey = data.startJourney
  if (journey.guideId !== 'start' || data.getJourney('start') !== journey) throw new Error('Invalid Start journey lookup')
  plainText(journey.title, 'start/title')
  plainText(journey.intro, 'start/intro')
  const nodes = uniqueMap(journey.nodes, 'start/nodes')
  const destinations = uniqueMap(journey.destinations, 'start/destinations')
  const starts = journey.nodes.filter((node) => node.kind === 'start')
  if (starts.length !== 1) throw new Error('Start journey must have one start')
  const outgoing = new Map([...nodes.keys()].map((id) => [id, []]))
  const sourceFiles = new Set()
  for (const destination of journey.destinations) {
    plainText(destination.label, `start/${destination.id}/label`)
    plainText(destination.status, `start/${destination.id}/status`)
    if (destination.note) plainText(destination.note, `start/${destination.id}/note`)
    if (!roles.has(destination.role) || destination.topicIds.length) throw new Error(`Invalid Start destination: ${destination.id}`)
    const href = data.startDestinationHref(destination)
    if (destination.href !== href || data.destinationHref(journey, destination) !== href) throw new Error(`Start destination href mismatch: ${destination.id}`)
    const file = sourcePath(href).file
    if (file === 'site/hardware/start.md') throw new Error('Start is not a hardware hub')
    sourceFiles.add(file)
  }
  for (const node of journey.nodes) {
    plainText(node.label, `start/${node.id}/label`)
    if (!['start', 'decision', 'destination'].includes(node.kind)) throw new Error(`Unknown Start node kind: ${node.id}`)
    if (node.kind === 'destination' ? !destinations.has(node.destinationId) : node.destinationId !== undefined) throw new Error(`Invalid Start node destination: ${node.id}`)
  }
  const seenEdges = new Set()
  for (const edge of journey.edges) {
    if (!nodes.has(edge.from) || !nodes.has(edge.to)) throw new Error(`Dangling Start edge: ${edge.from} -> ${edge.to}`)
    const key = JSON.stringify([edge.from, edge.to, edge.label])
    if (seenEdges.has(key)) throw new Error(`Repeated Start edge: ${edge.from} -> ${edge.to}`)
    seenEdges.add(key)
    outgoing.get(edge.from).push(edge)
    if (edge.label !== undefined) plainText(edge.label, `start/${edge.from}/answer`)
  }
  const decisions = journey.nodes.filter((node) => node.kind === 'decision')
  for (const node of decisions) {
    const answers = outgoing.get(node.id)
    const labels = answers.map((edge) => edge.label ?? nodes.get(edge.to).label)
    if (answers.length < 2 || labels.some((label) => !label.trim())) throw new Error(`Start decision needs named alternatives: ${node.id}`)
    if (new Set(labels).size !== answers.length) throw new Error(`Repeated Start decision answer: ${node.id}`)
  }
  // Destination nodes may continue the work order, including the authored repeat loop.
  const reachable = new Set([starts[0].id])
  for (const id of reachable) for (const edge of outgoing.get(id)) reachable.add(edge.to)
  const unreachable = [...nodes.keys()].filter((id) => !reachable.has(id))
  if (unreachable.length) throw new Error(`Unreachable Start nodes: ${unreachable.join(', ')}`)
  for (const file of sourceFiles) {
    try { await access(path.join(root, file)) }
    catch { throw new Error(`Start Markdown destination does not exist: ${file}`) }
  }
  return { nodes: nodes.size, edges: journey.edges.length, decisions: decisions.length, destinations: destinations.size, linkedNodes: journey.nodes.filter((node) => node.kind === 'destination').length }
}

export function replaceStartOverview(text, source) {
  if (text.split(startBegin).length !== 2 || text.split(startEnd).length !== 2) throw new Error('site/start.md requires exactly one Start overview marker pair')
  const begin = text.indexOf(startBegin) + startBegin.length
  const end = text.indexOf(startEnd)
  if (end < begin) throw new Error('site/start.md Start overview markers are out of order')
  const newline = text.slice(begin, begin + 2) === '\r\n' ? '\r\n' : '\n'
  const snippet = ['', '```mermaid', '%% hwid-journey:start', source.replace(/\r\n/g, '\n'), '```', ''].join('\n').replace(/\n/g, newline)
  return text.slice(0, begin) + snippet + text.slice(end)
}

function markdown(value) {
  return value.replace(/[\\`*_[\]<>]/g, '\\$&')
}

function link(label, href, file) {
  const ordinary = `[${markdown(label)}](${githubHref(href, file)})`
  // Preserve authored Unicode fragments; GitHub's slug behavior is a separate renderer.
  return /[^\x00-\x7f]/.test(href.split('#')[1] ?? '') ? `${ordinary} ([website section](${website}${href}))` : ordinary
}

function renderPage(journey, data) {
  const guide = data.guides.find((candidate) => candidate.id === journey.guideId)
  const file = `site/hardware/${journey.guideId}.md`
  const topicMap = new Map(data.guides.flatMap((owner) => owner.methods.map((method) => [method.id, method])))
  const destinations = new Map(journey.destinations.map((destination) => [destination.id, destination]))
  const title = `${guide.shortTitle} guide chooser`
  const lines = [
    '---',
    `title: ${JSON.stringify(title)}`,
    `description: ${JSON.stringify(journey.intro)}`,
    'aside: false',
    `journey: ${journey.guideId}`,
    '---',
    '',
    `<!-- Generated from ${metadataOwner} by site/scripts/journey-pages.mjs. Edit the metadata owner; review --json output and apply it explicitly. -->`,
    '',
    `# ${markdown(title)}`,
    '',
    markdown(journey.intro),
    '',
    `- **Identify:** ${link(guide.identify.label, guide.identify.href, file)}`,
    `- **Prepare:** ${link(guide.prepare.label, guide.prepare.href, file)}`,
    `- **Full guide:** ${link(guide.title, guide.href, file)}`,
    `- **Verify:** ${link(guide.verify.label, guide.verify.href, file)}`,
    ...journey.destinations
      .filter((destination) => journey.guideId === 'router' && destination.role === 'preparation' && !destination.topicIds.length)
      .map((destination) => `- **Before choosing:** ${link(destination.label, data.destinationHref(journey, destination), file)}`),
    '',
    '## Choose your route',
    '',
    '```mermaid',
    `%% hwid-journey:${journey.guideId}`,
    data.mermaidSource(journey, 'github'),
    '```',
  ]
  for (const section of journey.sections) {
    lines.push('', `## ${markdown(section.label)}`, '')
    for (const destinationId of section.destinationIds) {
      const destination = destinations.get(destinationId)
      const href = data.destinationHref(journey, destination)
      lines.push(`- ${link(destination.label, href, file)} (${destination.role}). **${markdown(destination.status)}**${destination.note ? ` ${markdown(destination.note)}` : ''}`)
      const aliases = destination.topicIds.map((topicId) => topicMap.get(topicId).label)
      if (aliases.length > 1 || (aliases.length === 1 && aliases[0] !== destination.label)) {
        lines.push(`  Topic names: ${aliases.map(markdown).join('; ')}.`)
      }
    }
  }
  lines.push('', `${link('Home', '/', file)} · ${link('Complete work order', '/start.html', file)} · ${link('All hardware topics', '/devices.html', file)} · ${link('Reference', '/reference.html', file)}`, '')
  return lines.join('\n')
}

export async function generatePages(root = defaultRoot) {
  const data = await loadJourneyData(root)
  await validateJourneys(data, root)
  await validateStartJourney(data, root)
  const pages = Object.fromEntries(data.journeys.map((journey) => [`site/hardware/${journey.guideId}.md`, renderPage(journey, data)]))
  const start = await readFile(path.join(root, 'site/start.md'), 'utf8')
  pages['site/start.md'] = replaceStartOverview(start, data.mermaidSource(data.startJourney, 'github'))
  return pages
}

export function comparePages(expected, actual) {
  // Git's native Windows checkout may use CRLF; all other characters stay exact.
  return Object.keys(expected).sort().flatMap((file) => {
    const current = actual[file]?.replace(/\r\n/g, '\n')
    return current === expected[file].replace(/\r\n/g, '\n')
      ? []
      : [{ path: file, reason: actual[file] === undefined ? 'missing' : 'changed' }]
  })
}

async function main() {
  const args = process.argv.slice(2)
  if (args.length > 1 || (args.length && !['--check', '--json'].includes(args[0]))) {
    console.error('Usage: node site/scripts/journey-pages.mjs [--check | --json]')
    process.exitCode = 2
    return
  }
  const expected = await generatePages()
  if (args[0] === '--json') {
    process.stdout.write(`${JSON.stringify(expected, null, 2)}\n`)
    return
  }
  const actual = Object.fromEntries(await Promise.all(Object.keys(expected).map(async (file) => {
    try { return [file, await readFile(path.join(defaultRoot, file), 'utf8')] }
    catch (error) { if (error.code === 'ENOENT') return [file, undefined]; throw error }
  })))
  const drift = comparePages(expected, actual)
  if (drift.length) {
    console.error(`Journey page drift:\n${drift.map((entry) => `${entry.reason}: ${entry.path}`).join('\n')}`)
    process.exitCode = 1
  } else console.log(`Journey pages match metadata (${Object.keys(expected).length} pages).`)
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    console.error(`journey-pages: ${error.message}`)
    process.exitCode = 1
  })
}
