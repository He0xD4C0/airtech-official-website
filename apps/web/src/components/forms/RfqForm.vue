<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import type { ProductContext } from '@airtek/contracts'
import { newIdempotencyKey, PublicApiError, submitRfq } from '@/lib/api'
import { buildRfqRequest, hasCompleteProductContext, parseRfqSession, serializeRfqSession } from '@/lib/submissions'
import { trackAnalyticsEvent } from '@/lib/analytics'
import type { RfqType } from '@/types/content'

const props = defineProps<{ kind: RfqType; productContext?: ProductContext }>()
const step = ref(1)
const state = ref<'idle' | 'submitting' | 'success' | 'error'>('idle')
const error = ref('')
const fieldErrors = ref<Record<string, string>>({})
const receipt = ref('')
const contextKey = computed(() => `airtek.public.rfq-context.${props.kind}.v2`)

const context = reactive({
  application: '', existingModel: '', quantity: '', airflow: '', airflowUnit: 'm3/h', pressure: '', pressureUnit: 'Pa',
  voltage: '', frequency: '', environment: '', projectStage: '', priority: 'efficiency',
  maximumDiameter: '', requiredCertifications: '', control: '', projectScale: '', schedule: '',
  engineeringNeeds: '', installationConstraints: '', replacementGoal: '',
})
const contact = reactive({ company: '', contactName: '', businessEmail: '', phone: '', country: '', message: '', consent: false, website: '' })

const title = computed(() => ({
  product: 'Identify the product context', selection: 'Define the duty point', project: 'Describe the project', replacement: 'Describe the existing installation',
})[props.kind])
const needsDutyPoint = computed(() => props.kind === 'selection' || props.kind === 'replacement')
const productBlocked = computed(() => props.kind === 'product' && !hasCompleteProductContext(props.productContext))

onMounted(() => {
  try {
    const saved = window.sessionStorage.getItem(contextKey.value)
    const restored = parseRfqSession(saved)
    if (restored) Object.assign(context, restored)
  } catch { /* Ignore invalid session data. */ }
  void trackAnalyticsEvent('rfqStarted', {
    journey: props.kind,
    productId: props.productContext?.productId,
    productRevision: props.productContext?.publishedRevision,
  })
})
watch(context, (value) => {
  if (typeof window !== 'undefined') window.sessionStorage.setItem(contextKey.value, serializeRfqSession({ ...value }))
}, { deep: true })

function next() {
  if (productBlocked.value) {
    error.value = 'Choose a published product record before starting a Product RFQ. If no validated record is available, use the Fan Selection RFQ.'
    void trackAnalyticsEvent('rfqValidationError', { journey: props.kind, step: step.value, fieldName: 'productContext', errorCode: 'publishedContextRequired' })
    return
  }
  if (step.value < 3) {
    void trackAnalyticsEvent('rfqStepCompleted', { journey: props.kind, step: step.value })
    step.value += 1
  }
  window.scrollTo({ top: 220, behavior: 'smooth' })
}
function previous() {
  if (step.value > 1) step.value -= 1
}

async function submit() {
  if (contact.website) return
  state.value = 'submitting'
  error.value = ''
  fieldErrors.value = {}
  try {
    const request = buildRfqRequest(props.kind, context, contact, props.productContext)
    const result = await submitRfq(request, newIdempotencyKey())
    receipt.value = result.reference
    state.value = 'success'
    window.sessionStorage.removeItem(contextKey.value)
    void trackAnalyticsEvent('rfqSubmitted', { journey: props.kind })
  } catch (cause) {
    if (cause instanceof PublicApiError && cause.errors) {
      fieldErrors.value = Object.fromEntries(
        Object.entries(cause.errors).map(([field, messages]) => [field, messages.join(' ')]),
      )
      error.value = 'Check the highlighted fields and submit again.'
    } else {
      error.value = cause instanceof Error ? cause.message : 'The request could not be submitted.'
    }
    state.value = 'error'
    void trackAnalyticsEvent('rfqSubmitFailed', { journey: props.kind, errorCode: 'apiRejected' })
  }
}

function fieldError(path: string): string | undefined {
  return fieldErrors.value[path]
}
</script>

