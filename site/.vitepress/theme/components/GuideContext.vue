<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vitepress'
import { workflowSteps, type GuideRoute } from '../guide-data'
import { hubHref } from '../journey-data'
import { canonicalChooserHref } from '../../navigation'
import EvidenceLegend from './EvidenceLegend.vue'
import WikiIcon from './WikiIcon.vue'

const props = defineProps<{ guide: GuideRoute }>()
const { hash } = useData()
const chooserHref = computed(() => canonicalChooserHref(props.guide.id, hash.value))
const chooserLabel = computed(() => props.guide.id === 'tpm' ? 'TPM chooser' : props.guide.id === 'ftpm' ? 'fTPM chooser' : 'Choose a route')
const dockChooserLabel = computed(() => `${props.guide.id === 'ftpm' ? 'fTPM' : props.guide.shortTitle} chooser`)
const relatedChooser = computed(() => props.guide.id === 'tpm' ? { id: 'ftpm', label: 'fTPM chooser' } : props.guide.id === 'ftpm' ? { id: 'tpm', label: 'TPM chooser' } : undefined)
const returnHref = computed(() => {
  const stage = workflowSteps.find((step) =>
    step.links.some((link) => link.href.split('#')[0] === props.guide.href.split('#')[0]),
  )
  return stage ? `/start.html#${stage.id}` : '/start.html'
})
const localHref = (href: string) =>
  href.split('#')[0] === props.guide.href.split('#')[0] && href.includes('#')
    ? `#${href.split('#')[1]}`
    : href
</script>

<template>
  <div class="wiki-guide-context">
    <span class="wiki-guide-identity">
      <span class="wiki-icon-tile"><WikiIcon :name="guide.id" /></span>
      <span class="wiki-guide-location">{{ guide.group }} / {{ guide.shortTitle }}</span>
    </span>
    <nav :aria-label="`${guide.shortTitle} guide links`">
      <a :href="chooserHref"><WikiIcon name="method" />{{ chooserLabel }}</a>
      <a v-if="relatedChooser" :href="`${hubHref(relatedChooser.id)}#decision-tree`"><WikiIcon :name="relatedChooser.id" />{{ relatedChooser.label }}</a>
      <a :href="guide.href"><WikiIcon name="understand" />Full guide</a>
      <a :href="localHref(guide.prepare.href)"><WikiIcon name="prepare" />Prepare</a>
      <a :href="localHref(guide.verify.href)"><WikiIcon name="verify" />Verify</a>
      <a v-if="guide.troubleshoot" :href="localHref(guide.troubleshoot.href)"><WikiIcon name="troubleshoot" />Troubleshooting</a>
      <a :href="returnHref"><WikiIcon name="workflow" />Return to work order</a>
    </nav>
    <EvidenceLegend compact />
    <nav class="wiki-guide-chooser-dock" :aria-label="`${guide.shortTitle} chooser and guide map`">
      <a :href="chooserHref"><WikiIcon :name="guide.id" />{{ dockChooserLabel }}</a>
      <a v-if="relatedChooser" :href="`${hubHref(relatedChooser.id)}#decision-tree`"><WikiIcon :name="relatedChooser.id" />{{ relatedChooser.label }}</a>
      <a :href="`/start.html#node-${guide.id}`"><WikiIcon name="workflow" />Guide map</a>
    </nav>
  </div>
</template>
