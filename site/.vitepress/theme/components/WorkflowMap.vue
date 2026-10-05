<script setup lang="ts">
import { guides, workflowSteps } from '../guide-data'
import { hubHref } from '../journey-data'
import { iconForHref } from '../icons'
import WikiIcon from './WikiIcon.vue'

defineProps<{ compact?: boolean }>()

function workflowHref(stage: number, href: string): string {
  if (stage < 5 || stage > 7) return href
  const guide = guides.find((item) => item.href === href.split('#')[0])
  return guide ? hubHref(guide.id) : href
}

const overview = [
  {
    title: 'Understand',
    description: 'Learn which identifiers belong to which parts of a PC.',
    href: workflowSteps.find((step) => step.number === 1)?.links[0]?.href ?? '/start.html',
    icon: 'understand',
  },
  {
    title: 'Inspect',
    description: 'Take a baseline with HWIDChecker before making changes.',
    href: workflowSteps.find((step) => step.number === 2)?.links[0]?.href ?? '/start.html',
    icon: 'inspect',
  },
  {
    title: 'Choose a component',
    description: 'Find its methods, preparation and limits.',
    href: '/devices.html',
    icon: 'overview',
  },
  {
    title: 'Verify',
    description: 'Check the result using the component’s own guide.',
    href: '/devices.html#verify',
    icon: 'verify',
  },
]
</script>

<template>
  <nav v-if="compact" class="wiki-route-overview" aria-label="Ways to use the guides">
    <ul class="wiki-overview-rail">
      <li v-for="item in overview" :key="item.title" class="wiki-overview-stage">
        <span class="wiki-overview-node" aria-hidden="true">
          <WikiIcon :name="item.icon" />
        </span>
        <a :href="item.href">{{ item.title }}</a>
        <p>{{ item.description }}</p>
      </li>
    </ul>
  </nav>

  <ol v-else class="wiki-workflow" aria-label="Ten-stage project work order">
    <li v-for="step in workflowSteps" :id="step.id" :key="step.id" class="wiki-workflow-stage">
      <span class="wiki-workflow-node" aria-hidden="true">{{ step.number }}</span>
      <div class="wiki-workflow-content">
        <h3>{{ step.title }}</h3>
        <p v-if="step.description">{{ step.description }}</p>
        <ul class="wiki-workflow-links">
          <li v-for="link in step.links" :key="link.href">
            <a :href="workflowHref(step.number, link.href)"><WikiIcon :name="iconForHref(link.href) ?? 'method'" />{{ link.label }}</a>
          </li>
        </ul>
      </div>
    </li>
  </ol>
</template>