<template>
  <section class="section shell rfq-layout">
    <aside class="rfq-progress" aria-label="Request progress">
      <p>Step {{ step }} of 3</p>
      <ol><li :class="{ active: step >= 1 }">Context</li><li :class="{ active: step >= 2 }">Contact</li><li :class="{ active: step >= 3 }">Review</li></ol>
      <small>No document upload is requested. Technical files can be exchanged through an agreed channel after follow-up.</small>
    </aside>

    <div v-if="state === 'success'" class="success-panel" role="status">
      <p class="eyebrow">Request received</p><h2>Your inquiry has been recorded.</h2>
      <p>Reference: <strong>{{ receipt }}</strong>. This acknowledgement does not promise suitability, price, availability or delivery timing.</p>
      <a class="button" href="/en">Return home</a>
    </div>

    <form v-else class="form-card" @submit.prevent="step < 3 ? next() : submit()">
      <div v-if="step === 1">
        <p class="eyebrow">Inquiry context</p><h2>{{ title }}</h2>
        <div class="field-grid">
          <div v-if="kind === 'product'" class="wide review-panel">
            <template v-if="!productBlocked && productContext">
              <p class="eyebrow">Published product context</p>
              <dl>
                <div><dt>Stable ID</dt><dd>{{ productContext.stableId }}</dd></div>
                <div><dt>Model</dt><dd>{{ productContext.model }}</dd></div>
                <div><dt>Published revision</dt><dd>{{ productContext.publishedRevision }}</dd></div>
              </dl>
              <p class="review-note">This immutable context came from the published product projection and cannot be edited in the RFQ.</p>
            </template>
            <template v-else>
              <p class="eyebrow">Published context required</p>
              <h3>A Product RFQ cannot be submitted from this generic entry.</h3>
              <p>Open a published model and use its Request a Quote action. If no validated model is available, send the duty point through the Fan Selection RFQ.</p>
              <div class="button-row"><a class="button secondary" href="/en/products">Browse products</a><a class="button" href="/en/request-a-quote/selection">Use Selection RFQ</a></div>
            </template>
          </div>
          <label class="wide"><span>Application or system</span><input v-model.trim="context.application" required placeholder="Describe where airflow is needed"></label>
          <label v-if="kind === 'replacement'"><span>Existing model/nameplate text</span><input v-model.trim="context.existingModel" required></label>
          <label><span>Estimated quantity <small>(optional)</small></span><input v-model.trim="context.quantity" inputmode="numeric"></label>
          <template v-if="needsDutyPoint">
            <label><span>Required airflow</span><div class="input-unit"><input v-model="context.airflow" type="number" min="0" step="any" required :aria-invalid="Boolean(fieldError('context.dutyPoint.airflow'))"><select v-model="context.airflowUnit"><option value="m3/h">m³/h</option><option value="CFM">CFM</option></select></div><small v-if="fieldError('context.dutyPoint.airflow')" class="form-error">{{ fieldError('context.dutyPoint.airflow') }}</small></label>
            <label><span>Required pressure</span><div class="input-unit"><input v-model="context.pressure" type="number" min="0" step="any" required :aria-invalid="Boolean(fieldError('context.dutyPoint.pressure'))"><select v-model="context.pressureUnit"><option value="Pa">Pa</option><option value="kPa">kPa</option><option value="inH2O">inH₂O</option></select></div><small v-if="fieldError('context.dutyPoint.pressure')" class="form-error">{{ fieldError('context.dutyPoint.pressure') }}</small></label>
          </template>
          <template v-if="kind === 'selection'">
            <label><span>Maximum diameter mm <small>(optional)</small></span><input v-model="context.maximumDiameter" type="number" min="0" step="any" :aria-invalid="Boolean(fieldError('context.maximumDiameterMm'))"><small v-if="fieldError('context.maximumDiameterMm')" class="form-error">{{ fieldError('context.maximumDiameterMm') }}</small></label>
            <label><span>Control method <small>(optional)</small></span><input v-model.trim="context.control" :aria-invalid="Boolean(fieldError('context.control'))"><small v-if="fieldError('context.control')" class="form-error">{{ fieldError('context.control') }}</small></label>
            <label class="wide"><span>Required certifications <small>(optional, comma separated)</small></span><input v-model.trim="context.requiredCertifications" :aria-invalid="Boolean(fieldError('context.requiredCertifications'))"><small v-if="fieldError('context.requiredCertifications')" class="form-error">{{ fieldError('context.requiredCertifications') }}</small></label>
          </template>
          <label><span>Voltage <small>(optional)</small></span><input v-model.trim="context.voltage" inputmode="decimal" placeholder="Value only"></label>
          <label><span>Frequency <small>(optional)</small></span><select v-model="context.frequency"><option value="">Not specified</option><option value="50">50 Hz</option><option value="60">60 Hz</option></select></label>
          <label class="wide"><span>Environment and constraints <small>(optional)</small></span><textarea v-model.trim="context.environment" rows="4" placeholder="Installation envelope, ambient conditions, control or required compliance"></textarea></label>
          <label v-if="kind === 'project'" class="wide"><span>Project stage</span><select v-model="context.projectStage" required><option value="">Select stage</option><option>Concept</option><option>Engineering</option><option>Prototype</option><option>Production planning</option></select></label>
          <template v-if="kind === 'project'">
            <label><span>Project scale <small>(optional)</small></span><input v-model.trim="context.projectScale" :aria-invalid="Boolean(fieldError('context.projectScale'))"><small v-if="fieldError('context.projectScale')" class="form-error">{{ fieldError('context.projectScale') }}</small></label>
            <label><span>Target schedule <small>(optional)</small></span><input v-model.trim="context.schedule" :aria-invalid="Boolean(fieldError('context.schedule'))"><small v-if="fieldError('context.schedule')" class="form-error">{{ fieldError('context.schedule') }}</small></label>
            <label class="wide"><span>Engineering needs <small>(optional)</small></span><textarea v-model.trim="context.engineeringNeeds" rows="4" :aria-invalid="Boolean(fieldError('context.engineeringNeeds'))"></textarea><small v-if="fieldError('context.engineeringNeeds')" class="form-error">{{ fieldError('context.engineeringNeeds') }}</small></label>
          </template>
          <template v-if="kind === 'replacement'">
            <label class="wide"><span>Installation constraints <small>(optional)</small></span><textarea v-model.trim="context.installationConstraints" rows="4" :aria-invalid="Boolean(fieldError('context.installationConstraints'))"></textarea><small v-if="fieldError('context.installationConstraints')" class="form-error">{{ fieldError('context.installationConstraints') }}</small></label>
            <label class="wide"><span>Replacement goal <small>(optional)</small></span><textarea v-model.trim="context.replacementGoal" rows="3" :aria-invalid="Boolean(fieldError('context.replacementGoal'))"></textarea><small v-if="fieldError('context.replacementGoal')" class="form-error">{{ fieldError('context.replacementGoal') }}</small></label>
          </template>
        </div>
      </div>

      <div v-else-if="step === 2">
        <p class="eyebrow">Contact details</p><h2>Who should we follow up with?</h2>
        <div class="field-grid">
          <label><span>Company</span><input v-model.trim="contact.company" autocomplete="organization" required></label>
          <label><span>Your name</span><input v-model.trim="contact.contactName" autocomplete="name" required></label>
          <label><span>Business email</span><input v-model.trim="contact.businessEmail" type="email" autocomplete="email" required></label>
          <label><span>Phone <small>(optional)</small></span><input v-model.trim="contact.phone" type="tel" autocomplete="tel" :aria-invalid="Boolean(fieldError('contact.phone'))"><small v-if="fieldError('contact.phone')" class="form-error">{{ fieldError('contact.phone') }}</small></label>
          <label><span>Country or region</span><input v-model.trim="contact.country" autocomplete="country-name" required></label>
          <label class="wide"><span>Additional context <small>(optional)</small></span><textarea v-model.trim="contact.message" rows="5"></textarea></label>
          <label class="honeypot" aria-hidden="true"><span>Website</span><input v-model="contact.website" tabindex="-1" autocomplete="off"></label>
          <label class="checkbox wide"><input v-model="contact.consent" type="checkbox" required><span>I agree that AIRTEKPOWER may use these details to evaluate and respond to this inquiry. See <a href="/en/privacy">Privacy</a>.</span></label>
        </div>
      </div>

      <div v-else class="review-panel">
        <p class="eyebrow">Review</p><h2>Check your request</h2>
        <dl>
          <div><dt>Inquiry</dt><dd>{{ kind }}</dd></div><div><dt>Application</dt><dd>{{ context.application }}</dd></div>
          <div v-if="context.existingModel"><dt>Known/existing model</dt><dd>{{ context.existingModel }}</dd></div>
          <div v-if="needsDutyPoint"><dt>Duty point</dt><dd>{{ context.airflow }} {{ context.airflowUnit }} at {{ context.pressure }} {{ context.pressureUnit }}</dd></div>
          <div><dt>Company</dt><dd>{{ contact.company }}</dd></div><div><dt>Contact</dt><dd>{{ contact.contactName }} · {{ contact.businessEmail }}<template v-if="contact.phone"> · {{ contact.phone }}</template></dd></div>
        </dl>
        <p class="review-note">Submitting creates a secure inquiry record. It does not automatically confirm product suitability or commercial terms.</p>
      </div>

      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div class="button-row form-actions">
        <button v-if="step > 1" class="button secondary" type="button" @click="previous">Back</button>
        <button class="button" type="submit" :disabled="state === 'submitting' || productBlocked">{{ step < 3 ? 'Continue' : state === 'submitting' ? 'Submitting…' : 'Submit request' }}</button>
      </div>
    </form>
  </section>
</template>
