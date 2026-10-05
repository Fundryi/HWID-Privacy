<script setup lang="ts">
import { guides } from '../guide-data'
import { hubHref } from '../journey-data'
import WikiIcon from './WikiIcon.vue'

withDefaults(defineProps<{ section?: 'jump' | 'methods' | 'verify' }>(), { section: 'methods' })

const groups = ['Firmware', 'Storage', 'Network', 'Peripherals']
const riskClass = (risk: string) =>
  /high/.test(risk) && /low|medium/.test(risk) ? 'mixed' : risk.toLowerCase()
</script>

<template>
  <nav v-if="section === 'jump'" class="wiki-device-jumps" aria-label="Jump to a component">
    <span>Jump to:</span>
    <ul>
      <li v-for="guide in guides" :key="guide.id">
        <a :href="`#${guide.id}`"><WikiIcon :name="guide.id" />{{ guide.shortTitle }}</a>
      </li>
    </ul>
  </nav>

  <ul v-else-if="section === 'verify'" class="wiki-verification-list">
    <li v-for="guide in guides" :key="guide.id">
      <a :href="guide.verify.href">
        <span class="wiki-verification-component"><WikiIcon :name="guide.id" />{{ guide.shortTitle }}</span>
        <span>{{ guide.verify.label }}</span>
      </a>
    </li>
  </ul>

  <div v-else class="wiki-device-browser">
    <section v-for="group in groups" :key="group" class="wiki-device-group">
      <h2 :id="`group-${group.toLowerCase()}`">{{ group }}</h2>

      <article
        v-for="guide in guides.filter((item) => item.group === group)"
        :aria-labelledby="guide.id"
        :key="guide.id"
        class="wiki-device-panel"
      >
        <div class="wiki-device-header">
          <span class="wiki-icon-tile"><WikiIcon :name="guide.id" /></span>
          <h3 :id="guide.id"><a :href="hubHref(guide.id)">{{ guide.title }}</a></h3>
          <div class="wiki-chips">
            <span class="wiki-chip" :class="riskClass(guide.risk)">Risk: {{ guide.risk }}</span>
            <span class="wiki-chip">Difficulty: {{ guide.difficulty }}</span>
          </div>
          <p>{{ guide.description }}</p>
          <p class="wiki-identify">
            Not sure which one you have?
            <a :href="guide.identify.href">{{ guide.identify.label }}</a>
          </p>
        </div>

        <ul class="wiki-method-list" :aria-label="`${guide.shortTitle} methods`">
          <li v-for="method in guide.methods" :key="method.id">
            <a :href="method.href">{{ method.label }}</a>
            <p v-if="method.description">{{ method.description }}</p>
            <p v-if="method.note" class="wiki-method-note">{{ method.note }}</p>
          </li>
        </ul>

        <nav class="wiki-device-links" :aria-label="`${guide.shortTitle} preparation and checks`">
          <a :href="guide.prepare.href"><WikiIcon name="prepare" />{{ guide.prepare.label }}</a>
          <a :href="guide.verify.href"><WikiIcon name="verify" />{{ guide.verify.label }}</a>
          <a v-if="guide.troubleshoot" :href="guide.troubleshoot.href"><WikiIcon name="troubleshoot" />{{ guide.troubleshoot.label }}</a>
        </nav>
      </article>
    </section>
  </div>
</template>
