<script setup lang="ts">
import { computed } from 'vue'
import { useData } from 'vitepress'
import { workflowSteps, type GuideRoute } from '../guide-data'
import { hubHref } from '../journey-data'
import { canonicalChooserHref } from '../../navigation'
import WikiIcon from './WikiIcon.vue'

const props = defineProps<{ guide: GuideRoute }>()
const { hash } = useData()
const chooserHref = computed(() => canonicalChooserHref(props.guide.id, hash.value))
const chooserLabel = computed(() => props.guide.id === 'ftpm' ? 'fTPM' : props.guide.shortTitle)
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
  <nav class="wiki-guide-end" :aria-label="`${guide.shortTitle} verification and return links`">
    <p>Done reading?</p>
    <div>
      <a :href="chooserHref"><WikiIcon name="method" />Back to {{ chooserLabel }} chooser</a>
      <a v-if="relatedChooser" :href="`${hubHref(relatedChooser.id)}#decision-tree`"><WikiIcon :name="relatedChooser.id" />{{ relatedChooser.label }}</a>
      <a :href="guide.href"><WikiIcon name="understand" />Full guide</a>
      <a :href="localHref(guide.prepare.href)"><WikiIcon name="prepare" />Prepare</a>
      <a :href="localHref(guide.verify.href)"><WikiIcon name="verify" />{{ guide.verify.label }}</a>
      <a v-if="guide.troubleshoot" :href="localHref(guide.troubleshoot.href)"><WikiIcon name="troubleshoot" />{{ guide.troubleshoot.label }}</a>
      <a :href="returnHref"><WikiIcon name="workflow" />Return to work order</a>
    </div>
  </nav>
</template>
