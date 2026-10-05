import DefaultTheme from 'vitepress/theme'
import { defineComponent, h } from 'vue'
import { useData, type Theme } from 'vitepress'
import { guides } from './guide-data'
import WikiHome from './components/WikiHome.vue'
import WorkflowMap from './components/WorkflowMap.vue'
import DeviceBrowser from './components/DeviceBrowser.vue'
import GuideContext from './components/GuideContext.vue'
import GuideEnd from './components/GuideEnd.vue'
import EvidenceLegend from './components/EvidenceLegend.vue'
import GuideVisual from './components/GuideVisual.vue'
import DecisionTree from './components/DecisionTree.vue'
import { useSidebarLocation } from './sidebar-location'
import './custom.css'
import './visuals.css'
import './reading.css'
import './navigation.css'
import './journey-layout.css'

export default {
  extends: DefaultTheme,
  Layout: defineComponent({
    setup() {
      const { page } = useData()
      useSidebarLocation()
      return () => {
        const source = page.value.relativePath.replace(/\.md$/, '')
        const guide = guides.find((item) => item.href.replace(/^\//, '').replace(/\.html$/, '') === source)
        return h(DefaultTheme.Layout, null, {
          'doc-before': () => guide ? h(GuideContext, { guide }) : null,
          'doc-footer-before': () => guide ? h(GuideEnd, { guide }) : null,
        })
      }
    },
  }),
  enhanceApp({ app }) {
    app.component('WikiHome', WikiHome)
    app.component('WorkflowMap', WorkflowMap)
    app.component('DeviceBrowser', DeviceBrowser)
    app.component('EvidenceLegend', EvidenceLegend)
    app.component('GuideVisual', GuideVisual)
    app.component('DecisionTree', DecisionTree)
  },
} satisfies Theme
