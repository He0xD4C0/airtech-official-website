<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import type { ProductFamily, SelectorRequest, SelectorResponse } from '@airtek/contracts'
import type { ProductFamilyProjection } from '@/types/content'
import { selectProducts } from '@/lib/api'
import { trackAnalyticsEvent } from '@/lib/analytics'
import { useCompareStore } from '@/stores/compare'

const props = defineProps<{ productFamilies: ProductFamilyProjection[]; motorTechnologies: string[] }>()
const step = ref<1 | 2 | 3 | 4>(1)
const touched = ref(false)
const submitting = ref(false)
const error = ref('')
const result = ref<SelectorResponse>()
const compare = useCompareStore()
const form = reactive({
  airflow: '', airflowUnit: 'm3/h', pressure: '', pressureUnit: 'Pa',
  ambientTemperature: '', maximumDiameter: '', voltage: '', frequency: '', certifications: '',
  family: '', motorTechnology: '', environment: '', control: '',
  priority: 'efficiency' as NonNullable<SelectorRequest['priority']>,
})

const dutyPointValid = computed(() => Number(form.airflow) > 0 && Number(form.pressure) > 0)
const familyMap = computed(() => Object.fromEntries(
  props.productFamilies.map((family) => [family.slug, family.code]),
) as Record<string, ProductFamily>)
const resultTitle = computed(() => ({
  matched: 'Published candidates found', noValidatedCandidates: 'No validated candidates',
  engineeringReviewRequired: 'Engineering review required',
})[result.value?.outcome ?? 'engineeringReviewRequired'])

function optionalPositive(value: string): number | undefined {
  if (!value.trim()) return undefined
  const number = Number(value)
  return Number.isFinite(number) && number > 0 ? number : undefined
}

function optionalFinite(value: string): number | undefined {
  if (!value.trim()) return undefined
  const number = Number(value)
  return Number.isFinite(number) ? number : undefined
}

function certificationValues(): string[] {
  return form.certifications.split(/[\n,]/u).map((value) => value.trim()).filter(Boolean).slice(0, 20)
}

function constraintCount(): number {
  return 2 + [
    form.ambientTemperature, form.maximumDiameter, form.voltage, form.frequency,
    form.certifications, form.family, form.motorTechnology,
  ].filter((value) => value.trim()).length
}

function next(): void {
  touched.value = true
  if (step.value === 1 && !dutyPointValid.value) return
  if (step.value < 3) {
    void trackAnalyticsEvent('selectorStepCompleted', { step: step.value, constraintCount: constraintCount() })
    step.value = (step.value + 1) as 2 | 3
    touched.value = false
  }
}

function previous(): void {
  if (step.value > 1 && step.value < 4) step.value = (step.value - 1) as 1 | 2
}

function rfqContext() {
  return {
    application: '', existingModel: '', quantity: '', airflow: form.airflow,
    airflowUnit: form.airflowUnit, pressure: form.pressure, pressureUnit: form.pressureUnit,
    voltage: form.voltage, frequency: form.frequency, environment: form.environment,
    projectStage: '', priority: form.priority, maximumDiameter: form.maximumDiameter,
    requiredCertifications: form.certifications, control: form.control,
    projectScale: '', schedule: '', engineeringNeeds: '', installationConstraints: '', replacementGoal: '',
  }
}

