import { defineConfig } from 'vitepress'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { guides, workflowSteps } from './theme/guide-data'
import { iconForHref, iconMarkup, type IconName } from './theme/icons'
import { guideVisuals } from './theme/guide-visuals'
import { journeys, destinationHref, hubHref, getJourney } from './theme/journey-data'
import { startJourney } from './theme/start-journey'
import { siteSidebars, topNavigation, allNavigationLinks, type NavigationItem as SidebarItem } from './navigation'

const repo = 'https://github.com/Fundryi/HWID-Privacy'
const site = 'https://hwid.idkzal.cc'
const repoRoot = fileURLToPath(new URL('../../', import.meta.url))
const gettingStarted = '/guides/getting-started/getting-started'
// New mirrored downloads are served from the built site as well as held in the
// repository, so the local preview works before the first publication.
const localDownloads = ['guides/monitor-spoofing/tools/monitorinfoview/monitorinfoview.zip']
// srcDir is the repo root, so there is no public/ folder; the logo travels as a data URI
const logo = `data:image/svg+xml;base64,${fs.readFileSync(new URL('./theme/logo.svg', import.meta.url)).toString('base64')}`

// Documentation hierarchy; workflow order remains owned by workflowSteps.
const sidebar = siteSidebars['/']

function sidebarLinks(items: SidebarItem[]): { text: string; link: string }[] {
  return items.flatMap((item) => [
    ...(item.link ? [{ text: item.text, link: item.link }] : []),
    ...sidebarLinks(item.items ?? []),
  ])
}

function illustratedSidebar(items: SidebarItem[]): SidebarItem[] {
  return items.map((item) => {
    const icon = item.link?.startsWith('/hardware/') ? iconForHref(item.link) : undefined
    const label = item.contextOnly ? `<span class="wiki-sidebar-context">${item.text}</span>` : item.text
    return {
      ...item,
      text: icon ? `<span class="wiki-sidebar-label">${iconMarkup(icon)}<span>${label}</span></span>` : label,
      ...(item.items ? { items: illustratedSidebar(item.items) } : {}),
    }
  })
}

function headingIcon(source: string, id: string, level: string): IconName | undefined {
  const route = `/${source.replace(/\.md$/, '.html')}`
  if (level === 'h1') {
    if (source === 'README.md' || source === 'site/devices.md') return 'overview'
    if (source === 'site/start.md') return 'workflow'
    return iconForHref(route)
  }
  if (level !== 'h2' && level !== 'h3') return undefined
  if (id === 'sources') return 'sources'
  const guide = guides.find((item) => item.href === route)
  if (guide) {
    const destinations = [
      [guide.troubleshoot?.href, 'troubleshoot'], [guide.verify.href, 'verify'],
      [guide.prepare.href, 'prepare'], [guide.identify.href, 'inspect'],
      ...guide.methods.map((method) => [method.href, 'method']),
    ] as [string | undefined, IconName][]
    const match = destinations.find(([href]) => href?.split('#')[0] === route && decodeURIComponent(href.split('#')[1] ?? '') === id)
    if (match) return match[1]
  }
  const common: Record<string, IconName> = {
    'safety-checklist': 'prepare', 'hwidcheckerexe': 'inspect',
    'take-before-and-after-snapshots': 'inspect', 'plan-the-work-in-the-right-order': 'workflow',
    'work-order': 'workflow', 'tools': 'prepare', 'recovery': 'recovery',
    'requirements': 'prepare', 'troubleshooting': 'troubleshoot',
    'verify': 'verify',
  }
  return common[id]
}

