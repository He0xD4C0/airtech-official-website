<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import type { ProductFamily, SelectorRequest, SelectorResponse } from '@airtek/contracts'
import type { ProductFamilyProjection } from '@/shared/types/content'
import { selectProducts } from '@/shared/lib/api'
import { trackAnalyticsEvent } from '@/features/analytics'
import { useCompareStore } from '@/features/compare'

import { certificationInput, inputText, numericInput, type NumericInput } from '@/features/conversion'
import { serializeRfqSession } from '@/features/conversion'
import { useI18n } from 'vue-i18n'

const props = defineProps<{ productFamilies: ProductFamilyProjection[]; motorTechnologies: string[] }>()
const { locale, t } = useI18n({ useScope: 'global' })
const step = ref<1 | 2 | 3 | 4>(1)
const touched = ref(false)
const submitting = ref(false)
const error = ref('')
const result = ref<SelectorResponse>()
const compare = useCompareStore()
const form = reactive({
  airflow: '' as NumericInput, airflowUnit: 'm3/h', pressure: '' as NumericInput, pressureUnit: 'Pa',
  ambientTemperature: '' as NumericInput, maximumDiameter: '' as NumericInput, voltage: '', frequency: '' as NumericInput, certifications: '',
  family: '', motorTechnology: '', environment: '', control: '',
  priority: 'efficiency' as NonNullable<SelectorRequest['priority']>,
})

const dutyPointValid = computed(() => [form.airflow, form.pressure].every((value) => Number.isFinite(Number(value)) && Number(value) > 0))
const familyMap = computed(() => Object.fromEntries(
  props.productFamilies.map((family) => [family.slug, family.code]),
) as Record<string, ProductFamily>)
const resultTitle = computed(() => t(`selector.${result.value?.outcome ?? 'engineeringReviewRequired'}`))

function constraints() {
  return {
    ambientTemperatureC: numericInput(form.ambientTemperature, 'ambientTemperatureC', { minimum: -100, maximum: 300 }),
    maximumDiameterMm: numericInput(form.maximumDiameter, 'maximumDiameterMm', { positive: true, maximum: 100000 }),
    frequencyHz: numericInput(form.frequency, 'frequencyHz', { positive: true, maximum: 1000 }),
    requiredCertifications: certificationInput(form.certifications),
  }
}

function constraintCount(): number {
  return 2 + [
    form.ambientTemperature, form.maximumDiameter, form.voltage, form.frequency,
    form.certifications, form.family, form.motorTechnology,
  ].filter((value) => inputText(value)).length
}