async function evaluate(): Promise<void> {
  submitting.value = true
  error.value = ''
  void trackAnalyticsEvent('selectorStarted', {
    constraintCount: constraintCount(),
    preferredFamily: form.family ? familyMap.value[form.family] : 'open',
    priority: form.priority,
  })
  try {
    result.value = await selectProducts({
      airflow: Number(form.airflow), airflowUnit: form.airflowUnit,
      pressure: Number(form.pressure), pressureUnit: form.pressureUnit,
      requiredCertifications: certificationValues(),
      ...(optionalFinite(form.ambientTemperature) !== undefined ? { ambientTemperatureC: Number(form.ambientTemperature) } : {}),
      ...(optionalPositive(form.maximumDiameter) ? { maximumDiameterMm: Number(form.maximumDiameter) } : {}),
      ...(form.voltage.trim() ? { voltage: form.voltage.trim() } : {}),
      ...(optionalPositive(form.frequency) ? { frequencyHz: Number(form.frequency) } : {}),
      ...(form.family ? { preferredFamily: familyMap.value[form.family] } : {}),
      ...(form.motorTechnology ? { motorTechnology: form.motorTechnology } : {}),
      priority: form.priority,
    })
    window.sessionStorage.setItem('airtek.public.rfq-context.selection.v2', JSON.stringify({
      version: 2,
      context: rfqContext(),
    }))
    step.value = 4
    void trackAnalyticsEvent('selectorStepCompleted', { step: 3, constraintCount: constraintCount() })
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

function revise(): void {
  step.value = 1
  error.value = ''
}

function familyName(code: ProductFamily): string {
  return props.productFamilies.find((family) => family.code === code)?.name ?? code
}

function addCandidate(candidate: SelectorResponse['candidates'][number]): void {
  compare.hydrate()
  compare.add({
    id: candidate.productId, slug: candidate.slug, label: candidate.title,
    family: familyName(candidate.family), familyCode: candidate.family,
    dataState: 'published', publishedRevision: candidate.productRevision,
  })
}
</script>

<template>
  <section class="section shell selector-layout">
    <aside class="selector-steps" aria-label="Selector steps">
      <ol>
        <li :class="{ active: step === 1 }"><span>1</span> Duty point</li>
        <li :class="{ active: step === 2 }"><span>2</span> Hard constraints</li>
        <li :class="{ active: step === 3 }"><span>3</span> Preferences</li>
        <li :class="{ active: step === 4 }"><span>4</span> Result</li>
      </ol>
      <p>Only structured fields in the selector request are evaluated. Free-text notes are carried to the Selection RFQ.</p>
    </aside>

    <form v-if="step < 4" class="form-card" @submit.prevent="step < 3 ? next() : evaluate()">
      <div v-if="step === 1">
        <p class="eyebrow">Step 1 of 4</p><h2>Define the duty point</h2>
        <p>A result requires a verified published curve with exactly matching units.</p>
        <div class="field-grid">
          <label><span>Required airflow</span><div class="input-unit"><input v-model="form.airflow" aria-label="Required airflow" inputmode="decimal" type="number" min="0" step="any" required><select v-model="form.airflowUnit" aria-label="Airflow unit"><option value="m3/h">m³/h</option><option value="CFM">CFM</option></select></div></label>
          <label><span>Required pressure</span><div class="input-unit"><input v-model="form.pressure" aria-label="Required pressure" inputmode="decimal" type="number" min="0" step="any" required><select v-model="form.pressureUnit" aria-label="Pressure unit"><option value="Pa">Pa</option><option value="kPa">kPa</option><option value="inH2O">inH₂O</option></select></div></label>
        </div>
        <p v-if="touched && !dutyPointValid" class="form-error" role="alert">Enter positive airflow and pressure values to continue.</p>
      </div>

      <div v-else-if="step === 2">
        <p class="eyebrow">Step 2 of 4</p><h2>Add hard constraints</h2>
        <div class="field-grid">
          <label><span>Ambient temperature °C <small>(optional)</small></span><input v-model="form.ambientTemperature" type="number" step="any"></label>
          <label><span>Maximum diameter mm <small>(optional)</small></span><input v-model="form.maximumDiameter" type="number" min="0" step="any"></label>
          <label><span>Voltage <small>(optional, exact curve label)</small></span><input v-model.trim="form.voltage"></label>
          <label><span>Frequency Hz <small>(optional)</small></span><input v-model="form.frequency" type="number" min="0" step="any"></label>
          <label class="wide"><span>Required certifications <small>(optional, comma separated)</small></span><input v-model.trim="form.certifications"></label>
          <label class="wide"><span>Environment or installation notes <small>(RFQ only)</small></span><textarea v-model.trim="form.environment" rows="3"></textarea></label>
          <label class="wide"><span>Control method <small>(RFQ only)</small></span><input v-model.trim="form.control"></label>
        </div>
        <p class="review-note">If published records cannot evaluate a structured constraint, the outcome is engineering review required—not a guessed match.</p>
      </div>

      <div v-else>
        <p class="eyebrow">Step 3 of 4</p><h2>Choose preferences</h2>
        <div class="field-grid">
          <label v-if="productFamilies.length"><span>Fan form</span><select v-model="form.family"><option value="">Open</option><option v-for="family in productFamilies" :key="family.code" :value="family.slug">{{ family.name }}</option></select></label>
          <label v-if="motorTechnologies.length"><span>Motor technology</span><select v-model="form.motorTechnology"><option value="">Open</option><option v-for="technology in motorTechnologies" :key="technology" :value="technology">{{ technology }}</option></select></label>
          <fieldset class="wide priority-field"><legend>Ranking priority</legend><label v-for="value in ['efficiency', 'noise', 'size', 'headroom']" :key="value"><input v-model="form.priority" type="radio" name="priority" :value="value"><span>{{ value.charAt(0).toUpperCase() + value.slice(1) }}</span></label></fieldset>
        </div>
      </div>

      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div class="button-row form-actions">
        <button v-if="step > 1" class="button secondary" type="button" @click="previous">Back</button>
        <button class="button" type="submit" :disabled="submitting">{{ step < 3 ? 'Continue' : submitting ? 'Evaluating…' : 'Evaluate published records' }}</button>
      </div>
    </form>

    <div v-else class="selector-result" aria-live="polite">
      <p class="eyebrow">Step 4 of 4 · Selection status</p><h2>{{ resultTitle }}</h2>
      <p v-if="result?.outcome === 'noValidatedCandidates'">No candidate was returned from the published projection. Your inputs were not converted into a synthetic match score.</p>
      <p v-else-if="result?.outcome === 'engineeringReviewRequired'">The published data is not sufficient for an automated recommendation. Continue through engineering review.</p>
      <p v-else>Only candidates returned by the public selector API are shown.</p>
      <ul v-if="result?.explanations.length" class="feature-list"><li v-for="explanation in result.explanations" :key="explanation">{{ explanation }}</li></ul>
      <div v-if="result?.candidates.length" class="card-grid two">
        <article v-for="candidate in result.candidates" :key="candidate.productId" class="card">
          <p class="eyebrow">Rank {{ candidate.rank }} · {{ familyName(candidate.family) }}</p><h3><a :href="candidate.canonicalPath">{{ candidate.title }}</a></h3>
          <ul><li v-for="reason in candidate.matchedConstraints" :key="reason">{{ reason }}</li></ul>
          <p v-for="warning in candidate.warnings" :key="warning" class="review-note">{{ warning }}</p>
          <div class="button-row"><a class="button secondary" :href="candidate.canonicalPath">View product</a><button class="button secondary" type="button" @click="addCandidate(candidate)">Add to compare</button></div>
        </article>
      </div>
      <dl class="input-summary">
        <div><dt>Duty point</dt><dd>{{ form.airflow }} {{ form.airflowUnit }} at {{ form.pressure }} {{ form.pressureUnit }}</dd></div>
        <div><dt>Fan form</dt><dd>{{ form.family || 'Open' }}</dd></div><div><dt>Motor technology</dt><dd>{{ form.motorTechnology || 'Open' }}</dd></div><div><dt>Priority</dt><dd>{{ form.priority }}</dd></div>
      </dl>
      <div class="button-row"><button class="button secondary" type="button" @click="revise">Revise inputs</button><a class="button" href="/en/request-a-quote/selection">Send to selection RFQ</a></div>
    </div>
  </section>
</template>