// Validate the navigation against the generated pages, including the site's
// custom Unicode heading IDs. A stale route must fail before publication.
function validateGuideRoutes(outDir: string) {
  const ids = new Set<string>()
  const links = guides.flatMap((guide) => {
    if (ids.has(guide.id)) throw new Error(`Duplicate guide route: ${guide.id}`)
    ids.add(guide.id)
    return [guide.href, `/devices.html#${guide.id}`, guide.identify.href,
      guide.prepare.href, guide.verify.href, ...(guide.troubleshoot ? [guide.troubleshoot.href] : []),
      ...guide.methods.map((method) => method.href)]
  })
  links.push(...workflowSteps.flatMap((step) => [`/start.html#${step.id}`, ...step.links.map((link) => link.href)]))
  links.push(...Object.entries(guideVisuals).flatMap(([source, visual]) => [
    `/${source.replace(/\.md$/, '.html')}#visual-${visual.id}`,
    ...visual.sources.map((link) => link.href),
  ]))
  links.push(...journeys.flatMap((journey) => [
    hubHref(journey.guideId),
    `${hubHref(journey.guideId)}#decision-tree`,
    ...journey.nodes.map((node) => `${hubHref(journey.guideId)}#node-${node.id}`),
    ...journey.destinations.map((destination) => destinationHref(journey, destination)),
  ]))
  links.push(...allNavigationLinks.filter((link) => link.startsWith('/')))
  links.push('/start.html#decision-tree',
    ...startJourney.nodes.map((node) => `/start.html#node-${node.id}`),
    ...startJourney.destinations.map((destination) => destinationHref(startJourney, destination)))
  const pageIds = new Map<string, Map<string, number>>()
  for (const href of new Set(links)) {
    if (!href.startsWith('/')) throw new Error(`Guide navigation must use a local URL: ${href}`)
    const [route, fragment] = href.split('#')
    const relative = route === '/' ? 'index.html' : `${route.slice(1).replace(/\.html$/, '')}.html`
    const file = path.resolve(outDir, relative)
    if (!file.startsWith(`${path.resolve(outDir)}${path.sep}`) || !fs.existsSync(file))
      throw new Error(`Guide navigation page not found: ${href}`)
    if (!fragment) continue
    if (!pageIds.has(file)) {
      const html = fs.readFileSync(file, 'utf8')
      const counts = new Map<string, number>()
      for (const match of html.matchAll(/\bid="([^"]+)"/g)) counts.set(match[1], (counts.get(match[1]) ?? 0) + 1)
      pageIds.set(file, counts)
    }
    const count = pageIds.get(file)!.get(decodeURIComponent(fragment)) ?? 0
    if (count !== 1)
      throw new Error(`Guide navigation heading must occur exactly once (${count} found): ${href}`)
  }
}

// [C] [A] [CC] [S] evidence grades used across the guides
const grades: Record<string, [type: string, title: string]> = {
  C: ['tip', 'Confirmed: tested first hand'],
  A: ['info', 'Authoritative: documented by vendor or spec'],
  CC: ['warning', 'Community consensus'],
  S: ['danger', 'Single report, untested'],
}

