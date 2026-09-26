<script setup lang="ts">
import { computed } from 'vue'
import type { Permission } from '@/shared/types/domain'
import { useAuthStore } from '@/shared/stores/auth'

const props = defineProps<{ permission: Permission }>()
const auth = useAuthStore()
const allowed = computed(() => auth.hasPermission(props.permission))
</script>

<template>
  <slot v-if="allowed" />
  <slot v-else name="fallback" />
</template>
