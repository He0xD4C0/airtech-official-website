import { describe, expect, it } from 'vitest'
import type { CmsContentKind, ContentTemplateDefinition } from '@airtek/contracts'
import { defaultTypeFields, draftFromTemplate, requiredBlocksForTemplate } from '@/features/content/services/contentDraftDefaults'

const ALL_KINDS: CmsContentKind[] = [
  'home',
  'page',
  'solution',
  'technology',
  'article',
  'news',
  'faq',
  'caseStudy',
  'download',
  'company',
  'legal',
  'generalInformation',
  'navigation',
  'footer',
]

function template(overrides: Partial<ContentTemplateDefinition> = {}): ContentTemplateDefinition {
  return {
    key: 'articleDetail',
    contentKind: 'article',
    bodyPolicy: 'required',
    requiredBlocks: ['hero', 'body'],
    allowedBlocks: ['hero', 'body', 'media', 'cta'],
    routable: true,
    routePattern: '/{locale}/resources/articles/{slug}',
    singletonPerLocale: false,
    ...overrides,
  }
}

describe('content draft defaults', () => {
  it('produces a type-fields discriminator for every content kind', () => {
    const discriminators = ALL_KINDS.map((kind) => defaultTypeFields(kind).type)
    expect(discriminators).toEqual(ALL_KINDS)
  })

  it('seeds only the required regions that do not need an existing asset', () => {
    const blocks = requiredBlocksForTemplate(template(), 'Title')
    expect(blocks.map((block) => block.type)).toEqual(['hero', 'body'])

    const downloadBlocks = requiredBlocksForTemplate(
      template({ key: 'downloadDetail', contentKind: 'download', requiredBlocks: ['hero', 'downloadAsset'] }),
      'Title',
    )
    expect(downloadBlocks.map((block) => block.type)).toEqual(['hero'])
  })

  it('keeps non-routable singleton templates free of slug and body', () => {
    const draft = draftFromTemplate({
      template: template({
        key: 'navigation',
        contentKind: 'navigation',
        bodyPolicy: 'forbidden',
        requiredBlocks: [],
        allowedBlocks: [],
        routable: false,
        routePattern: null,
        singletonPerLocale: true,
      }),
      title: 'Navigation',
      slug: 'must-be-dropped',
    })
    expect(draft.slug).toBeNull()
    expect(draft.body).toBeNull()
    expect(draft.schemaVersion).toBe(2)
    expect(draft.templateKey).toBe('navigation')
    expect(draft.isPlaceholder).toBe(true)
  })

  it('starts routable drafts at draft version 1 with the requested slug', () => {
    const draft = draftFromTemplate({ template: template(), title: 'Article', slug: 'my-article' })
    expect(draft.draftVersion).toBe(1)
    expect(draft.slug).toBe('my-article')
    expect(draft.composition.blocks.map((block) => block.type)).toEqual(['hero', 'body'])
    expect(new Set(draft.composition.blocks.map((block) => block.id)).size).toBe(2)
  })
})
