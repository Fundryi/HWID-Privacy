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
      slugify: (s) => s.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s/g, '-'),
    },
    config(md) {
      // Downloads (.zip/.exe/.bat) are not part of the site; link them to the file in the repo
      const render = md.renderer.rules.link_open ?? ((t, i, o, _e, self) => self.renderToken(t, i, o))
      md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
        const href = tokens[idx].attrGet('href')
        if (href && !/^[a-z]+:/i.test(href) && /\.(zip|exe|bat)$/i.test(href)) {
          const file = path.posix.resolve('/', path.posix.dirname(env.relativePath), decodeURI(href)).slice(1)
          tokens[idx].attrSet('href', `${repo}/raw/main/${encodeURI(file)}`)
        }
        return render(tokens, idx, options, env, self)
      }
    },
  },

  themeConfig: {
    search: { provider: 'local' },
    sidebar: [
      { text: 'Overview', link: '/' },
      {
        text: 'Guides',
        items: [
          { text: 'Motherboard', link: '/guides/motherboard-spoofing/motherboard-spoofing' },
          { text: 'SSD', link: '/guides/ssd-spoofing/ssd-spoofing' },
          { text: 'MAC Address', link: '/guides/mac-spoofing/mac-spoofing' },
          { text: 'TPM', link: '/guides/tpm-spoofing/tpm-spoofing' },
        ],
      },
    ],
    outline: { level: [2, 3] },
    socialLinks: [{ icon: 'github', link: repo }],
    editLink: { pattern: `${repo}/edit/main/:path`, text: 'Edit this page on GitHub' },
  },
})
