<script setup lang="ts">
import { guides, type GuideLink } from '../guide-data'
import { hubHref } from '../journey-data'
import WikiIcon from './WikiIcon.vue'

const descriptions: Record<string, string> = {
  motherboard: 'System, baseboard and chassis fields.',
  nvram: 'Read-only EFI-variable inspection.',
  tpm: 'TPM types, keys and clear behavior.',
  ftpm: 'Platform reports and identity evidence.',
  storage: 'Drive, bridge, partition and volume IDs.',
  network: 'Windows and adapter-stored MAC addresses.',
  router: 'Routed gateways and first-hop visibility.',
  memory: 'SPD, firmware views and write protection.',
  display: 'EDID overrides, emulators and hardware paths.',
}
const exampleIds: Record<string, [string, string][]> = {
  motherboard: [['motherboard-ami-dmiedit', 'DMIEdit'], ['motherboard-insyde', 'Insyde'], ['motherboard-asus', 'ASUS scope']],
  nvram: [['nvram-read-only', 'Inventory'], ['nvram-boot-variables', 'Boot variables'], ['nvram-unsupported-write', 'Write limits']],
  tpm: [['tpm-discrete', 'TPM types'], ['tpm-clear', 'Clear behavior'], ['tpm-update-continuity', 'Firmware updates']],
  ftpm: [['ftpm-amd-tpmb', 'AMD reports'], ['ftpm-intel-z890', 'Intel evidence'], ['ftpm-pluton-toggle', 'Pluton research']],
  storage: [['storage-map1202', 'MAP1202'], ['storage-yansen-kingspec', 'SATA'], ['storage-rtl9210b', 'USB bridges']],
  network: [['network-intel', 'Intel'], ['network-realtek-usb', 'Realtek USB'], ['network-connectx3', 'Mellanox']],
  router: [['router-glinet', 'GL.iNet'], ['router-openwrt', 'OpenWrt'], ['router-pi4', 'Raspberry Pi']],
  memory: [['memory-ddr4', 'DDR4'], ['memory-ddr5', 'DDR5'], ['memory-programmer', 'Programmer workflow']],
  display: [['display-windows', 'Windows override'], ['display-drhdmi8k', 'Dr HDMI'], ['display-displayport', 'DisplayPort scope']],
}
function examples(id: string): GuideLink[] {
  const guide = guides.find((item) => item.id === id)!
  return exampleIds[id].map(([methodId, label]) => ({ label, href: guide.methods.find((method) => method.id === methodId)!.href }))
}
</script>

<template>
  <div class="wiki-front-content">
    <div class="wiki-front-start" aria-label="Ways to begin">
      <section>
        <WikiIcon name="workflow" />
        <a href="/start.html">Start with the guide map</a>
        <p>Follow the overview from preparation to your component and its checks.</p>
      </section>
      <section>
        <WikiIcon name="overview" />
        <a href="#components">Choose your hardware</a>
        <p>Use a decision chart or jump straight to a method you know.</p>
      </section>
      <section>
        <WikiIcon name="inspect" />
        <a href="/guides/getting-started/getting-started.html#hwidcheckerexe">Inspect with HWIDChecker</a>
        <p>Collect a baseline and compare the identifiers it can read.</p>
      </section>
    </div>

    <section aria-labelledby="components">
      <div class="wiki-front-heading">
        <h2 id="components">Find your component</h2>
        <a href="/devices.html">All topics and methods</a>
      </div>
      <div class="wiki-front-grid">
        <article v-for="guide in guides" :key="guide.id" class="wiki-front-component">
          <header>
            <span class="wiki-icon-tile"><WikiIcon :name="guide.id" /></span>
            <div>
              <span class="wiki-front-group">{{ guide.group }}</span>
              <h3><a :href="hubHref(guide.id)">{{ guide.shortTitle }}</a></h3>
            </div>
            <span class="wiki-front-count">{{ guide.methods.length }} topics</span>
          </header>
          <p>{{ descriptions[guide.id] }}</p>
          <ul class="wiki-front-examples" :aria-label="`${guide.shortTitle} examples`">
            <li v-for="example in examples(guide.id)" :key="example.href">
              <a :href="example.href">{{ example.label }}</a>
            </li>
          </ul>
          <footer>
            <a :href="hubHref(guide.id)" class="wiki-open-chart">Open decision chart</a>
            <a :href="guide.href">Full guide</a>
          </footer>
        </article>
      </div>
    </section>
  </div>
</template>
