<script setup lang="ts">
import { ArrowLeft, ShieldX } from 'lucide-vue-next'
import { useRoute, useRouter } from 'vue-router'

const route = useRoute()
const router = useRouter()
</script>

<template>
  <main class="auth-status-page" aria-labelledby="forbidden-title">
    <section class="panel data-state">
      <ShieldX :size="34" />
      <p class="eyebrow">FORBIDDEN</p>
      <h1 id="forbidden-title">没有访问权限</h1>
      <p>当前会话缺少 <code>{{ String(route.query.permission ?? 'required permission') }}</code>。权限由 Rust API 强制执行。</p>
      <button class="button button--secondary" type="button" @click="router.back()">
        <ArrowLeft :size="16" />返回上一页
      </button>
    </section>
  </main>
</template>
