import { certificationInput, inputText, numericInput, type NumericInput } from '@/features/conversion/lib/formValues'
import type { ProductFamily } from '@airtek/contracts'
import type {
  CreateContactRequest,
  CreateRfqRequest,
  ProductContext,
  ProductRfqRequest,
  ProjectRfqRequest,
  ReplacementRfqRequest,
  RfqJourney,
  SelectionRfqRequest,
} from '@airtek/contracts'

export interface ContactFormValues {
  topic: string
  company: string
  contactName: string
  businessEmail: string
  phone: string
  message: string
  consent: boolean
}

export interface RfqContextValues {
  ambientTemperature?: NumericInput
  preferredFamily?: string
  motorTechnology?: string
  application: string
  existingModel: string
  quantity: string
  airflow: NumericInput
  airflowUnit: string
  pressure: NumericInput
  pressureUnit: string
  voltage: string
  frequency: NumericInput
  environment: string
  projectStage: string
  priority: string
  maximumDiameter: NumericInput
  requiredCertifications: string
  control: string
  projectScale: string
  schedule: string
  engineeringNeeds: string
  installationConstraints: string
  replacementGoal: string
}

export interface RfqContactValues {
  company: string
  contactName: string
  businessEmail: string
  phone: string
  country: string
  message: string
  consent: boolean
}

export function hasCompleteProductContext(value: ProductContext | undefined): value is ProductContext & { model: string } {
  return Boolean(value
    && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(value.productId)
    && value.stableId.trim()
    && value.model?.trim()
    && Number.isInteger(value.publishedRevision)
    && value.publishedRevision > 0)
}

export function buildContactRequest(values: ContactFormValues): CreateContactRequest {
  return {
    contact: {
      name: values.contactName,
      email: values.businessEmail,
      ...(values.phone ? { phone: values.phone } : {}),
      ...(values.company ? { company: values.company } : {}),
    },
    topic: values.topic,
    message: values.message,
    sourcePath: '/en/company/contact',
    locale: 'en',
    consent: values.consent,
  }
}

