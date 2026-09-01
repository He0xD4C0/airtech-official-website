<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import type { ProductFamily, SelectorRequest, SelectorResponse } from '@airtek/contracts'
import { selectProducts } from '@/lib/api'
import { trackAnalyticsEvent } from '@/lib/analytics'

type Stage = 'input' | 'result'
const stage = ref<Stage>('input')
const touched = ref(false)
const submitting = ref(false)
const error = ref('')
const result = ref<SelectorResponse>()
const form = reactive<{
  airflow: string
  airflowUnit: string
  pressure: string
  pressureUnit: string
  family: string
  motorTechnology: string
  environment: string
  priority: NonNullable<SelectorRequest['priority']>
}>({
  airflow: '',
  airflowUnit: 'm3/h',
  pressure: '',
  pressureUnit: 'Pa',
  family: '',
  motorTechnology: '',
  environment: '',
  priority: 'efficiency',
})

const valid = computed(() => Number(form.airflow) > 0 && Number(form.pressure) > 0)
const familyMap: Record<string, ProductFamily> = {
  'centrifugal-fans': 'centrifugal',
  'axial-fans': 'axial',
  'cross-flow-fans': 'crossFlow',
  'inline-duct-fans': 'inlineDuct',
  motors: 'motors',
}
const resultTitle = computed(() => ({
  matched: 'Published candidates found',
  noValidatedCandidates: 'No validated candidates',
  engineeringReviewRequired: 'Engineering review required',
})[result.value?.outcome ?? 'engineeringReviewRequired'])

async function evaluate() {
  touched.value = true
  if (!valid.value) return
  submitting.value = true
  error.value = ''
  void trackAnalyticsEvent('selectorStarted', {
    constraintCount: 2 + (form.family ? 1 : 0),
    preferredFamily: form.family ? familyMap[form.family] : 'open',
    priority: form.priority,
  })
  try {
    result.value = await selectProducts({
      airflow: Number(form.airflow),
      airflowUnit: form.airflowUnit,
      pressure: Number(form.pressure),
      pressureUnit: form.pressureUnit,
      requiredCertifications: [],
      ...(form.family ? { preferredFamily: familyMap[form.family] } : {}),
      priority: form.priority,
    })
    window.sessionStorage.setItem('airtek.public.rfq-context.selection.v1', JSON.stringify({
      application: '', existingModel: '', quantity: '',
      airflow: form.airflow, airflowUnit: form.airflowUnit,
      pressure: form.pressure, pressureUnit: form.pressureUnit,
      voltage: '', frequency: '', environment: form.environment,
      projectStage: '', priority: form.priority,
    }))
    stage.value = 'result'
    void trackAnalyticsEvent('selectorResult', {
      outcome: result.value.outcome,
      candidateCount: result.value.candidates.length,
    })
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : 'The selector service is unavailable.'
  } finally {
    submitting.value = false
  }
}

function revise() {
  stage.value = 'input'
  error.value = ''
}
</script>

