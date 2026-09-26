export type NumericInput = string | number

export function inputText(value: unknown): string {
  return typeof value === 'string' || (typeof value === 'number' && Number.isFinite(value))
    ? String(value).trim() : ''
}

export class FormValidationError extends Error {
  constructor(public errors: Record<string, string[]>) {
    super(Object.values(errors).flat().join(' '))
  }
}

export function numericInput(
  value: NumericInput | undefined,
  field: string,
  options: { required?: boolean; positive?: boolean; minimum?: number; maximum?: number } = {},
): number | undefined {
  if (inputText(value) === '' && !options.required && typeof value !== 'number') return undefined
  const number = Number(value)
  if (inputText(value) === '' || !Number.isFinite(number)
    || (options.positive && number <= 0)
    || (options.minimum !== undefined && number < options.minimum)
    || (options.maximum !== undefined && number > options.maximum)) {
    throw new FormValidationError({ [field]: ['Enter a valid' + (options.positive ? ' positive' : '') + ' number.'] })
  }
  return number
}

export function certificationInput(value: string, field = 'context.requiredCertifications'): string[] {
  const values = value.split(/[\n,]/u).map((entry) => entry.trim()).filter(Boolean)
  if (values.length > 20 || values.some((entry) => entry.length > 120)) {
    throw new FormValidationError({ [field]: ['Enter at most 20 certifications, each at most 120 characters.'] })
  }
  return values
}
