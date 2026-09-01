<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(defineProps<{
  points?: Array<{ airflow: number; pressure: number }>
  airflowUnit?: string
  pressureUnit?: string
  conditions?: string
}>(), {
  points: () => [],
})

const scaledPoints = computed(() => {
  if (!props.points.length) return ''
  const maximumAirflow = Math.max(...props.points.map((point) => point.airflow), 1)
  const maximumPressure = Math.max(...props.points.map((point) => point.pressure), 1)
  return props.points.map((point) => {
    const x = 58 + (point.airflow / maximumAirflow) * 540
    const y = 298 - (point.pressure / maximumPressure) * 270
    return `${x.toFixed(2)},${y.toFixed(2)}`
  }).join(' ')
})
</script>

<template>
  <div class="pq-panel">
    <div class="pq-chart" role="img" :aria-label="points.length ? 'Published product airflow and pressure curve' : 'No verified airflow and pressure curve is available'">
      <svg viewBox="0 0 640 360" aria-hidden="true" focusable="false">
        <defs>
          <pattern id="pq-grid" width="64" height="54" patternUnits="userSpaceOnUse">
            <path d="M 64 0 L 0 0 0 54" fill="none" stroke="currentColor" stroke-opacity=".12" />
          </pattern>
        </defs>
        <rect x="58" y="28" width="540" height="270" fill="url(#pq-grid)" />
        <path d="M58 28V298H598" fill="none" stroke="currentColor" stroke-width="2" />
        <polyline
          v-if="points.length"
          :points="scaledPoints"
          fill="none"
          stroke="var(--airtek-blue)"
          stroke-width="5"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        <g v-else class="pq-empty-label">
          <rect x="147" y="124" width="364" height="72" rx="12" fill="white" />
          <text x="329" y="154" text-anchor="middle">Verified curve pending</text>
          <text x="329" y="177" text-anchor="middle" class="small">No curve points have been assumed</text>
        </g>
        <text x="329" y="337" text-anchor="middle" class="axis-label">Airflow {{ airflowUnit ? `(${airflowUnit})` : '(unit pending)' }}</text>
        <text x="20" y="169" text-anchor="middle" class="axis-label" transform="rotate(-90 20 169)">Pressure {{ pressureUnit ? `(${pressureUnit})` : '(unit pending)' }}</text>
      </svg>
    </div>
    <p class="chart-condition">{{ conditions || 'Operating conditions and test method pending verification.' }}</p>
    <div class="table-scroll">
      <table>
        <caption>Equivalent performance curve data</caption>
        <thead><tr><th scope="col">Airflow</th><th scope="col">Pressure</th><th scope="col">State</th></tr></thead>
        <tbody>
          <tr v-if="!points.length"><td>—</td><td>—</td><td>Pending verification</td></tr>
          <tr v-for="(point, index) in points" :key="index">
            <td>{{ point.airflow }} {{ airflowUnit }}</td><td>{{ point.pressure }} {{ pressureUnit }}</td><td>Published</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
