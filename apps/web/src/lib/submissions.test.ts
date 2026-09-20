import { describe, expect, it } from 'vitest'
import type { ProductContext } from '@airtek/contracts'
import { buildContactRequest, buildRfqRequest, parseRfqSession, serializeRfqSession } from './submissions'

const rfqContext = {
  application: 'Air-handling system', existingModel: '', quantity: '4',
  airflow: '1200', airflowUnit: 'm3/h', pressure: '320', pressureUnit: 'Pa',
  voltage: '230 V', frequency: '50', environment: 'Indoor enclosure', projectStage: '', priority: 'efficiency',
  maximumDiameter: '450', requiredCertifications: 'CE, UL', control: '0-10 V',
  projectScale: '', schedule: '', engineeringNeeds: '', installationConstraints: '', replacementGoal: '',
}
const rfqContact = {
  company: 'Example Industrial', contactName: 'Buyer', businessEmail: 'buyer@example.com',
  phone: '+49 30 123456', country: 'DE', message: 'Please review the operating context.', consent: true,
}

describe('public submission contract builders', () => {
  it('builds the nested Rust contact request without legacy flat keys', () => {
    expect(buildContactRequest({
      topic: 'technical', company: 'Example Industrial', contactName: 'Buyer',
      businessEmail: 'buyer@example.com', phone: '+49 30 123456',
      message: 'A sufficiently detailed question.', consent: true,
    })).toEqual({
      contact: { name: 'Buyer', email: 'buyer@example.com', phone: '+49 30 123456', company: 'Example Industrial' },
      topic: 'technical',
      message: 'A sufficiently detailed question.',
      sourcePath: '/en/company/contact',
      locale: 'en',
      consent: true,
    })
  })

  it('places RFQ journey details inside the Rust context map', () => {
    expect(buildRfqRequest('selection', rfqContext, rfqContact)).toEqual({
      journey: 'selection',
      contact: {
        name: 'Buyer', email: 'buyer@example.com', phone: '+49 30 123456',
        company: 'Example Industrial', countryOrRegion: 'DE',
      },
      sourcePath: '/en/request-a-quote/selection',
      locale: 'en',
      consent: true,
      context: {
        application: 'Air-handling system',
        quantity: '4',
        dutyPoint: { airflow: 1200, airflowUnit: 'm3/h', pressure: 320, pressureUnit: 'Pa' },
        electrical: { voltage: '230 V', frequencyHz: 50 },
        environment: 'Indoor enclosure',
        priority: 'efficiency',
        maximumDiameterMm: 450,
        requiredCertifications: ['CE', 'UL'],
        control: '0-10 V',
        additionalMessage: 'Please review the operating context.',
      },
    })
  })

  it('blocks Product RFQ submission without immutable published context', () => {
    expect(() => buildRfqRequest('product', rfqContext, rfqContact)).toThrow(/published product record/i)
    expect(() => buildRfqRequest('product', rfqContext, rfqContact, {
      productId: '77935cef-4111-4c4c-bdb8-17679a8b42fe', stableId: 'AT-P-001', publishedRevision: 7,
    })).toThrow(/stable ID, model and revision/i)
  })

  it('carries the exact stable product identity returned by the published projection', () => {
    const productContext: ProductContext = {
      productId: '77935cef-4111-4c4c-bdb8-17679a8b42fe',
      stableId: 'AT-P-001', model: 'Verified model', publishedRevision: 7,
    }
    expect(buildRfqRequest('product', rfqContext, rfqContact, productContext).productContext).toEqual(productContext)
  })

  it('maps project and replacement journey fields without inventing values', () => {
    const project = buildRfqRequest('project', {
      ...rfqContext, projectStage: 'Engineering', projectScale: '12 air handlers',
      schedule: 'Prototype in Q4', engineeringNeeds: 'Acoustic review',
    }, rfqContact)
    expect(project.context).toMatchObject({
      projectStage: 'Engineering', projectScale: '12 air handlers',
      schedule: 'Prototype in Q4', engineeringNeeds: 'Acoustic review',
    })

    const replacement = buildRfqRequest('replacement', {
      ...rfqContext, existingModel: 'Legacy nameplate', installationConstraints: 'Existing cutout',
      replacementGoal: 'Reduce service downtime',
    }, rfqContact)
    expect(replacement.context).toMatchObject({
      existingModel: 'Legacy nameplate', installationConstraints: 'Existing cutout',
      replacementGoal: 'Reduce service downtime',
    })
  })

  it('restores only version 2 string fields and safely ignores old session data', () => {
    expect(parseRfqSession(serializeRfqSession(rfqContext))).toEqual(rfqContext)
    expect(parseRfqSession(JSON.stringify({ version: 1, context: rfqContext }))).toBeUndefined()
    expect(parseRfqSession('{invalid')).toBeUndefined()
    expect(parseRfqSession(JSON.stringify({
      version: 2, context: { application: 'Ventilation', quantity: 4, unknown: 'ignored' },
    }))).toEqual({ application: 'Ventilation' })
  })
})
