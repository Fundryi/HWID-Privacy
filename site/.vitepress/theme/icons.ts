import { guides } from './guide-data'

// Original geometric pictograms. One source is shared by Vue and Markdown
// rendering, so a component keeps the same symbol everywhere in the guide.
export const iconPaths = {
  motherboard: ['M4 3h16v18H4z', 'M8 7h7v7H8zM17 6v10M7 17h7M7 5v2m3-2v2m3-2v2M6 9h2m-2 3h2M10 14v2m3-2v2'],
  nvram: ['M6 5h12v14H6z', 'M9 9h6M9 12h6M9 15h3M9 2v3m6-3v3M9 19v3m6-3v3M3 8h3m-3 8h3m12-8h3m-3 8h3'],
  tpm: ['M12 3 20 6v6c0 4-4 7-8 9-4-2-8-5-8-9V6z', 'M14 11a2 2 0 1 1-4 0 2 2 0 0 1 4 0zM12 13v4'],
  ftpm: ['M12 3 20 6v6c0 4-4 7-8 9-4-2-8-5-8-9V6z', 'M9 9h6v6H9zM11 7v2m2-2v2M11 15v2m2-2v2M7 11h2m-2 2h2m6-2h2m-2 2h2'],
  storage: ['M5 3h14v18H5z', 'M8 7h8M8 10h8M9 17h1m4 0h2'],
  network: ['M4 4h16v13h-4v4H8v-4H4z', 'M8 4v5m3-5v5m3-5v5m3-5v5M8 13h8'],
  router: ['M4 13h16v7H4zM7 16h1m3 0h1M7 13V7m10 6V7', 'M5 6a10 10 0 0 1 14 0M8 9a6 6 0 0 1 8 0'],
  memory: ['M3 6h18v11H3zM6 9h3v5H6zM12 9h3v5h-3z', 'M6 17v3m3-3v3m3-3v3m3-3v3m3-3v3M18 9v5'],
  display: ['M3 4h18v13H3zM8 21h8M12 17v4', 'M7 8h5M7 11h9'],
  understand: ['M3 4h6c2 0 3 1 3 3 0-2 1-3 3-3h6v15h-6c-2 0-3 1-3 2 0-1-1-2-3-2H3zM12 7v14'],
  inspect: ['M16 10a6 6 0 1 1-12 0 6 6 0 0 1 12 0zM15 15l6 6'],
  prepare: ['M3 8h18v12H3zM8 8V4h8v4M3 12h18M10 11v4h4v-4'],
  method: ['M6 3v18M6 6h10l4 4-4 4H6'],
  verify: ['M21 12a9 9 0 1 1-9-9M7 11l5 5L22 6'],
  troubleshoot: ['M15 3a6 6 0 0 0-7 8L3 16a3 3 0 0 0 5 5l5-5a6 6 0 0 0 8-7l-5 3-4-4z'],
  sources: ['M5 3h12l3 3v15H5zM17 3v4h3M8 10h9M8 14h9M8 18h5'],
  recovery: ['M4 9a8 8 0 1 1 0 7M4 9V3m0 6h6M12 7v5l3 2'],
  workflow: ['M4 3h7v5H4zM13 16h7v5h-7zM7 8v5h9v3M13 10l3 3-3 3'],
  overview: ['M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z'],
} as const

export type IconName = keyof typeof iconPaths

export function isIconName(name: string): name is IconName {
  return Object.prototype.hasOwnProperty.call(iconPaths, name)
}

export function iconMarkup(name: IconName, className = 'wiki-icon'): string {
  return `<svg class="${className}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">${iconPaths[name].map((d) => `<path d="${d}"/>`).join('')}</svg>`
}

export function iconForHref(href: string): IconName | undefined {
  const route = href.split(/[?#]/)[0].replace(/\.html$/, '').replace(/\/$/, '')
  if (route === '/start') return 'workflow'
  if (route === '/devices' || route === '/overview') return 'overview'
  if (route === '/reference') return 'sources'
  if (route.startsWith('/hardware/')) {
    const id = route.slice('/hardware/'.length)
    if (guides.some((guide) => guide.id === id) && isIconName(id)) return id
  }
  if (route.includes('/getting-started/')) return href.includes('#hwidchecker') ? 'inspect' : 'understand'
  const guide = guides.find((item) => item.href.replace(/\.html$/, '') === route)
  return guide && isIconName(guide.id) ? guide.id : undefined
}
