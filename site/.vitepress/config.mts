import { defineConfig } from 'vitepress'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repo = 'https://github.com/Fundryi/HWID-Privacy'

// Guides stay in the repo root so they still read fine on GitHub.
// The site reads them from there; nothing is copied.
export default defineConfig({
  title: 'HWID Privacy',
  description: 'Hardware identifier privacy guides',
  base: '/', // served from the custom domain set in Settings > Pages
  srcDir: '..',
  srcExclude: ['app/**', 'site/**', 'docs/**', 'TMP/**', 'AGENTS.md', 'AI_TOOLS.md', 'CLAUDE*.md'],
  rewrites: { 'README.md': 'index.md' },
  lastUpdated: true,
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
        // README.md is the site's home page (see rewrites)
        if (href && /(^|\/)README\.md(#|$)/.test(href)) tokens[idx].attrSet('href', href.replace('README.md', 'index.md'))
        if (href && !/^[a-z]+:|^#/i.test(href)) {
          const file = path.posix.resolve('/', path.posix.dirname(env.relativePath), decodeURI(href.split('#')[0])).slice(1)
          const kind = /\.(zip|exe|bat)$/i.test(file) ? 'raw' : file.startsWith('app/') ? 'blob' : null
          if (kind) tokens[idx].attrSet('href', `${repo}/${kind}/main/${encodeURI(file)}`)
        }
        return render(tokens, idx, options, env, self)
      }
    },
  },

  themeConfig: {
    search: { provider: 'local' },
    sidebar: [
      { text: 'Overview', link: '/' },
      { text: 'Getting Started', link: '/guides/getting-started/getting-started' },
      {
        text: 'Guides',
        items: [
          { text: 'Motherboard (SMBIOS)', link: '/guides/motherboard-spoofing/motherboard-spoofing' },
          { text: 'NVRAM (EFI variables)', link: '/guides/nvram-spoofing/nvram-spoofing' },
          { text: 'Storage (SSD)', link: '/guides/ssd-spoofing/ssd-spoofing' },
          { text: 'MAC Address', link: '/guides/mac-spoofing/mac-spoofing' },
          { text: 'RAM (SPD)', link: '/guides/ram-spoofing/ram-spoofing' },
          { text: 'Monitor (EDID)', link: '/guides/monitor-spoofing/monitor-spoofing' },
          { text: 'Router (ARP)', link: '/guides/arp-spoofing/arp-spoofing' },
          { text: 'TPM', link: '/guides/tpm-spoofing/tpm-spoofing' },
          { text: 'fTPM Reset (AM5)', link: '/guides/resets/ftpm-reset-tutorial' },
        ],
      },
    ],
    outline: { level: [2, 3] },
    socialLinks: [{ icon: 'github', link: repo }],
    editLink: { pattern: `${repo}/edit/main/:path`, text: 'Edit this page on GitHub' },
  },
})
