<script setup lang="ts">
import horizontalLogo from '@airtek/ui/assets/brand/airtek-standard-lockup-horizontal.webp'
import verticalLogo from '@airtek/ui/assets/brand/airtek-standard-lockup-vertical.webp'

withDefaults(defineProps<{ compact?: boolean; vertical?: boolean }>(), {
  compact: false,
  vertical: false,
})
</script>

<template>
  <img
    class="brand-mark"
    :class="{ 'brand-mark--compact': compact, 'brand-mark--vertical': vertical }"
    :src="vertical ? verticalLogo : horizontalLogo"
    alt="AIRTEKPOWER"
  />
</template>

<style scoped>
@layer components {
  .brand-mark {
    display: block;
    width: min(13rem, 100%);
    height: auto;
    border-radius: 0.25rem;
  }

  .brand-mark--compact { width: min(10rem, 100%); }
  .brand-mark--vertical { width: min(15rem, 70vw); }
}
</style>
