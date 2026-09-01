export function shouldUseMockApi(isDevelopment: boolean, configuredValue: string | undefined): boolean {
  return isDevelopment && configuredValue === 'true'
}
