<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue'
import type { ProductContext } from '@airtek/contracts'
import { newIdempotencyKey, PublicApiError, submitRfq } from '@/shared/lib/api'
import { buildRfqRequest, hasCompleteProductContext, parseRfqSession, serializeRfqSession } from '@/features/conversion/lib/submissions'
import { trackAnalyticsEvent } from '@/features/analytics'
import { FormValidationError } from '@/features/conversion/lib/formValues'
import { rfqFieldPath, rfqReviewFields } from '@/features/conversion/lib/rfqFields'
import type { RfqContextValues } from '@/features/conversion/lib/submissions'
import type { RfqType } from '@/shared/types/content'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ kind: RfqType; productContext?: ProductContext }>()
const { locale, t } = useI18n({ useScope: 'global' })
const step = ref(1)
const state = ref<'idle' | 'submitting' | 'success' | 'error'>('idle')
const error = ref('')
const fieldErrors = ref<Record<string, string>>({})
const receipt = ref('')
let submittedBody = ''
let submissionKey = ''
const reviewFields = computed(() => rfqReviewFields(props.kind, context))
const contextKey = computed(() => `airtek.public.rfq-context.${props.kind}.v3`)

const context = reactive<RfqContextValues>({
  ambientTemperature: '', preferredFamily: '', motorTechnology: '',
  application: '', existingModel: '', quantity: '', airflow: '', airflowUnit: 'm3/h', pressure: '', pressureUnit: 'Pa',
  voltage: '', frequency: '', environment: '', projectStage: '', priority: 'efficiency',
  maximumDiameter: '', requiredCertifications: '', control: '', projectScale: '', schedule: '',
  engineeringNeeds: '', installationConstraints: '', replacementGoal: '',
})
const contact = reactive({ company: '', contactName: '', businessEmail: '', phone: '', country: '', message: '', consent: false, website: '' })

const title = computed(() => t(`rfq.titles.${props.kind}`))
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
  try { window.sessionStorage.setItem(contextKey.value, serializeRfqSession({ ...value })) } catch { /* Draft storage is optional. */ }
}, { deep: true })

function next() {
  if (productBlocked.value) {
    error.value = t('rfq.blockedError')
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
    const body = JSON.stringify(request)
    if (body !== submittedBody) { submittedBody = body; submissionKey = newIdempotencyKey() }
    const result = await submitRfq(request, submissionKey)
    receipt.value = result.reference
    state.value = 'success'
    try { window.sessionStorage.removeItem(contextKey.value) } catch { /* Submission already succeeded. */ }
    void trackAnalyticsEvent('rfqSubmitted', { journey: props.kind })
  } catch (cause) {
    if ((cause instanceof PublicApiError || cause instanceof FormValidationError) && cause.errors) {
      fieldErrors.value = Object.fromEntries(
        Object.keys(cause.errors).map((field) => [field, t('forms.invalidField')]),
      )
      error.value = t('forms.invalidFields')
      await showField(Object.keys(fieldErrors.value)[0] ?? 'context')
    } else {
      error.value = t('errors.submit')
    }
    state.value = 'error'
    void trackAnalyticsEvent('rfqSubmitFailed', { journey: props.kind, errorCode: 'apiRejected' })
  }
}

async function showField(path: string) {
  step.value = path.startsWith('contact.') || path === 'consent' || path === 'context.additionalMessage' ? 2 : 1
  await nextTick()
  const target = document.getElementById('rfq-' + path)
  target?.focus()
  target?.scrollIntoView({ block: 'center', behavior: 'smooth' })
}

function fieldError(path: string): string | undefined {
  return fieldErrors.value[path]
}
</script>

