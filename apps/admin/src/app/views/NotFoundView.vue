<script setup lang="ts">
import { ArrowLeft, SearchX } from 'lucide-vue-next'
</script>

<template>
  <section class="centered-state">
    <div class="centered-state__icon"><SearchX :size="30" /></div>
    <p class="eyebrow">404</p>
    <h1>后台页面不存在</h1>
    <p>此地址不属于管理平台，或当前构建未包含该模块。</p>
    <RouterLink class="button button--primary" to="/"><ArrowLeft :size="16" />返回工作台</RouterLink>
  </section>
</template>
