<script setup lang="ts">
import { computed } from 'vue'
import type { RfqContextSnapshot } from '@airtek/contracts'
const props = defineProps<{ snapshot: RfqContextSnapshot }>()
const labels: Record<string, string> = {
  application: '应用场景', quantity: '数量', environment: '使用环境', priority: '优先级',
  additionalMessage: '补充说明', maximumDiameterMm: '最大直径（mm）', ambientTemperatureC: '环境温度（°C）',
  preferredFamily: '产品家族', motorTechnology: '马达技术', requiredCertifications: '认证要求', control: '控制方式',
  projectStage: '项目阶段', projectScale: '项目规模', schedule: '计划时间', engineeringNeeds: '工程需求',
  existingModel: '原型号', installationConstraints: '安装限制', replacementGoal: '替换目标',
  airflow: '风量', airflowUnit: '风量单位', pressure: '压力', pressureUnit: '压力单位',
  voltage: '电压', frequencyHz: '频率（Hz）',
}
const rows = computed(() => Object.entries(props.snapshot.context).flatMap(([key, value]) => {
  if (value === undefined || value === null) return []
  if (key === 'dutyPoint' || key === 'electrical') return Object.entries(value).map(([field, entry]) => ({ label: labels[field] ?? field, value: entry }))
  return [{ label: labels[key] ?? key, value: Array.isArray(value) ? value.join(', ') : value }]
}))
</script>

<template>
  <section class="detail-section" aria-label="RFQ 工程需求">
    <h3>提交时的工程需求 · {{ snapshot.journey }}</h3>
    <p>以下是客户提交的需求，不代表已验证的产品适用性或报价。</p>
    <dl><div v-for="row in rows" :key="row.label"><dt>{{ row.label }}</dt><dd>{{ row.value }}</dd></div></dl>
  </section>
</template>