<template>
  <section class="section shell rfq-layout" :lang="locale">
    <aside class="rfq-progress" :aria-label="t('rfq.progress')">
      <p>{{ t('rfq.step', { step }) }}</p>
      <ol><li :class="{ active: step >= 1 }">{{ t('rfq.context') }}</li><li :class="{ active: step >= 2 }">{{ t('rfq.contact') }}</li><li :class="{ active: step >= 3 }">{{ t('rfq.review') }}</li></ol>
      <small>{{ t('rfq.uploadNote') }}</small>
    </aside>

    <div v-if="state === 'success'" class="success-panel" role="status">
      <p class="eyebrow">{{ t('rfq.received') }}</p><h2>{{ t('rfq.recorded') }}</h2>
      <p>{{ t('rfq.acknowledgement', { reference: receipt }) }}</p>
      <a class="button" href="/en">{{ t('rfq.returnHome') }}</a>
    </div>

    <form v-else class="form-card" @submit.prevent="step < 3 ? next() : submit()">
      <div v-if="step === 1">
        <p class="eyebrow">{{ t('rfq.inquiryContext') }}</p><h2>{{ title }}</h2>
        <div class="field-grid">
          <div v-if="kind === 'product'" class="wide review-panel">
            <template v-if="!productBlocked && productContext">
              <p class="eyebrow">{{ t('rfq.productContext') }}</p>
              <dl>
                <div><dt>{{ t('compare.stableId') }}</dt><dd lang="en">{{ productContext.stableId }}</dd></div>
                <div><dt>{{ t('compare.model') }}</dt><dd lang="en">{{ productContext.model }}</dd></div>
                <div><dt>{{ t('rfq.revision') }}</dt><dd>{{ productContext.publishedRevision }}</dd></div>
              </dl>
              <p class="review-note">{{ t('rfq.immutableContext') }}</p>
            </template>
            <template v-else>
              <p class="eyebrow">{{ t('rfq.contextRequired') }}</p>
              <h3>{{ t('rfq.genericBlocked') }}</h3>
              <p>{{ t('rfq.blockedHelp') }}</p>
              <div class="button-row"><a class="button secondary" href="/en/products">{{ t('rfq.browse') }}</a><a class="button" href="/en/request-a-quote/selection">{{ t('rfq.useSelection') }}</a></div>
            </template>
          </div>
          <label class="wide"><span>{{ t('rfq.application') }}</span><input v-model.trim="context.application" :id="'rfq-' + rfqFieldPath('context.application')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.application')))" :aria-describedby="fieldError(rfqFieldPath('context.application')) ? 'error-' + rfqFieldPath('context.application') : undefined" required :placeholder="t('rfq.applicationPlaceholder')"></label>
          <label v-if="kind === 'replacement'"><span>{{ t('rfq.existingModel') }}</span><input v-model.trim="context.existingModel" :id="'rfq-' + rfqFieldPath('context.existingModel')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.existingModel')))" :aria-describedby="fieldError(rfqFieldPath('context.existingModel')) ? 'error-' + rfqFieldPath('context.existingModel') : undefined" required></label>
          <label><span>{{ t('rfq.quantity') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="context.quantity" :id="'rfq-' + rfqFieldPath('context.quantity')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.quantity')))" :aria-describedby="fieldError(rfqFieldPath('context.quantity')) ? 'error-' + rfqFieldPath('context.quantity') : undefined" inputmode="numeric"></label>
          <template v-if="needsDutyPoint">
            <label><span>{{ t('rfq.requiredAirflow') }}</span><div class="input-unit"><input v-model="context.airflow" :id="'rfq-' + rfqFieldPath('context.airflow')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.airflow')))" :aria-describedby="fieldError(rfqFieldPath('context.airflow')) ? 'error-' + rfqFieldPath('context.airflow') : undefined" type="number" min="0" step="any" required><select v-model="context.airflowUnit" :id="'rfq-' + rfqFieldPath('context.airflowUnit')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.airflowUnit')))" :aria-describedby="fieldError(rfqFieldPath('context.airflowUnit')) ? 'error-' + rfqFieldPath('context.airflowUnit') : undefined"><option value="m3/h">m³/h</option><option value="CFM">CFM</option></select></div><small v-if="fieldError('context.dutyPoint.airflow')" class="form-error">{{ fieldError('context.dutyPoint.airflow') }}</small></label>
            <label><span>{{ t('rfq.requiredPressure') }}</span><div class="input-unit"><input v-model="context.pressure" :id="'rfq-' + rfqFieldPath('context.pressure')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.pressure')))" :aria-describedby="fieldError(rfqFieldPath('context.pressure')) ? 'error-' + rfqFieldPath('context.pressure') : undefined" type="number" min="0" step="any" required><select v-model="context.pressureUnit" :id="'rfq-' + rfqFieldPath('context.pressureUnit')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.pressureUnit')))" :aria-describedby="fieldError(rfqFieldPath('context.pressureUnit')) ? 'error-' + rfqFieldPath('context.pressureUnit') : undefined"><option value="Pa">Pa</option><option value="kPa">kPa</option><option value="inH2O">inH₂O</option></select></div><small v-if="fieldError('context.dutyPoint.pressure')" class="form-error">{{ fieldError('context.dutyPoint.pressure') }}</small></label>
          </template>
          <template v-if="kind === 'selection'">
            <label><span>{{ t('rfq.ambient') }} <small>({{ t('common.optional') }})</small></span><input v-model="context.ambientTemperature" :id="'rfq-' + rfqFieldPath('context.ambientTemperature')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.ambientTemperature')))" :aria-describedby="fieldError(rfqFieldPath('context.ambientTemperature')) ? 'error-' + rfqFieldPath('context.ambientTemperature') : undefined" type="number" min="-273.15" max="1000" step="any"></label>
            <label><span>{{ t('rfq.fanFamily') }} <small>({{ t('common.optional') }})</small></span><select v-model="context.preferredFamily" :id="'rfq-' + rfqFieldPath('context.preferredFamily')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.preferredFamily')))" :aria-describedby="fieldError(rfqFieldPath('context.preferredFamily')) ? 'error-' + rfqFieldPath('context.preferredFamily') : undefined"><option value="">{{ t('rfq.notSpecified') }}</option><option value="axial">Axial</option><option value="centrifugal">Centrifugal</option><option value="crossFlow">Cross-flow</option><option value="inlineDuct">Inline duct</option><option value="motors">Motors</option></select></label>
            <label><span>{{ t('rfq.motorTechnology') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="context.motorTechnology" :id="'rfq-' + rfqFieldPath('context.motorTechnology')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.motorTechnology')))" :aria-describedby="fieldError(rfqFieldPath('context.motorTechnology')) ? 'error-' + rfqFieldPath('context.motorTechnology') : undefined"></label>
            <label><span>{{ t('rfq.maximumDiameter') }} <small>({{ t('common.optional') }})</small></span><input v-model="context.maximumDiameter" :id="'rfq-' + rfqFieldPath('context.maximumDiameter')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.maximumDiameter')))" :aria-describedby="fieldError(rfqFieldPath('context.maximumDiameter')) ? 'error-' + rfqFieldPath('context.maximumDiameter') : undefined" type="number" min="0" step="any"><small v-if="fieldError('context.maximumDiameterMm')" class="form-error">{{ fieldError('context.maximumDiameterMm') }}</small></label>
            <label><span>{{ t('rfq.control') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="context.control" :id="'rfq-' + rfqFieldPath('context.control')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.control')))" :aria-describedby="fieldError(rfqFieldPath('context.control')) ? 'error-' + rfqFieldPath('context.control') : undefined"><small v-if="fieldError('context.control')" class="form-error">{{ fieldError('context.control') }}</small></label>
            <label class="wide"><span>{{ t('rfq.certifications') }} <small>({{ t('rfq.commaSeparated') }})</small></span><input v-model.trim="context.requiredCertifications" :id="'rfq-' + rfqFieldPath('context.requiredCertifications')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.requiredCertifications')))" :aria-describedby="fieldError(rfqFieldPath('context.requiredCertifications')) ? 'error-' + rfqFieldPath('context.requiredCertifications') : undefined"><small v-if="fieldError('context.requiredCertifications')" class="form-error">{{ fieldError('context.requiredCertifications') }}</small></label>
          </template>
          <label><span>{{ t('selector.voltage') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="context.voltage" :id="'rfq-' + rfqFieldPath('context.voltage')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.voltage')))" :aria-describedby="fieldError(rfqFieldPath('context.voltage')) ? 'error-' + rfqFieldPath('context.voltage') : undefined" inputmode="decimal" :placeholder="t('rfq.valueOnly')"></label>
          <label><span>{{ t('rfq.frequency') }} <small>({{ t('common.optional') }})</small></span><select v-model="context.frequency" :id="'rfq-' + rfqFieldPath('context.frequency')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.frequency')))" :aria-describedby="fieldError(rfqFieldPath('context.frequency')) ? 'error-' + rfqFieldPath('context.frequency') : undefined"><option value="">{{ t('rfq.notSpecified') }}</option><option value="50">50 Hz</option><option value="60">60 Hz</option></select></label>
          <label class="wide"><span>{{ t('rfq.environment') }} <small>({{ t('common.optional') }})</small></span><textarea v-model.trim="context.environment" :id="'rfq-' + rfqFieldPath('context.environment')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.environment')))" :aria-describedby="fieldError(rfqFieldPath('context.environment')) ? 'error-' + rfqFieldPath('context.environment') : undefined" rows="4" :placeholder="t('rfq.environmentPlaceholder')"></textarea></label>
          <label v-if="kind === 'project'" class="wide"><span>{{ t('rfq.projectStage') }}</span><select v-model="context.projectStage" :id="'rfq-' + rfqFieldPath('context.projectStage')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.projectStage')))" :aria-describedby="fieldError(rfqFieldPath('context.projectStage')) ? 'error-' + rfqFieldPath('context.projectStage') : undefined" required><option value="">{{ t('rfq.selectStage') }}</option><option value="Concept">{{ t('rfq.concept') }}</option><option value="Engineering">{{ t('rfq.engineering') }}</option><option value="Prototype">{{ t('rfq.prototype') }}</option><option value="Production planning">{{ t('rfq.productionPlanning') }}</option></select></label>
          <template v-if="kind === 'project'">
            <label><span>{{ t('rfq.projectScale') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="context.projectScale" :id="'rfq-' + rfqFieldPath('context.projectScale')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.projectScale')))" :aria-describedby="fieldError(rfqFieldPath('context.projectScale')) ? 'error-' + rfqFieldPath('context.projectScale') : undefined"><small v-if="fieldError('context.projectScale')" class="form-error">{{ fieldError('context.projectScale') }}</small></label>
            <label><span>{{ t('rfq.schedule') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="context.schedule" :id="'rfq-' + rfqFieldPath('context.schedule')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.schedule')))" :aria-describedby="fieldError(rfqFieldPath('context.schedule')) ? 'error-' + rfqFieldPath('context.schedule') : undefined"><small v-if="fieldError('context.schedule')" class="form-error">{{ fieldError('context.schedule') }}</small></label>
            <label class="wide"><span>{{ t('rfq.engineeringNeeds') }} <small>({{ t('common.optional') }})</small></span><textarea v-model.trim="context.engineeringNeeds" :id="'rfq-' + rfqFieldPath('context.engineeringNeeds')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.engineeringNeeds')))" :aria-describedby="fieldError(rfqFieldPath('context.engineeringNeeds')) ? 'error-' + rfqFieldPath('context.engineeringNeeds') : undefined" rows="4"></textarea><small v-if="fieldError('context.engineeringNeeds')" class="form-error">{{ fieldError('context.engineeringNeeds') }}</small></label>
          </template>
          <template v-if="kind === 'replacement'">
            <label class="wide"><span>{{ t('rfq.installationConstraints') }} <small>({{ t('common.optional') }})</small></span><textarea v-model.trim="context.installationConstraints" :id="'rfq-' + rfqFieldPath('context.installationConstraints')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.installationConstraints')))" :aria-describedby="fieldError(rfqFieldPath('context.installationConstraints')) ? 'error-' + rfqFieldPath('context.installationConstraints') : undefined" rows="4"></textarea><small v-if="fieldError('context.installationConstraints')" class="form-error">{{ fieldError('context.installationConstraints') }}</small></label>
            <label class="wide"><span>{{ t('rfq.replacementGoal') }} <small>({{ t('common.optional') }})</small></span><textarea v-model.trim="context.replacementGoal" :id="'rfq-' + rfqFieldPath('context.replacementGoal')" :aria-invalid="Boolean(fieldError(rfqFieldPath('context.replacementGoal')))" :aria-describedby="fieldError(rfqFieldPath('context.replacementGoal')) ? 'error-' + rfqFieldPath('context.replacementGoal') : undefined" rows="3"></textarea><small v-if="fieldError('context.replacementGoal')" class="form-error">{{ fieldError('context.replacementGoal') }}</small></label>
          </template>
        </div>
      </div>

      <div v-else-if="step === 2">
        <p class="eyebrow">{{ t('rfq.contactDetails') }}</p><h2>{{ t('rfq.followUp') }}</h2>
        <div class="field-grid">
          <label><span>{{ t('forms.company') }}</span><input v-model.trim="contact.company" :id="'rfq-' + rfqFieldPath('contact.company')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.company')))" :aria-describedby="fieldError(rfqFieldPath('contact.company')) ? 'error-' + rfqFieldPath('contact.company') : undefined" autocomplete="organization" required></label>
          <label><span>{{ t('forms.name') }}</span><input v-model.trim="contact.contactName" :id="'rfq-' + rfqFieldPath('contact.contactName')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.contactName')))" :aria-describedby="fieldError(rfqFieldPath('contact.contactName')) ? 'error-' + rfqFieldPath('contact.contactName') : undefined" autocomplete="name" required></label>
          <label><span>{{ t('forms.email') }}</span><input v-model.trim="contact.businessEmail" :id="'rfq-' + rfqFieldPath('contact.businessEmail')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.businessEmail')))" :aria-describedby="fieldError(rfqFieldPath('contact.businessEmail')) ? 'error-' + rfqFieldPath('contact.businessEmail') : undefined" type="email" autocomplete="email" required></label>
          <label><span>{{ t('forms.phone') }} <small>({{ t('common.optional') }})</small></span><input v-model.trim="contact.phone" :id="'rfq-' + rfqFieldPath('contact.phone')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.phone')))" :aria-describedby="fieldError(rfqFieldPath('contact.phone')) ? 'error-' + rfqFieldPath('contact.phone') : undefined" type="tel" autocomplete="tel"><small v-if="fieldError('contact.phone')" class="form-error">{{ fieldError('contact.phone') }}</small></label>
          <label><span>{{ t('rfq.country') }}</span><input v-model.trim="contact.country" :id="'rfq-' + rfqFieldPath('contact.country')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.country')))" :aria-describedby="fieldError(rfqFieldPath('contact.country')) ? 'error-' + rfqFieldPath('contact.country') : undefined" autocomplete="country-name" required></label>
          <label class="wide"><span>{{ t('rfq.additional') }} <small>({{ t('common.optional') }})</small></span><textarea v-model.trim="contact.message" :id="'rfq-' + rfqFieldPath('contact.message')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.message')))" :aria-describedby="fieldError(rfqFieldPath('contact.message')) ? 'error-' + rfqFieldPath('contact.message') : undefined" rows="5"></textarea></label>
          <label class="honeypot" aria-hidden="true"><span>{{ t('rfq.website') }}</span><input v-model="contact.website" tabindex="-1" autocomplete="off"></label>
          <label class="checkbox wide"><input v-model="contact.consent" :id="'rfq-' + rfqFieldPath('contact.consent')" :aria-invalid="Boolean(fieldError(rfqFieldPath('contact.consent')))" :aria-describedby="fieldError(rfqFieldPath('contact.consent')) ? 'error-' + rfqFieldPath('contact.consent') : undefined" type="checkbox" required><span>{{ t('rfq.consent') }} <a href="/en/privacy">{{ t('forms.privacy') }}</a>.</span></label>
        </div>
      </div>

      <div v-else class="review-panel">
        <p class="eyebrow">{{ t('rfq.review') }}</p><h2>{{ t('rfq.check') }}</h2>
        <dl>
          <div><dt>{{ t('rfq.inquiry') }}</dt><dd>{{ t(`rfq.kinds.${kind}`) }}</dd></div>
          <div v-for="[key, value] in reviewFields" :key="key"><dt>{{ t(`rfq.fields.${key}`) }}</dt><dd>{{ value }}</dd></div>
          <div v-if="productContext"><dt>{{ t('rfq.publishedProduct') }}</dt><dd lang="en">{{ productContext.model }} · {{ productContext.stableId }} · {{ t('rfq.revisionInline', { revision: productContext.publishedRevision }) }}</dd></div>
          <div v-if="contact.message"><dt>{{ t('rfq.additionalMessage') }}</dt><dd>{{ contact.message }}</dd></div>
          <div><dt>{{ t('forms.company') }}</dt><dd>{{ contact.company }}</dd></div><div><dt>{{ t('rfq.contactLabel') }}</dt><dd>{{ contact.contactName }} · {{ contact.businessEmail }}<template v-if="contact.phone"> · {{ contact.phone }}</template></dd></div>
          <div><dt>{{ t('rfq.country') }}</dt><dd>{{ contact.country }}</dd></div>
          <div><dt>{{ t('rfq.permission') }}</dt><dd>{{ contact.consent ? t('rfq.agreed') : t('rfq.notAgreed') }}</dd></div>
        </dl>
        <p class="review-note">{{ t('rfq.submitNote') }}</p>
      </div>

      <ul v-if="Object.keys(fieldErrors).length" :aria-label="t('forms.fieldCorrection')">
        <li v-for="(message, path) in fieldErrors" :id="'error-' + path" :key="path"><button type="button" class="button secondary" @click="showField(path)">{{ message }}</button></li>
      </ul>
      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div class="button-row form-actions">
        <button v-if="step > 1" class="button secondary" type="button" @click="previous">{{ t('common.back') }}</button>
        <button class="button" type="submit" :disabled="state === 'submitting' || productBlocked">{{ step < 3 ? t('common.continue') : state === 'submitting' ? t('rfq.submitting') : t('rfq.submit') }}</button>
      </div>
    </form>
  </section>
</template>
