import type {
  CmsBodyPolicy,
  CmsContentKind,
  ContentBlockKind,
  ContentTemplateKey,
} from '@airtek/contracts'

/**
 * Front-end mirror of the controlled template registry in
 * `services/platform/src/services/cms_templates.rs`. The Rust registry stays
 * authoritative for validation; this module only drives the Admin UI so the
 * editor never invents a template, block, or required region.
 */
export interface ContentTemplateOption {
  key: ContentTemplateKey
  contentKind: CmsContentKind
  bodyPolicy: CmsBodyPolicy
  requiredBlocks: ContentBlockKind[]
  allowedBlocks: ContentBlockKind[]
  routable: boolean
  singletonPerLocale: boolean
  label: string
  description: string
}

const PAGE_ALLOWED: ContentBlockKind[] = [
  'hero',
  'body',
  'media',
  'featureGrid',
  'evidence',
  'cta',
  'relationCollection',
]

const EDITORIAL_ALLOWED: ContentBlockKind[] = [
  ...PAGE_ALLOWED,
  'faqCollection',
  'downloadAsset',
]

const HERO_REQUIRED: ContentBlockKind[] = ['hero']
const HERO_BODY_REQUIRED: ContentBlockKind[] = ['hero', 'body']
const FAQ_REQUIRED: ContentBlockKind[] = ['hero', 'faqCollection']
const DOWNLOAD_REQUIRED: ContentBlockKind[] = ['hero', 'downloadAsset']
const CONTACT_REQUIRED: ContentBlockKind[] = ['hero', 'contactBlock']

function page(
  key: ContentTemplateKey,
  label: string,
  description: string,
  singletonPerLocale: boolean,
): ContentTemplateOption {
  return {
    key,
    contentKind: 'page',
    bodyPolicy: 'optional',
    requiredBlocks: [...HERO_REQUIRED],
    allowedBlocks: [...PAGE_ALLOWED],
    routable: true,
    singletonPerLocale,
    label,
    description,
  }
}

function editorial(
  key: ContentTemplateKey,
  contentKind: CmsContentKind,
  label: string,
  description: string,
): ContentTemplateOption {
  return {
    key,
    contentKind,
    bodyPolicy: 'required',
    requiredBlocks: [...HERO_BODY_REQUIRED],
    allowedBlocks: [...EDITORIAL_ALLOWED],
    routable: true,
    singletonPerLocale: false,
    label,
    description,
  }
}

