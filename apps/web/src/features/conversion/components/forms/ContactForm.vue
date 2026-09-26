<script setup lang="ts">
import { reactive, ref } from 'vue'
import { newIdempotencyKey, submitContact } from '@/shared/lib/api'
import { buildContactRequest } from '@/features/conversion/lib/submissions'
import { useI18n } from 'vue-i18n'

const state = ref<'idle' | 'submitting' | 'success' | 'error'>('idle')
const { locale, t } = useI18n({ useScope: 'global' })
const error = ref('')
const receipt = ref('')
const form = reactive({ topic: 'general', company: '', contactName: '', businessEmail: '', phone: '', message: '', consent: false, website: '' })

async function submit() {
  if (form.website) return
  state.value = 'submitting'
  error.value = ''
  try {
    const result = await submitContact(buildContactRequest(form), newIdempotencyKey())
    receipt.value = result.reference
    state.value = 'success'
  } catch {
    error.value = t('errors.submit')
    state.value = 'error'
  }
}
</script>

<template>
  <div v-if="state === 'success'" class="success-panel" role="status" :lang="locale">
    <p class="eyebrow">{{ t('forms.messageReceived') }}</p><h2>{{ t('forms.thankYou') }}</h2>
    <p>{{ t('forms.reference', { reference: receipt }) }}</p>
  </div>
  <form v-else class="form-card" :lang="locale" @submit.prevent="submit">
    <div class="field-grid">
      <label><span>{{ t('forms.topic') }}</span><select v-model="form.topic" required><option value="sales">{{ t('forms.topics.sales') }}</option><option value="technical">{{ t('forms.topics.technical') }}</option><option value="existingOrder">{{ t('forms.topics.existingOrder') }}</option><option value="careers">{{ t('forms.topics.careers') }}</option><option value="supplier">{{ t('forms.topics.supplier') }}</option><option value="general">{{ t('forms.topics.general') }}</option></select></label>
      <label><span>{{ t('forms.company') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="form.company" autocomplete="organization"></label>
      <label><span>{{ t('forms.name') }}</span><input v-model.trim="form.contactName" autocomplete="name" required></label>
      <label><span>{{ t('forms.email') }}</span><input v-model.trim="form.businessEmail" type="email" autocomplete="email" required></label>
      <label><span>{{ t('forms.phone') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="form.phone" type="tel" autocomplete="tel"></label>
      <label class="wide"><span>{{ t('forms.help') }}</span><textarea v-model.trim="form.message" rows="6" required></textarea></label>
      <label class="honeypot" aria-hidden="true"><span>{{ t('forms.website') }}</span><input v-model="form.website" tabindex="-1" autocomplete="off"></label>
      <label class="checkbox wide"><input v-model="form.consent" type="checkbox" required><span>{{ t('forms.consent') }} <a href="/en/privacy">{{ t('forms.privacy') }}</a>.</span></label>
    </div>
    <p v-if="error" class="form-error" role="alert">{{ error }}</p>
    <button class="button" type="submit" :disabled="state === 'submitting'">{{ state === 'submitting' ? t('forms.sending') : t('forms.send') }}</button>
  </form>
</template>
