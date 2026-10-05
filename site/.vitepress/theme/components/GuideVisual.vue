<script setup lang="ts">
import { computed } from 'vue'
import { guideVisuals } from '../guide-visuals'
import WikiIcon from './WikiIcon.vue'

const props = defineProps<{ visualId: string }>()
const visual = computed(() => Object.values(guideVisuals).find((item) => item.id === props.visualId))
</script>

<template>
  <figure
    v-if="visual"
    :id="`visual-${visual.id}`"
    class="wiki-guide-visual"
    :class="[`wiki-guide-visual-${visual.kind}`, `wiki-visual-columns-${visual.items.length}`]"
  >
    <figcaption>
      <span class="wiki-visual-kind">{{ visual.kind === 'flow' ? 'Observation path' : 'Compare the scope' }}</span>
      <strong>{{ visual.title }}</strong>
    </figcaption>

    <component :is="visual.kind === 'flow' ? 'ol' : 'ul'" class="wiki-visual-items">
      <li v-for="item in visual.items" :key="item.title" class="wiki-visual-item">
        <span class="wiki-icon-tile"><WikiIcon :name="item.icon" /></span>
        <strong>{{ item.title }}</strong>
        <p><template v-for="(part, index) in item.text.split(/(_)/)" :key="index">{{ part }}<wbr v-if="part === '_'" /></template></p>
      </li>
    </component>

    <p class="wiki-visual-note">{{ visual.note }}</p>
    <nav class="wiki-visual-sources" aria-label="Guide sections supporting this visual">
      <span><WikiIcon name="sources" />In this guide:</span>
      <a v-for="source in visual.sources" :key="source.href" :href="source.href">{{ source.label }}</a>
    </nav>
  </figure>
</template>