// Guides stay in the repo root so they still read fine on GitHub.
// The site reads them from there; nothing is copied.
export default defineConfig({
  title: 'HWID Privacy',
  description: 'Hardware ID guides for Windows PCs: which parts have a fixed ID, and how to change them.',
  base: '/', // served from the custom domain set in Settings > Pages
  srcDir: '..',
  srcExclude: [
    'app/**', 'docs/**', 'TMP/**', 'AGENTS.md', 'AI_TOOLS.md', 'CLAUDE*.md',
    'guides/**/tools/**',
    'site/node_modules/**', 'site/.vitepress/**', 'site/package*.json',
  ],
  // site/home.md is the site-only landing page; the GitHub README becomes the overview
  rewrites: {
    'site/home.md': 'index.md', 'README.md': 'overview.md',
    'site/start.md': 'start.md', 'site/devices.md': 'devices.md',
    'site/reference.md': 'reference.md',
    ...Object.fromEntries(journeys.map((journey) => [`site/hardware/${journey.guideId}.md`, `hardware/${journey.guideId}.md`])),
  },
  appearance: 'dark', // matches the app
  lastUpdated: true,
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: logo }],
    ['meta', { property: 'og:site_name', content: 'HWID Privacy' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:image', content: `${site}/og.png` }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
  ],
  sitemap: { hostname: site },
  // Link previews (Discord, X, GitHub) show the page title
  transformHead({ pageData, title, description }) {
    const url = `${site}/${pageData.relativePath.replace(/\.md$/, '.html').replace(/(^|\/)index\.html$/, '$1')}`
    return [
      ['meta', { property: 'og:title', content: title }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: url }],
    ]
  },
  // Files with no page: preview image, crawler rules, and llms.txt (a page map for AI tools, llmstxt.org)
  buildEnd({ outDir }) {
    for (const file of localDownloads) {
      const destination = path.join(outDir, file)
      fs.mkdirSync(path.dirname(destination), { recursive: true })
      fs.copyFileSync(new URL(`../../${file}`, import.meta.url), destination)
    }
    fs.copyFileSync(new URL('./theme/og.png', import.meta.url), path.join(outDir, 'og.png'))
    fs.writeFileSync(path.join(outDir, 'robots.txt'), `User-agent: *\nAllow: /\n\nSitemap: ${site}/sitemap.xml\n`)
    const siteSources: Record<string, string> = { '/overview': 'README.md', '/start': 'site/start.md', '/devices': 'site/devices.md', '/reference': 'site/reference.md' }
    const md = (link: string) => `${repo}/raw/main/${siteSources[link] ?? (link.startsWith('/hardware/') ? `site${link}.md` : `${link.slice(1)}.md`)}`
    // The sidebar's Tools group only holds anchors, so it gets its own section below
    const sections = sidebar.filter((g) => g.text !== 'Tools').map((g) => [`## ${g.text}`,
      ...sidebarLinks(g.items ?? []).filter((item) => !item.link.includes('#')).map((i) => {
        const link = i.link.replace(/\.html$/, '')
        const owner = guides.find((guide) => guide.href === `${link}.html`)
        const label = i.text === 'Full guide' && owner ? `${owner.shortTitle} full guide` : i.text
        return `- [${label}](${site}${link}.html): Markdown source ${md(link)}`
      })].join('\n'))
    fs.writeFileSync(path.join(outDir, 'llms.txt'), [
      '# HWID Privacy',
      '> Guides for the hardware identifiers of a Windows PC: which parts carry a fixed ID, who reads it, and how to change it. Each part has its own guide with tested steps, risk and evidence grades. HWIDChecker is a Windows app that lists the current identifiers.',
      `Source repository: ${repo}`,
      ...sections,
      `## Tools\n- [HWIDChecker.exe](${repo}/raw/main/HWIDChecker.exe): Windows app that lists hardware identifiers`,
    ].join('\n\n') + '\n')
    validateGuideRoutes(outDir)
  },
  transformPageData(page) {
    const route = `/${page.relativePath.replace(/\.md$/, '.html')}`
    if (guides.some((guide) => guide.href === route) || /^\/hardware\//.test(route)) {
      // Sidebar siblings are alternatives, not a mandatory operation sequence.
      page.frontmatter.prev = false
      page.frontmatter.next = false
    }
  },
  // Pages live above site/, so point their imports at site/node_modules
  vite: { resolve: { alias: { vue: fileURLToPath(new URL('../node_modules/vue', import.meta.url)) } } },

  markdown: {
    // GitHub-style heading ids, so the existing "#2-storage" style links keep working
    anchor: {
      slugify: (s) => s.trim().toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/\s/g, '-'),
    },
    config(md) {
      const fence = md.renderer.rules.fence!
      md.renderer.rules.fence = (tokens, idx, options, env, self) => {
        if (tokens[idx].info.trim() === 'mermaid') {
          const marker = tokens[idx].content.match(/^%% hwid-journey:([a-z][a-z0-9-]*)$/m)
          if (marker && getJourney(marker[1]))
            return `<DecisionTree guide-id="${marker[1]}" />\n`
        }
        return fence(tokens, idx, options, env, self)
      }
      const headingOpen = md.renderer.rules.heading_open ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
      md.renderer.rules.heading_open = (tokens, idx, options, env, self) => {
        const icon = headingIcon(env.relativePath ?? '', tokens[idx].attrGet('id') ?? '', tokens[idx].tag)
        return headingOpen(tokens, idx, options, env, self) + (icon ? iconMarkup(icon, 'wiki-icon wiki-heading-icon') : '')
      }

      // Place each explanation beside its source section, after the first list
      // or table (or at the section end). The canonical Markdown stays intact.
      md.core.ruler.push('hwid_guide_visuals', (state) => {
        const visual = guideVisuals[state.env.relativePath as keyof typeof guideVisuals]
        if (!visual) return
        const tokens = state.tokens
        const start = tokens.findIndex((token, i) => token.type === 'heading_open' && token.tag === 'h2' && tokens[i + 1]?.content === visual.afterHeading)
        if (start < 0) throw new Error(`Visual section not found: ${state.env.relativePath}: ${visual.afterHeading}`)
        let end = start + 3
        while (end < tokens.length && !(tokens[end].type === 'heading_open' && ['h1', 'h2'].includes(tokens[end].tag))) end++
        let insert = end
        for (let i = start + 3; i < end; i++) {
          if (['table_close', 'bullet_list_close', 'ordered_list_close'].includes(tokens[i].type) && tokens[i].level === tokens[start].level) {
            insert = i + 1
            break
          }
        }
        const block = new state.Token('html_block', '', 0)
        block.content = `<GuideVisual visual-id="${visual.id}" />\n`
        tokens.splice(insert, 0, block)
      })

      md.core.ruler.push('hwid_image_figures', (state) => {
        const tokens = state.tokens
        for (let i = 0; i < tokens.length - 2; i++) {
          const children = tokens[i + 1].children
          if (tokens[i].type !== 'paragraph_open' || tokens[i + 1].type !== 'inline'
            || tokens[i + 2].type !== 'paragraph_close' || children?.length !== 1 || children[0].type !== 'image') continue
          tokens[i].tag = tokens[i + 2].tag = 'figure'
          tokens[i].hidden = tokens[i + 2].hidden = false
          tokens[i].attrJoin('class', 'wiki-figure')
          children[0].meta = { ...children[0].meta, inFigure: true }
          const alt = children[0].content.trim()
          if (alt && !/^step\s+\d+$/i.test(alt)) {
            const caption = new state.Token('html_block', '', 0)
            caption.content = `<figcaption aria-hidden="true" v-pre>${md.utils.escapeHtml(alt)}</figcaption>\n`
            tokens.splice(i + 2, 0, caption)
            i++
          }
          i += 2
        }
      })

      // Some existing screenshots share a paragraph with another image or a
      // numbered step. A block-styled span frames these without invalid HTML
      // or changing the surrounding list/paragraph structure.
      const renderImage = md.renderer.rules.image!
      md.renderer.rules.image = (tokens, idx, options, env, self) => {
        const image = renderImage(tokens, idx, options, env, self)
        if (tokens[idx].meta?.inFigure) return image
        const alt = tokens[idx].content.trim()
        const caption = alt && !/^step\s+\d+$/i.test(alt)
          ? `<span class="wiki-image-caption" aria-hidden="true" v-pre>${md.utils.escapeHtml(alt)}</span>` : ''
        return `<span class="wiki-image-frame">${image}${caption}</span>`
      }

      // Downloads and app source files are not part of the site; link them to the file in the repo
      const render = md.renderer.rules.link_open ?? ((t, i, o, _e, self) => self.renderToken(t, i, o))
      md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
        let href = tokens[idx].attrGet('href')
        // Generated hub Markdown uses repository-relative links for GitHub.
        // Rewrites move those pages up one directory on the website, so map
        // their source links to public routes before VitePress validates them.
        const originalSource = path.relative(repoRoot, env.realPath ?? env.path ?? '').replace(/\\/g, '/')
        if (originalSource.startsWith('site/hardware/') && href && !/^[a-z]+:|^#|^\//i.test(href)) {
          const [sourceHref, fragment] = href.split('#')
          const target = path.posix.normalize(path.posix.join(path.posix.dirname(originalSource), decodeURI(sourceHref)))
          const publicPath = target === 'site/home.md' ? '/'
            : target.startsWith('site/') ? `/${target.slice(5).replace(/\.md$/, '.html')}`
            : `/${target.replace(/\.md$/, '.html')}`
          href = `${publicPath}${fragment ? `#${fragment}` : ''}`
          tokens[idx].attrSet('href', href)
        }
        // README.md is the site's overview page (see rewrites)
        if (href && /(^|\/)README\.md(#|$)/.test(href)) tokens[idx].attrSet('href', href.replace('README.md', 'overview.md'))
        if (href && !/^[a-z]+:|^#/i.test(href)) {
          const file = path.posix.resolve('/', path.posix.dirname(env.relativePath), decodeURI(href.split('#')[0])).slice(1)
          const kind = /\.(zip|exe)$/i.test(file) ? 'raw' : file.startsWith('app/') ? 'blob' : null
          if (localDownloads.includes(file)) tokens[idx].attrSet('href', `/${encodeURI(file)}`)
          else if (kind) tokens[idx].attrSet('href', `${repo}/${kind}/main/${encodeURI(file)}`)
        }
        return render(tokens, idx, options, env, self)
      }

      // Preserve table content and headers in HTML, while giving narrow-screen
      // readers an explicit label beside each value. GitHub Markdown is unchanged.
      md.core.ruler.push('hwid_responsive_tables', (state) => {
        const tokens = state.tokens
        for (let start = 0; start < tokens.length; start++) {
          if (tokens[start].type !== 'table_open') continue
          let end = start + 1
          while (end < tokens.length && tokens[end].type !== 'table_close') end++
          const headers: string[] = []
          for (let i = start; i < end; i++) {
            if (tokens[i].type === 'th_open') {
              const inline = tokens[i + 1]
              headers.push(inline.children?.filter((token) => ['text', 'code_inline'].includes(token.type)).map((token) => token.content).join('') || inline.content)
            }
          }
          if (headers.length < 3) { start = end; continue }
          tokens[start].attrJoin('class', 'stack')
          tokens[start].attrSet('role', 'table')
          let column = 0
          for (let i = start + 1; i < end; i++) {
            const token = tokens[i]
            if (token.type === 'thead_open' || token.type === 'tbody_open') token.attrSet('role', 'rowgroup')
            if (token.type === 'tr_open') { column = 0; token.attrSet('role', 'row') }
            if (token.type === 'th_open') { token.attrSet('scope', 'col'); token.attrSet('role', 'columnheader') }
            if (token.type === 'td_open') {
              token.attrSet('role', 'cell')
              token.attrSet('data-label', headers[column++] ?? '')
              token.meta = { ...token.meta, stacked: true }
            }
            if (token.type === 'td_close') token.meta = { ...token.meta, stacked: true }
          }
          start = end
        }
      })
      const cellOpen = md.renderer.rules.td_open ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
      const cellClose = md.renderer.rules.td_close ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
      // VitePress 1.6.4's default table renderer hardcodes the opener and drops
      // token attributes. Keep its keyboard focus plus our responsive metadata.
      md.renderer.rules.table_open = (tokens, idx, options, _env, self) => {
        tokens[idx].attrSet('tabindex', '0')
        return self.renderToken(tokens, idx, options)
      }
      md.renderer.rules.td_open = (tokens, idx, options, env, self) => cellOpen(tokens, idx, options, env, self) + (tokens[idx].meta?.stacked
        ? `<span class="table-label" aria-hidden="true">${md.utils.escapeHtml(tokens[idx].attrGet('data-label') ?? '')}</span><span class="table-cell-value">` : '')
      md.renderer.rules.td_close = (tokens, idx, options, env, self) => (tokens[idx].meta?.stacked ? '</span>' : '') + cellClose(tokens, idx, options, env, self)

      md.core.ruler.push('hwid_site', (state) => {
        const tokens = state.tokens

        for (const t of tokens) {
          // Evidence grades become colored badges, also inside bold text and table cells
          if (t.type === 'inline' && t.children) {
            t.children = t.children.flatMap((c) => {
              if (c.type !== 'text' || !/\[(CC|C|A|S)\]/.test(c.content)) return [c]
              return c.content.split(/\[(CC|C|A|S)\]/).flatMap((part, i) => {
                const tok = new state.Token(i % 2 ? 'html_inline' : 'text', '', 0)
                if (i % 2) {
                  const [type, title] = grades[part]
                  tok.content = `<Badge type="${type}" text="${part}" title="${title}" />`
                } else tok.content = part
                return part ? [tok] : []
              })
            })
          }
          // Mark "outdated" <details> blocks so CSS can color them
          if (t.type === 'html_block' && /<summary>[^<]*outdated/i.test(t.content))
            t.content = t.content.replace(/<details>/, '<details class="outdated">')
        }

        // Fold everything under "## Sources" into a closed <details>
        const start = tokens.findIndex((t, i) => t.type === 'heading_open' && t.tag === 'h2' && tokens[i + 1]?.content.trim() === 'Sources')
        if (start < 0) return
        const from = start + 3 // after heading_open, inline, heading_close
        let end = tokens.findIndex((t, i) => i >= from && t.type === 'heading_open' && (t.tag === 'h1' || t.tag === 'h2'))
        if (end < 0) end = tokens.length
        const items = tokens.slice(from, end).filter((t) => t.type === 'list_item_open')
        const top = Math.min(...items.map((t) => t.level))
        const n = items.filter((t) => t.level === top).length
        const open = new state.Token('html_block', '', 0)
        open.content = `<details class="sources"><summary>Sources (${n})</summary>\n`
        const close = new state.Token('html_block', '', 0)
        close.content = '</details>\n'
        tokens.splice(end, 0, close)
        tokens.splice(from, 0, open)
      })
    },
  },

  themeConfig: {
    logo,
    search: { provider: 'local', options: {
      detailedView: true,
      // Local search runs before Vue SSR. Index the same curated data used by
      // the components, with fragments that exist in their rendered markup.
      _render(src, env, md) {
        let html = md.render(src, env)
        if (env.frontmatter?.search === false) return ''
        const escape = md.utils.escapeHtml
        // VitePress's index splitter recognizes headings by their anchor link,
        // not by id alone. Match its native heading shape.
        const heading = (id: string, title: string) => `<h2 id="${escape(id)}">${escape(title)}<a class="header-anchor" href="#${escape(id)}" aria-hidden="true">#</a></h2>`
        if (env.relativePath === 'site/devices.md') {
          html += guides.map((guide) => heading(guide.id, guide.title)
            + `<p>${escape(`${guide.group}. ${guide.description}`)}</p>`
            + guide.methods.map((method) => `<p>${escape(`${method.label}. ${method.description} ${method.note ?? ''}`)}</p>`).join('')).join('')
        }
        if (env.relativePath === 'site/start.md') {
          html += workflowSteps.map((step) => heading(step.id, step.title)
            + `<p>${escape(step.description)} ${escape(step.links.map((link) => link.label).join('. '))}</p>`).join('')
          html += startJourney.nodes.map((node) => {
            const destination = startJourney.destinations.find((item) => item.id === node.destinationId)
            return heading(`node-${node.id}`, node.label)
              + `<p>${escape(destination ? destination.status : startJourney.intro)}</p>`
          }).join('')
        }
        const journey = journeys.find((item) => env.relativePath === `site/hardware/${item.guideId}.md`
          || env.relativePath === `hardware/${item.guideId}.md`)
        if (journey) {
          html += journey.nodes.map((node) => {
            const destination = journey.destinations.find((item) => item.id === node.destinationId)
            return heading(`node-${node.id}`, node.label)
              + `<p>${escape(destination ? `${destination.status}. ${destination.note ?? ''}` : journey.intro)}</p>`
          }).join('')
        }
        const visual = guideVisuals[env.relativePath ?? '']
        if (visual) {
          html += heading(`visual-${visual.id}`, visual.title)
            + visual.items.map((item) => `<p>${escape(`${item.title}. ${item.text}`)}</p>`).join('')
            + `<p>${escape(visual.note)}</p>`
        }
        return html
      },
    } },
    nav: topNavigation,
    sidebar: Object.fromEntries(Object.entries(siteSidebars).map(([route, items]) => [route, illustratedSidebar(items)])),
    outline: { level: [2, 3] },
    socialLinks: [{ icon: 'github', link: repo }],
    editLink: {
      // Theme callbacks are serialized; do not capture server-only constants.
      pattern: (page) => `https://github.com/Fundryi/HWID-Privacy/edit/main/${/^hardware\//.test(page.relativePath) ? 'site/.vitepress/theme/journey-data.ts' : page.filePath}`,
      text: 'Edit this page on GitHub',
    },
  },
})