<template>
  <section class="section shell selector-layout">
    <aside class="selector-steps" aria-label="Selector steps">
      <ol>
        <li class="active"><span>1</span> Duty point</li>
        <li><span>2</span> Hard constraints</li>
        <li><span>3</span> Preferences</li>
        <li :class="{ active: stage === 'result' }"><span>4</span> Result</li>
      </ol>
      <p>Units remain visible and are sent with the request. Hard constraints are evaluated before preferences.</p>
    </aside>

    <form v-if="stage === 'input'" class="form-card" @submit.prevent="evaluate">
      <div>
        <p class="eyebrow">Step 1–3</p>
        <h2>Define the operating context</h2>
        <p>These inputs prepare a structured query; they do not generate a recommendation without validated product curves.</p>
      </div>
      <div class="field-grid">
        <label>
          <span>Required airflow</span>
          <div class="input-unit"><input v-model="form.airflow" aria-label="Required airflow" inputmode="decimal" type="number" min="0" step="any" required><select v-model="form.airflowUnit" aria-label="Airflow unit"><option value="m3/h">m³/h</option><option value="CFM">CFM</option></select></div>
        </label>
        <label>
          <span>Required pressure</span>
          <div class="input-unit"><input v-model="form.pressure" aria-label="Required pressure" inputmode="decimal" type="number" min="0" step="any" required><select v-model="form.pressureUnit" aria-label="Pressure unit"><option value="Pa">Pa</option></select></div>
        </label>
        <label><span>Fan form</span><select v-model="form.family"><option value="">Open</option><option value="centrifugal-fans">Centrifugal</option><option value="axial-fans">Axial</option><option value="cross-flow-fans">Cross-flow</option><option value="inline-duct-fans">Inline duct</option></select></label>
        <label><span>Motor technology</span><select v-model="form.motorTechnology"><option value="">Open</option><option value="AC">AC</option><option value="DC">DC</option><option value="EC">EC</option></select></label>
        <label class="wide"><span>Environment or installation constraints</span><textarea v-model="form.environment" rows="3" placeholder="For example: ambient conditions, available envelope, required ingress protection"></textarea></label>
        <fieldset class="wide priority-field">
          <legend>Ranking priority</legend>
          <label v-for="value in ['efficiency', 'noise', 'size', 'headroom']" :key="value"><input v-model="form.priority" type="radio" name="priority" :value="value"><span>{{ value.charAt(0).toUpperCase() + value.slice(1) }}</span></label>
        </fieldset>
      </div>
      <p v-if="touched && !valid" class="form-error" role="alert">Enter positive airflow and pressure values to continue.</p>
      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <p class="review-note">Motor technology and environment are retained for the Selection RFQ handoff; the current API response only reflects fields present in its published selector contract.</p>
      <button class="button" type="submit" :disabled="submitting">{{ submitting ? 'Evaluating…' : 'Evaluate published records' }}</button>
    </form>

    <div v-else class="selector-result" aria-live="polite">
      <p class="eyebrow">Selection status</p>
      <h2>{{ resultTitle }}</h2>
      <p v-if="result?.outcome === 'noValidatedCandidates'">No candidate was returned from the published projection. Your inputs have not been converted into a synthetic match score.</p>
      <p v-else-if="result?.outcome === 'engineeringReviewRequired'">The published data is not sufficient for an automated recommendation. Continue through engineering review.</p>
      <p v-else>Only candidates returned by the public selector API are shown.</p>
      <ul v-if="result?.explanations.length" class="feature-list"><li v-for="explanation in result.explanations" :key="explanation">{{ explanation }}</li></ul>
      <div v-if="result?.candidates.length" class="card-grid two">
        <article v-for="candidate in result.candidates" :key="candidate.productId" class="card">
          <p class="eyebrow">Rank {{ candidate.rank }}</p><h3>{{ candidate.title }}</h3>
          <ul><li v-for="reason in candidate.matchedConstraints" :key="reason">{{ reason }}</li></ul>
          <p v-for="warning in candidate.warnings" :key="warning" class="review-note">{{ warning }}</p>
        </article>
      </div>
      <dl class="input-summary">
        <div><dt>Duty point</dt><dd>{{ form.airflow }} {{ form.airflowUnit }} at {{ form.pressure }} {{ form.pressureUnit }}</dd></div>
        <div><dt>Fan form</dt><dd>{{ form.family || 'Open' }}</dd></div>
        <div><dt>Motor technology</dt><dd>{{ form.motorTechnology || 'Open' }}</dd></div>
        <div><dt>Priority</dt><dd>{{ form.priority }}</dd></div>
      </dl>
      <div class="button-row">
        <button class="button secondary" type="button" @click="revise">Revise inputs</button>
        <a class="button" href="/en/request-a-quote/selection">Send to selection RFQ</a>
      </div>
    </div>
  </section>
</template>