function next(): void {
  touched.value = true
  if (step.value === 1 && !dutyPointValid.value) return
  try { constraints() } catch {
    error.value = t('errors.selector')
    return
  }
  error.value = ''
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
    ambientTemperature: form.ambientTemperature, preferredFamily: form.family ? familyMap.value[form.family] : '',
    motorTechnology: form.motorTechnology,
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
      airflow: numericInput(form.airflow, 'airflow', { required: true, positive: true })!, airflowUnit: form.airflowUnit,
      pressure: numericInput(form.pressure, 'pressure', { required: true, positive: true })!, pressureUnit: form.pressureUnit,
      ...constraints(),
      ...(form.voltage.trim() ? { voltage: form.voltage.trim() } : {}),
      ...(form.family ? { preferredFamily: familyMap.value[form.family] } : {}),
      ...(form.motorTechnology ? { motorTechnology: form.motorTechnology } : {}),
      priority: form.priority,
    })
    try { window.sessionStorage.setItem('airtek.public.rfq-context.selection.v3', serializeRfqSession(rfqContext())) } catch { /* Optional draft persistence. */ }
    step.value = 4
    void trackAnalyticsEvent('selectorStepCompleted', { step: 3, constraintCount: constraintCount() })
    void trackAnalyticsEvent('selectorResult', {
      outcome: result.value.outcome,
      candidateCount: result.value.candidates.length,
    })
  } catch {
    error.value = t('errors.selector')
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
  <section class="section shell selector-layout" :lang="locale">
    <aside class="selector-steps" :aria-label="t('selector.stepsLabel')">
      <ol>
        <li :class="{ active: step === 1 }"><span>1</span> {{ t('selector.dutyPoint') }}</li>
        <li :class="{ active: step === 2 }"><span>2</span> {{ t('selector.hardConstraints') }}</li>
        <li :class="{ active: step === 3 }"><span>3</span> {{ t('selector.preferences') }}</li>
        <li :class="{ active: step === 4 }"><span>4</span> {{ t('selector.result') }}</li>
      </ol>
      <p>{{ t('selector.scope') }}</p>
    </aside>

    <form v-if="step < 4" class="form-card" @submit.prevent="step < 3 ? next() : evaluate()">
      <div v-if="step === 1">
        <p class="eyebrow">{{ t('selector.step', { step: 1 }) }}</p><h2>{{ t('selector.defineDutyPoint') }}</h2>
        <p>{{ t('selector.curveRequirement') }}</p>
        <div class="field-grid">
          <label><span>{{ t('selector.airflow') }}</span><div class="input-unit"><input v-model="form.airflow" :aria-label="t('selector.airflow')" inputmode="decimal" type="number" min="0" step="any" required><select v-model="form.airflowUnit" :aria-label="t('selector.airflowUnit')"><option value="m3/h">m³/h</option><option value="CFM">CFM</option></select></div></label>
          <label><span>{{ t('selector.pressure') }}</span><div class="input-unit"><input v-model="form.pressure" :aria-label="t('selector.pressure')" inputmode="decimal" type="number" min="0" step="any" required><select v-model="form.pressureUnit" :aria-label="t('selector.pressureUnit')"><option value="Pa">Pa</option><option value="kPa">kPa</option><option value="inH2O">inH₂O</option></select></div></label>
        </div>
        <p v-if="touched && !dutyPointValid" class="form-error" role="alert">{{ t('selector.positiveValues') }}</p>
      </div>

      <div v-else-if="step === 2">
        <p class="eyebrow">{{ t('selector.step', { step: 2 }) }}</p><h2>{{ t('selector.addConstraints') }}</h2>
        <div class="field-grid">
          <label><span>{{ t('selector.ambient') }} <small>({{ t('common.optional') }})</small></span><input v-model="form.ambientTemperature" type="number" step="any"></label>
          <label><span>{{ t('selector.maximumDiameter') }} <small>({{ t('common.optional') }})</small></span><input v-model="form.maximumDiameter" type="number" min="0" step="any"></label>
          <label><span>{{ t('selector.voltage') }} <small>({{ t('selector.exactCurveLabel') }})</small></span><input v-model.trim="form.voltage"></label>
          <label><span>{{ t('selector.frequency') }} <small>({{ t('common.optional') }})</small></span><input v-model="form.frequency" type="number" min="0" step="any"></label>
          <label class="wide"><span>{{ t('selector.certifications') }} <small>({{ t('selector.commaSeparated') }})</small></span><input v-model.trim="form.certifications"></label>
          <label class="wide"><span>{{ t('selector.environment') }} <small>({{ t('selector.rfqOnly') }})</small></span><textarea v-model.trim="form.environment" rows="3"></textarea></label>
          <label class="wide"><span>{{ t('selector.control') }} <small>({{ t('selector.rfqOnly') }})</small></span><input v-model.trim="form.control"></label>
        </div>
        <p class="review-note">{{ t('selector.reviewRule') }}</p>
      </div>

      <div v-else>
        <p class="eyebrow">{{ t('selector.step', { step: 3 }) }}</p><h2>{{ t('selector.choosePreferences') }}</h2>
        <div class="field-grid">
          <label v-if="productFamilies.length"><span>{{ t('selector.fanForm') }}</span><select v-model="form.family"><option value="">{{ t('selector.open') }}</option><option v-for="family in productFamilies" :key="family.code" :value="family.slug">{{ family.name }}</option></select></label>
          <label v-if="motorTechnologies.length"><span>{{ t('selector.motorTechnology') }}</span><select v-model="form.motorTechnology"><option value="">{{ t('selector.open') }}</option><option v-for="technology in motorTechnologies" :key="technology" :value="technology">{{ technology }}</option></select></label>
          <fieldset class="wide priority-field"><legend>{{ t('selector.rankingPriority') }}</legend><label v-for="value in ['efficiency', 'noise', 'size', 'headroom']" :key="value"><input v-model="form.priority" type="radio" name="priority" :value="value"><span>{{ t(`selector.${value}`) }}</span></label></fieldset>
        </div>
      </div>

      <p v-if="error" class="form-error" role="alert">{{ error }}</p>
      <div class="button-row form-actions">
        <button v-if="step > 1" class="button secondary" type="button" @click="previous">{{ t('common.back') }}</button>
        <button class="button" type="submit" :disabled="submitting">{{ step < 3 ? t('common.continue') : submitting ? t('selector.evaluating') : t('selector.evaluate') }}</button>
      </div>
    </form>

    <div v-else class="selector-result" aria-live="polite">
      <p class="eyebrow">{{ t('selector.step', { step: 4 }) }} · {{ t('selector.selectionStatus') }}</p><h2>{{ resultTitle }}</h2>
      <p v-if="result?.outcome === 'noValidatedCandidates'">{{ t('selector.noCandidate') }}</p>
      <p v-else-if="result?.outcome === 'engineeringReviewRequired'">{{ t('selector.reviewRequired') }}</p>
      <p v-else>{{ t('selector.apiOnly') }}</p>
      <ul v-if="result?.explanations.length" class="feature-list" lang="en"><li v-for="explanation in result.explanations" :key="explanation">{{ explanation }}</li></ul>
      <div v-if="result?.candidates.length" class="card-grid two">
        <article v-for="candidate in result.candidates" :key="candidate.productId" class="card">
          <p class="eyebrow">{{ t('selector.rank', { rank: candidate.rank }) }} · <span lang="en">{{ familyName(candidate.family) }}</span></p><h3 lang="en"><a :href="candidate.canonicalPath">{{ candidate.title }}</a></h3>
          <ul lang="en"><li v-for="reason in candidate.matchedConstraints" :key="reason">{{ reason }}</li></ul>
          <p v-for="warning in candidate.warnings" :key="warning" class="review-note" lang="en">{{ warning }}</p>
          <div class="button-row"><a class="button secondary" :href="candidate.canonicalPath">{{ t('selector.viewProduct') }}</a><button class="button secondary" type="button" @click="addCandidate(candidate)">{{ t('selector.addToCompare') }}</button></div>
        </article>
      </div>
      <dl class="input-summary">
        <div><dt>{{ t('selector.dutyPoint') }}</dt><dd>{{ form.airflow }} {{ form.airflowUnit }} / {{ form.pressure }} {{ form.pressureUnit }}</dd></div>
        <div><dt>{{ t('selector.fanForm') }}</dt><dd lang="en">{{ form.family || t('selector.open') }}</dd></div><div><dt>{{ t('selector.motorTechnology') }}</dt><dd lang="en">{{ form.motorTechnology || t('selector.open') }}</dd></div><div><dt>{{ t('selector.priority') }}</dt><dd>{{ t(`selector.${form.priority}`) }}</dd></div>
      </dl>
      <div class="button-row"><button class="button secondary" type="button" @click="revise">{{ t('selector.revise') }}</button><a class="button" href="/en/request-a-quote/selection">{{ t('selector.sendToRfq') }}</a></div>
    </div>
  </section>
</template>