export function buildRfqRequest(
  journey: 'product',
  values: RfqContextValues,
  contact: RfqContactValues,
  productContext: ProductContext,
): ProductRfqRequest
export function buildRfqRequest(
  journey: 'selection',
  values: RfqContextValues,
  contact: RfqContactValues,
  productContext?: ProductContext,
): SelectionRfqRequest
export function buildRfqRequest(
  journey: 'project',
  values: RfqContextValues,
  contact: RfqContactValues,
  productContext?: ProductContext,
): ProjectRfqRequest
export function buildRfqRequest(
  journey: 'replacement',
  values: RfqContextValues,
  contact: RfqContactValues,
  productContext?: ProductContext,
): ReplacementRfqRequest
export function buildRfqRequest(
  journey: RfqJourney,
  values: RfqContextValues,
  contact: RfqContactValues,
  productContext?: ProductContext,
): CreateRfqRequest
export function buildRfqRequest(
  journey: RfqJourney,
  values: RfqContextValues,
  contact: RfqContactValues,
  productContext?: ProductContext,
): CreateRfqRequest {
  if (journey === 'product' && !hasCompleteProductContext(productContext)) {
    throw new Error('A Product RFQ must start from a published product record with a stable ID, model and revision.')
  }

  if (!contact.consent) throw new Error('Consent is required before an RFQ can be submitted.')
  const priority = ['efficiency', 'noise', 'size', 'headroom'].includes(values.priority)
    ? values.priority as 'efficiency' | 'noise' | 'size' | 'headroom'
    : undefined
  const frequency = numericInput(values.frequency, 'context.electrical.frequencyHz', { positive: true, maximum: 1000 })
  const commonContext = {
    application: values.application,
    ...(values.quantity ? { quantity: values.quantity } : {}),
    ...(values.voltage || frequency !== undefined ? { electrical: {
      ...(values.voltage ? { voltage: values.voltage } : {}),
      ...(frequency !== undefined ? { frequencyHz: frequency } : {}),
    } } : {}),
    ...(values.environment ? { environment: values.environment } : {}),
    ...(priority ? { priority } : {}),
    ...(contact.message ? { additionalMessage: contact.message } : {}),
  }
  const commonRequest = {
    contact: {
      name: contact.contactName,
      email: contact.businessEmail,
      ...(contact.phone ? { phone: contact.phone } : {}),
      ...(contact.company ? { company: contact.company } : {}),
      ...(contact.country ? { countryOrRegion: contact.country } : {}),
    },
    sourcePath: `/en/request-a-quote/${journey}`,
    locale: 'en' as const,
    consent: true as const,
  }

  if (journey === 'product') {
    const context = productContext as ProductContext & { model: string }
    return {
      ...commonRequest,
      journey,
      productContext: {
        productId: context.productId,
        stableId: context.stableId,
        model: context.model,
        publishedRevision: context.publishedRevision,
      },
      context: commonContext,
    }
  }
  const dutyPoint = () => {
    if (!['m3/h', 'm³/h', 'CFM', 'cfm'].includes(values.airflowUnit)
      || !['Pa', 'pa', 'kPa', 'kpa', 'inH2O', 'inh2o'].includes(values.pressureUnit)) {
      throw new Error('The selected duty-point units are not supported.')
    }
    return {
      airflow: numericInput(values.airflow, 'context.dutyPoint.airflow', { required: true, positive: true, maximum: 1e9 })!,
      airflowUnit: values.airflowUnit as 'm3/h' | 'm³/h' | 'CFM' | 'cfm',
      pressure: numericInput(values.pressure, 'context.dutyPoint.pressure', { required: true, positive: true, maximum: 1e8 })!,
      pressureUnit: values.pressureUnit as 'Pa' | 'pa' | 'kPa' | 'kpa' | 'inH2O' | 'inh2o',
    }
  }
  if (journey === 'selection') {
    const requiredCertifications = certificationInput(values.requiredCertifications)
    const ambientTemperatureC = numericInput(values.ambientTemperature, 'context.ambientTemperatureC', { minimum: -273.15, maximum: 1000 })
    const maximumDiameterMm = numericInput(values.maximumDiameter, 'context.maximumDiameterMm', { positive: true, maximum: 100000 })
    return {
      ...commonRequest,
      journey,
      context: {
        ...commonContext,
        dutyPoint: dutyPoint(),
        ...(ambientTemperatureC !== undefined ? { ambientTemperatureC } : {}),
        ...(values.preferredFamily && ['axial', 'centrifugal', 'crossFlow', 'inlineDuct', 'motors'].includes(values.preferredFamily) ? { preferredFamily: values.preferredFamily as ProductFamily } : {}),
        ...(values.motorTechnology ? { motorTechnology: values.motorTechnology } : {}),
        ...(maximumDiameterMm !== undefined ? { maximumDiameterMm } : {}),
        ...(requiredCertifications.length ? { requiredCertifications } : {}),
        ...(values.control ? { control: values.control } : {}),
      },
    }
  }
  if (journey === 'project') {
    if (!['Concept', 'Engineering', 'Prototype', 'Production planning'].includes(values.projectStage)) {
      throw new Error('A supported project stage is required.')
    }
    return {
      ...commonRequest,
      journey,
      context: {
        ...commonContext,
        projectStage: values.projectStage as 'Concept' | 'Engineering' | 'Prototype' | 'Production planning',
        ...(values.projectScale ? { projectScale: values.projectScale } : {}),
        ...(values.schedule ? { schedule: values.schedule } : {}),
        ...(values.engineeringNeeds ? { engineeringNeeds: values.engineeringNeeds } : {}),
      },
    }
  }
  if (!values.existingModel) throw new Error('Existing model or nameplate text is required for a Replacement RFQ.')
  return {
    ...commonRequest,
    journey,
    context: {
      ...commonContext,
      existingModel: values.existingModel,
      dutyPoint: dutyPoint(),
      ...(values.installationConstraints ? { installationConstraints: values.installationConstraints } : {}),
      ...(values.replacementGoal ? { replacementGoal: values.replacementGoal } : {}),
    },
  }
}

const rfqContextKeys: Array<keyof RfqContextValues> = [
  'application', 'existingModel', 'quantity', 'airflow', 'airflowUnit', 'pressure', 'pressureUnit',
  'voltage', 'frequency', 'environment', 'projectStage', 'priority', 'maximumDiameter',
  'requiredCertifications', 'control', 'projectScale', 'schedule', 'engineeringNeeds',
  'installationConstraints', 'replacementGoal', 'ambientTemperature', 'preferredFamily', 'motorTechnology',
]

export function serializeRfqSession(values: RfqContextValues): string {
  return JSON.stringify({ version: 3, context: Object.fromEntries(rfqContextKeys.filter((key) => values[key] !== undefined).map((key) => [key, inputText(values[key])])) })
}

export function parseRfqSession(value: string | null): Partial<RfqContextValues> | undefined {
  if (!value) return undefined
  try {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)
      || !('version' in parsed) || parsed.version !== 3
      || !('context' in parsed) || typeof parsed.context !== 'object'
      || parsed.context === null || Array.isArray(parsed.context)) return undefined
    const context = parsed.context as Record<string, unknown>
    const restored: Partial<RfqContextValues> = {}
    for (const key of rfqContextKeys) {
      if (typeof context[key] === 'string' || (typeof context[key] === 'number' && Number.isFinite(context[key]))) restored[key] = inputText(context[key])
    }
    return restored
  } catch {
    return undefined
  }
}
