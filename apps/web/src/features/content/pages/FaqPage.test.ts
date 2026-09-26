import { createSSRApp, h } from 'vue'
import { renderToString } from 'vue/server-renderer'
import { describe, expect, it } from 'vitest'
import { publicPageFixture } from '@/shared/test/publicPageFixture'
import { publicProjectionFixture, tiptapDocument } from '@/shared/test/publicProjectionFixture'
import FaqPage from '@/features/content/pages/FaqPage.vue'
import { createPublicTestPlugins } from '@/shared/test/publicAppPlugins'

describe('native V2 FAQ page', () => {
  it('SSR-renders the V2 body and typed question-and-answer collection', async () => {
    const body = tiptapDocument([
      { type: 'paragraph', content: [{ type: 'text', text: 'Published V2 introduction.' }] },
    ])
    const projection = publicProjectionFixture({
      kind: 'faq',
      templateKey: 'faqDetail',
      slug: 'technical',
      title: 'Technical FAQ',
      body,
      composition: {
        blocks: [
          { type: 'body', id: '11111111-1111-4111-8111-111111111111', width: 'standard' },
          { type: 'faqCollection', id: '22222222-2222-4222-8222-222222222222', heading: 'Questions' },
        ],
      },
      typeFields: {
        type: 'faq',
        items: [{
          id: '33333333-3333-4333-8333-333333333333',
          question: 'Which context is visible?',
          answer: tiptapDocument([
            { type: 'paragraph', content: [{ type: 'text', text: 'Only the answer in the V2 projection.' }] },
          ]),
        }],
      },
    })
    const page = publicPageFixture('/en/resources/faqs/technical', {
      projection, dataState: 'published', indexable: true,
    })
    const app = createSSRApp({ render: () => h(FaqPage, { page }) })
    for (const plugin of createPublicTestPlugins()) app.use(plugin)
    const html = await renderToString(app)

    expect(html).toContain('Published V2 introduction.')
    expect(html).toContain('<details>')
    expect(html).toContain('Which context is visible?')
    expect(html).toContain('Only the answer in the V2 projection.')
    expect(html).not.toContain('Why do operating conditions matter for a PQ curve?')
  })
})
