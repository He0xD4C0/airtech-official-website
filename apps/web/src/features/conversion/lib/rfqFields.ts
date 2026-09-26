const paths: Record<string, string> = {
  'context.airflow': 'context.dutyPoint.airflow', 'context.airflowUnit': 'context.dutyPoint.airflowUnit',
  'context.pressure': 'context.dutyPoint.pressure', 'context.pressureUnit': 'context.dutyPoint.pressureUnit',
  'context.voltage': 'context.electrical.voltage', 'context.frequency': 'context.electrical.frequencyHz',
  'context.maximumDiameter': 'context.maximumDiameterMm', 'context.ambientTemperature': 'context.ambientTemperatureC',
  'contact.contactName': 'contact.name', 'contact.businessEmail': 'contact.email',
  'contact.country': 'contact.countryOrRegion', 'contact.message': 'context.additionalMessage', 'contact.consent': 'consent',
}
export function rfqFieldPath(model: string): string { return paths[model] ?? model }
import type { RfqContextValues } from '@/features/conversion/lib/submissions'
import type { RfqType } from '@/shared/types/content'

export function rfqReviewFields(kind: RfqType, values: RfqContextValues) {
  const common = ['application', 'quantity', 'voltage', 'frequency', 'environment', 'priority']
  const duty = ['airflow', 'airflowUnit', 'pressure', 'pressureUnit']
  const extra = {
    product: [],
    selection: [...duty, 'ambientTemperature', 'preferredFamily', 'motorTechnology', 'maximumDiameter', 'requiredCertifications', 'control'],
    project: ['projectStage', 'projectScale', 'schedule', 'engineeringNeeds'],
    replacement: [...duty, 'existingModel', 'installationConstraints', 'replacementGoal'],
  }
  return Object.entries(values).filter(([key, value]) => [...common, ...extra[kind]].includes(key) && value !== '' && value !== undefined)
}
