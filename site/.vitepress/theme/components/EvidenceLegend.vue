<script setup lang="ts">
import { evidenceGrades } from '../guide-data'

defineProps<{ compact?: boolean }>()
</script>

<template>
  <details class="wiki-evidence-legend" :class="{ compact }">
    <summary>Evidence grades</summary>
    <dl>
      <div v-for="grade in evidenceGrades" :key="grade.code">
        <dt><span class="wiki-evidence-code">{{ grade.code }}</span> {{ grade.label }}</dt>
        <dd>{{ grade.description }}</dd>
      </div>
    </dl>
  </details>
</template>