export const contentTemplates: ContentTemplateOption[] = [
  {
    key: 'home',
    contentKind: 'home',
    bodyPolicy: 'optional',
    requiredBlocks: [...HERO_REQUIRED],
    allowedBlocks: [...PAGE_ALLOWED],
    routable: true,
    singletonPerLocale: true,
    label: 'Home',
    description: '首页单例，每语言一份。',
  },
  page('productIndex', 'Product Index', '产品中心索引页单例。', true),
  page('productFamily', 'Product Family', '产品家族详情页。', false),
  page('selector', 'Selector', '选型页单例。', true),
  page('compare', 'Compare', '对比页单例。', true),
  page('solutionIndex', 'Solution Index', '解决方案索引页单例。', true),
  editorial('solutionDetail', 'solution', 'Solution Detail', '解决方案详情，正文必需。'),
  page('technologyIndex', 'Technology Index', '技术索引页单例。', true),
  editorial('technologyDetail', 'technology', 'Technology Detail', '技术详情，正文必需。'),
  page('articleIndex', 'Article Index', '文章索引页单例。', true),
  editorial('articleDetail', 'article', 'Article Detail', '文章详情，正文必需。'),
  page('newsIndex', 'News Index', 'News 索引页单例。', true),
  editorial('newsDetail', 'news', 'News Detail', 'News 详情，正文必需。'),
  {
    key: 'faqIndex',
    contentKind: 'page',
    bodyPolicy: 'optional',
    requiredBlocks: [...HERO_REQUIRED],
    allowedBlocks: ['hero', 'body', 'cta', 'relationCollection', 'faqCollection'],
    routable: true,
    singletonPerLocale: true,
    label: 'FAQ Index',
    description: 'FAQ 索引页单例。',
  },
  {
    key: 'faqDetail',
    contentKind: 'faq',
    bodyPolicy: 'forbidden',
    requiredBlocks: [...FAQ_REQUIRED],
    allowedBlocks: ['hero', 'cta', 'relationCollection', 'faqCollection'],
    routable: true,
    singletonPerLocale: false,
    label: 'FAQ Detail',
    description: 'FAQ 详情，正文由问答条目组成。',
  },
  page('caseStudyIndex', 'Case Study Index', '案例索引页单例。', true),
  editorial('caseStudyDetail', 'caseStudy', 'Case Study Detail', '案例详情，正文必需。'),
  page('downloadIndex', 'Download Index', '下载索引页单例。', true),
  {
    key: 'downloadDetail',
    contentKind: 'download',
    bodyPolicy: 'optional',
    requiredBlocks: [...DOWNLOAD_REQUIRED],
    allowedBlocks: [...EDITORIAL_ALLOWED],
    routable: true,
    singletonPerLocale: false,
    label: 'Download Detail',
    description: '受控下载详情，必须引用受控文件区块。',
  },
  editorial('about', 'company', 'About', '公司介绍页。'),
  {
    key: 'contact',
    contentKind: 'company',
    bodyPolicy: 'optional',
    requiredBlocks: [...CONTACT_REQUIRED],
    allowedBlocks: ['hero', 'body', 'media', 'cta', 'contactBlock'],
    routable: true,
    singletonPerLocale: true,
    label: 'Contact',
    description: '联系页单例，必须包含联系区块。',
  },
  page('rfqRouter', 'RFQ Router', 'RFQ 路由页单例。', true),
  page('rfqForm', 'RFQ Form', 'RFQ 表单页。', false),
  {
    key: 'search',
    contentKind: 'page',
    bodyPolicy: 'forbidden',
    requiredBlocks: [...HERO_REQUIRED],
    allowedBlocks: ['hero'],
    routable: true,
    singletonPerLocale: true,
    label: 'Search',
    description: '站内搜索页单例。',
  },
  {
    key: 'legal',
    contentKind: 'legal',
    bodyPolicy: 'required',
    requiredBlocks: ['body'],
    allowedBlocks: ['hero', 'body'],
    routable: true,
    singletonPerLocale: false,
    label: 'Legal',
    description: '法律条款页，正文必需。',
  },
  {
    key: 'navigation',
    contentKind: 'navigation',
    bodyPolicy: 'forbidden',
    requiredBlocks: [],
    allowedBlocks: [],
    routable: false,
    singletonPerLocale: true,
    label: 'Navigation',
    description: '全站导航单例，不参与公开路由。',
  },
  {
    key: 'footer',
    contentKind: 'footer',
    bodyPolicy: 'forbidden',
    requiredBlocks: [],
    allowedBlocks: [],
    routable: false,
    singletonPerLocale: true,
    label: 'Footer',
    description: '全站页脚单例，不参与公开路由。',
  },
  {
    key: 'generalInformation',
    contentKind: 'generalInformation',
    bodyPolicy: 'forbidden',
    requiredBlocks: [],
    allowedBlocks: [],
    routable: false,
    singletonPerLocale: true,
    label: 'General Information',
    description: '全站信息单例，不参与公开路由。',
  },
]

const templatesByKey = new Map(contentTemplates.map((template) => [template.key, template]))

export function templateDefinition(key: ContentTemplateKey): ContentTemplateOption | undefined {
  return templatesByKey.get(key)
}

export function templatesForKind(kind: CmsContentKind): ContentTemplateOption[] {
  return contentTemplates.filter((template) => template.contentKind === kind)
}

export function isRequiredBlock(template: ContentTemplateOption, block: ContentBlockKind): boolean {
  return template.requiredBlocks.includes(block)
}

const contentKindLabels: Record<CmsContentKind, string> = {
  home: 'Home',
  page: '页面',
  solution: 'Solution',
  technology: 'Technology',
  article: 'Article',
  news: 'News',
  faq: 'FAQ',
  caseStudy: 'Case Study',
  download: 'Download',
  company: 'Company',
  legal: 'Legal',
  generalInformation: 'General Information',
  navigation: 'Navigation',
  footer: 'Footer',
}

const blockKindLabels: Record<ContentBlockKind, string> = {
  hero: 'Hero',
  body: '正文',
  media: '媒体',
  featureGrid: '特性网格',
  evidence: '证据与数据',
  cta: '行动按钮',
  relationCollection: '关联集合',
  faqCollection: 'FAQ 集合',
  downloadAsset: '下载文件',
  contactBlock: '联系信息',
}

const bodyPolicyLabels: Record<CmsBodyPolicy, string> = {
  required: '正文必需',
  optional: '正文可选',
  forbidden: '正文禁用',
}

export function contentKindLabel(kind: CmsContentKind): string {
  return contentKindLabels[kind] ?? kind
}

export function blockKindLabel(block: ContentBlockKind): string {
  return blockKindLabels[block] ?? block
}

export function bodyPolicyLabel(policy: CmsBodyPolicy): string {
  return bodyPolicyLabels[policy] ?? policy
}

/** Templates that the Admin UI can create. Every registry entry is creatable. */
export function creatableTemplates(): ContentTemplateOption[] {
  return contentTemplates
}
