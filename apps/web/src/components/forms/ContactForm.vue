<script setup lang="ts">
import { reactive, ref } from 'vue'
import { newIdempotencyKey, submitContact } from '@/lib/api'
import { buildContactRequest } from '@/lib/submissions'

const state = ref<'idle' | 'submitting' | 'success' | 'error'>('idle')
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
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : 'The request could not be submitted.'
    state.value = 'error'
  }
}
</script>

<template>
  <div v-if="state === 'success'" class="success-panel" role="status">
    <p class="eyebrow">Message received</p><h2>Thank you for contacting us.</h2>
    <p>Your reference is <strong>{{ receipt }}</strong>. Keep it for any follow-up.</p>
  </div>
  <form v-else class="form-card" @submit.prevent="submit">
    <div class="field-grid">
      <label><span>Topic</span><select v-model="form.topic" required><option value="sales">Sales inquiry</option><option value="technical">Technical support</option><option value="existingOrder">Existing order</option><option value="careers">Careers</option><option value="supplier">Supplier</option><option value="general">General</option></select></label>
      <label><span>Company <small>(optional)</small></span><input v-model.trim="form.company" autocomplete="organization"></label>
      <label><span>Your name</span><input v-model.trim="form.contactName" autocomplete="name" required></label>
      <label><span>Business email</span><input v-model.trim="form.businessEmail" type="email" autocomplete="email" required></label>
      <label><span>Phone <small>(optional)</small></span><input v-model.trim="form.phone" type="tel" autocomplete="tel"></label>
      <label class="wide"><span>How can we help?</span><textarea v-model.trim="form.message" rows="6" required></textarea></label>
      <label class="honeypot" aria-hidden="true"><span>Website</span><input v-model="form.website" tabindex="-1" autocomplete="off"></label>
      <label class="checkbox wide"><input v-model="form.consent" type="checkbox" required><span>I agree that AIRTEKPOWER may use these details to respond to this inquiry. See <a href="/en/privacy">Privacy</a>.</span></label>
    </div>
    <p v-if="error" class="form-error" role="alert">{{ error }}</p>
    <button class="button" type="submit" :disabled="state === 'submitting'">{{ state === 'submitting' ? 'Sending…' : 'Send message' }}</button>
  </form>
</template>
