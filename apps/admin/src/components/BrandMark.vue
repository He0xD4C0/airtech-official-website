<script setup lang="ts">
withDefaults(defineProps<{ compact?: boolean; inverse?: boolean }>(), {
  compact: false,
  inverse: false,
})
</script>

<template>
  <div class="brand-mark" :class="{ 'brand-mark--compact': compact, 'brand-mark--inverse': inverse }" aria-label="AIRTEKPOWER">
    <span class="brand-mark__name">AIRTEK<span>POWER</span></span>
  </div>
</template>

<style scoped>
.brand-mark {
  display: inline-flex;
  align-items: center;
  color: var(--airtek-ink);
}

.brand-mark__name {
  font-size: 0.98rem;
  font-weight: 800;
  letter-spacing: 0.055em;
}

.brand-mark__name span { color: var(--airtek-blue); }
.brand-mark--inverse { color: white; }
.brand-mark--inverse .brand-mark__name span { color: #6cd59a; }
.brand-mark--compact .brand-mark__name { font-size: 0.85rem; }
</style>
