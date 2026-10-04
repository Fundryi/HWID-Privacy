import { defineConfig } from 'vitepress'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repo = 'https://github.com/Fundryi/HWID-Privacy'
const site = 'https://hwid.idkzal.cc'
const gettingStarted = '/guides/getting-started/getting-started'
// srcDir is the repo root, so there is no public/ folder; the logo travels as a data URI
const logo = `data:image/svg+xml;base64,${fs.readFileSync(new URL('./theme/logo.svg', import.meta.url)).toString('base64')}`

// Work order. Previous / next buttons follow this list.
const sidebar = [
  {
    text: 'Start',
    items: [
      { text: 'Overview', link: '/overview' },
      { text: 'Getting Started', link: gettingStarted },
    ],
  },
  {
    text: 'Firmware',
    items: [
      { text: 'Motherboard (SMBIOS)', link: '/guides/motherboard-spoofing/motherboard-spoofing' },
      { text: 'NVRAM (EFI variables)', link: '/guides/nvram-spoofing/nvram-spoofing' },
      { text: 'TPM', link: '/guides/tpm-spoofing/tpm-spoofing' },
      { text: 'fTPM Reset (AM5)', link: '/guides/resets/ftpm-reset-tutorial' },
    ],
  },
  {
    text: 'Storage',
    items: [{ text: 'SSD', link: '/guides/ssd-spoofing/ssd-spoofing' }],
  },
  {
    text: 'Network',
    items: [
      { text: 'MAC Address', link: '/guides/mac-spoofing/mac-spoofing' },
      { text: 'Router (ARP)', link: '/guides/arp-spoofing/arp-spoofing' },
    ],
  },
  {
    text: 'Peripherals',
    items: [
      { text: 'RAM (SPD)', link: '/guides/ram-spoofing/ram-spoofing' },
      { text: 'Monitor (EDID)', link: '/guides/monitor-spoofing/monitor-spoofing' },
    ],
  },
  {
    // In-site anchors, so prev / next never points at a download
    text: 'Tools',
    items: [
      { text: 'HWIDChecker', link: `${gettingStarted}#hwidcheckerexe` },
    ],
  },
]

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
    'site/node_modules/**', 'site/.vitepress/**', 'site/package*.json',
  ],
  // site/home.md is the site-only landing page; the GitHub README becomes the overview
  rewrites: { 'site/home.md': 'index.md', 'README.md': 'overview.md' },
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
    fs.copyFileSync(new URL('./theme/og.png', import.meta.url), path.join(outDir, 'og.png'))
    fs.writeFileSync(path.join(outDir, 'robots.txt'), `User-agent: *\nAllow: /\n\nSitemap: ${site}/sitemap.xml\n`)
    const md = (link: string) => `${repo}/raw/main/${link === '/overview' ? 'README.md' : `${link.slice(1)}.md`}`
    // The sidebar's Tools group only holds anchors, so it gets its own section below
    const sections = sidebar.filter((g) => g.text !== 'Tools').map((g) => [`## ${g.text}`,
      ...g.items.map((i) => `- [${i.text}](${site}${i.link}.html): Markdown source ${md(i.link)}`)].join('\n'))
    fs.writeFileSync(path.join(outDir, 'llms.txt'), [
      '# HWID Privacy',
      '> Guides for the hardware identifiers of a Windows PC: which parts carry a fixed ID, who reads it, and how to change it. Each part has its own guide with tested steps, risk and evidence grades. HWIDChecker is a Windows app that lists the current identifiers.',
      `Source repository: ${repo}`,
      ...sections,
      `## Tools\n- [HWIDChecker.exe](${repo}/raw/main/HWIDChecker.exe): Windows app that lists hardware identifiers`,
    ].join('\n\n') + '\n')
  },
  // Landing page hero shows the logo
  transformPageData(page) {
    if (page.frontmatter.layout === 'home') page.frontmatter.hero.image = { src: logo, alt: '' }
  },
  // Pages live above site/, so point their imports at site/node_modules
  vite: { resolve: { alias: { vue: fileURLToPath(new URL('../node_modules/vue', import.meta.url)) } } },

  markdown: {
    // GitHub-style heading ids, so the existing "#2-storage" style links keep working
    anchor: {
      slugify: (s) => s.trim().toLowerCase().replace(/[^\p{L}\p{M}\p{N}\s_-]/gu, '').replace(/\s/g, '-'),
    },
    config(md) {
      // Downloads and app source files are not part of the site; link them to the file in the repo
      const render = md.renderer.rules.link_open ?? ((t, i, o, _e, self) => self.renderToken(t, i, o))
      md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
        const href = tokens[idx].attrGet('href')
        // README.md is the site's overview page (see rewrites)
        if (href && /(^|\/)README\.md(#|$)/.test(href)) tokens[idx].attrSet('href', href.replace('README.md', 'overview.md'))
        if (href && !/^[a-z]+:|^#/i.test(href)) {
          const file = path.posix.resolve('/', path.posix.dirname(env.relativePath), decodeURI(href.split('#')[0])).slice(1)
          const kind = /\.(zip|exe)$/i.test(file) ? 'raw' : file.startsWith('app/') ? 'blob' : null
          if (kind) tokens[idx].attrSet('href', `${repo}/${kind}/main/${encodeURI(file)}`)
        }
        return render(tokens, idx, options, env, self)
      }

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
    search: { provider: 'local', options: { detailedView: true } },
    nav: [
      { text: 'Get started', link: gettingStarted },
      { text: 'Overview', link: '/overview' },
      { text: 'Download', link: `${repo}/raw/main/HWIDChecker.exe` },
    ],
    sidebar,
    outline: { level: [2, 3] },
    socialLinks: [{ icon: 'github', link: repo }],
    editLink: { pattern: `${repo}/edit/main/:path`, text: 'Edit this page on GitHub' },
  },
})
