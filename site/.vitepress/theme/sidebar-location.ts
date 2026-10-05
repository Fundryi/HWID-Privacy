import { nextTick, onMounted, onUnmounted, watch } from 'vue'
import { useData } from 'vitepress'
import { siteSidebars } from '../navigation'

const contextLinks = new Set<string>()
const sectionLinks = new Set<string>()
function collect(items: typeof siteSidebars[string]) {
  for (const item of items) {
    if (item.link) {
      if (item.contextOnly) contextLinks.add(item.link)
      else if (item.link.includes('#')) sectionLinks.add(item.link)
    }
    if (item.items) collect(item.items)
  }
}
collect(siteSidebars['/'])

/** Keep native hash selection as the sole sidebar location policy. */
export function useSidebarLocation(): void {
  const { page, hash } = useData()
  let mounted = false
  const clear = () => {
    document.querySelectorAll('.VPSidebar .wiki-sidebar-neutral-context')
      .forEach((item) => item.classList.remove('wiki-sidebar-neutral-context'))
    document.querySelectorAll('.VPSidebar a[aria-current="location"]')
      .forEach((item) => item.removeAttribute('aria-current'))
  }
  const update = () => {
    if (!mounted) return
    clear()
    const path = `/${page.value.relativePath.replace(/\.md$/, '.html')}`.replace(/^\/index\.html$/, '/')
    const sectionSelected = sectionLinks.has(`${path}${hash.value}`)
    let selectedLink: HTMLAnchorElement | undefined
    for (const link of document.querySelectorAll<HTMLAnchorElement>('.VPSidebar a.link')) {
      const url = new URL(link.href)
      const href = `${url.pathname}${url.hash}`
      if (sectionSelected && contextLinks.has(href) && url.pathname === path)
        link.closest('.VPSidebarItem')?.classList.add('wiki-sidebar-neutral-context')
      if (sectionSelected ? href === `${path}${hash.value}` : contextLinks.has(href) && url.pathname === path) {
        link.setAttribute('aria-current', 'location')
        selectedLink = link
      }
    }
    if (!selectedLink || !window.matchMedia('(min-width: 960px)').matches) return
    const sidebar = selectedLink.closest<HTMLElement>('.VPSidebar')!
    const sidebarBounds = sidebar.getBoundingClientRect()
    const linkBounds = selectedLink.getBoundingClientRect()
    const dock = document.querySelector<HTMLElement>('.wiki-guide-chooser-dock')?.getBoundingClientRect()
    const navHeight = parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--vp-nav-height')) || 64
    const top = Math.max(sidebarBounds.top, navHeight) + 12
    const bottom = Math.min(sidebarBounds.bottom, dock?.height ? dock.top : sidebarBounds.bottom) - 12
    // Reveal only inside the sidebar. scrollIntoView would also move the guide
    // away from the requested hash and obscure the distinction between menus.
    if (linkBounds.top < top) sidebar.scrollTop -= top - linkBounds.top
    else if (linkBounds.bottom > bottom) sidebar.scrollTop += linkBounds.bottom - bottom
  }
  watch([page, hash], () => { if (mounted) void nextTick(update) }, { flush: 'post' })
  // Native sidebar children update their is-active classes on mount. Wait for
  // that render before decorating the rows, or Vue overwrites our class.
  onMounted(() => { mounted = true; void nextTick(update) })
  onUnmounted(() => { clear(); mounted = false })
}
