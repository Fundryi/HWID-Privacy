<script setup lang="ts">
import { computed } from 'vue'
import { iconPaths, isIconName } from '../icons'

const props = defineProps<{ name: string }>()
const paths = computed(() => isIconName(props.name) ? iconPaths[props.name] : iconPaths.overview)
</script>

<template>
  <svg class="wiki-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">
    <path v-for="(path, index) in paths" :key="index" :d="path" />
  </svg>
</template>
