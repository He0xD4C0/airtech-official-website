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
  application: string
  existingModel: string
  quantity: string
  airflow: string
  airflowUnit: string
  pressure: string
  pressureUnit: string
  voltage: string
  frequency: string
  environment: string
  projectStage: string
  priority: string
  maximumDiameter: string
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
  const commonContext = {
    application: values.application,
    ...(values.quantity ? { quantity: values.quantity } : {}),
    ...(values.voltage || values.frequency ? { electrical: {
      ...(values.voltage ? { voltage: values.voltage } : {}),
      ...(values.frequency ? { frequencyHz: Number(values.frequency) } : {}),
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
    if (!values.airflow || !values.pressure) throw new Error('A positive airflow and pressure duty point is required.')
    if (!['m3/h', 'm³/h', 'CFM', 'cfm'].includes(values.airflowUnit)
      || !['Pa', 'pa', 'kPa', 'kpa', 'inH2O', 'inh2o'].includes(values.pressureUnit)) {
      throw new Error('The selected duty-point units are not supported.')
    }
    return {
      airflow: Number(values.airflow),
      airflowUnit: values.airflowUnit as 'm3/h' | 'm³/h' | 'CFM' | 'cfm',
      pressure: Number(values.pressure),
      pressureUnit: values.pressureUnit as 'Pa' | 'pa' | 'kPa' | 'kpa' | 'inH2O' | 'inh2o',
    }
  }
  if (journey === 'selection') {
    const requiredCertifications = values.requiredCertifications
      .split(/[\n,]/u).map((value) => value.trim()).filter(Boolean).slice(0, 20)
    return {
      ...commonRequest,
      journey,
      context: {
        ...commonContext,
        dutyPoint: dutyPoint(),
        ...(values.maximumDiameter ? { maximumDiameterMm: Number(values.maximumDiameter) } : {}),
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
  'installationConstraints', 'replacementGoal',
]

export function serializeRfqSession(values: RfqContextValues): string {
  return JSON.stringify({ version: 2, context: values })
}

export function parseRfqSession(value: string | null): Partial<RfqContextValues> | undefined {
  if (!value) return undefined
  try {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)
      || !('version' in parsed) || parsed.version !== 2
      || !('context' in parsed) || typeof parsed.context !== 'object'
      || parsed.context === null || Array.isArray(parsed.context)) return undefined
    const context = parsed.context as Record<string, unknown>
    const restored: Partial<RfqContextValues> = {}
    for (const key of rfqContextKeys) {
      if (typeof context[key] === 'string') restored[key] = context[key]
    }
    return restored
  } catch {
    return undefined
  }
}
