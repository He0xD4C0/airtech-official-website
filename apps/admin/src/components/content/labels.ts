import type { CmsBodyPolicy, CmsContentKind, ContentBlockKind, ContentTemplateKey } from '@airtek/contracts'

/** Shared CMS labels for the Admin editor. Single source so inspector panels and lists agree. */
export const contentKindLabels: Record<CmsContentKind, string> = {
  home: '首页',
  page: '页面',
  solution: '解决方案',
  technology: '技术',
  article: '文章',
  news: '新闻',
  faq: 'FAQ',
  caseStudy: '案例',
  download: '下载',
  company: '公司',
  legal: '法律文本',
  generalInformation: 'General Information',
  navigation: '导航',
  footer: '页脚',
}

export const blockKindLabels: Record<ContentBlockKind, string> = {
  hero: 'Hero 首屏',
  body: '正文',
  media: '媒体',
  featureGrid: '特性网格',
  evidence: '证据',
  cta: 'CTA',
  relationCollection: '关联集合',
  faqCollection: 'FAQ 集合',
  downloadAsset: '下载资源',
  contactBlock: '联系块',
}

export const contentStatusLabels: Record<string, string> = {
  draft: '草稿',
  published: '已发布',
  archived: '已归档',
}

export const contentTemplateLabels: Record<ContentTemplateKey, string> = {
  home: '网站首页',
  productIndex: '产品总览',
  productFamily: '产品家族页',
  selector: '产品选型器',
  compare: '产品对比页',
  solutionIndex: '解决方案总览',
  solutionDetail: '解决方案详情',
  technologyIndex: '技术能力总览',
  technologyDetail: '技术能力详情',
  articleIndex: '文章中心',
  articleDetail: '文章详情',
  newsIndex: '新闻中心',
  newsDetail: '新闻详情',
  faqIndex: '常见问题总览',
  faqDetail: '常见问题详情',
  caseStudyIndex: '案例中心',
  caseStudyDetail: '案例详情',
  downloadIndex: '下载中心',
  downloadDetail: '下载详情',
  about: '关于 AIRTEKPOWER',
  contact: '联系页面',
  rfqRouter: '询价入口',
  rfqForm: '询价表单页',
  search: '站内搜索页',
  legal: '法律文本页',
  navigation: '全站导航配置',
  footer: '全站页脚配置',
  generalInformation: '企业基本信息',
}

export const revisionKindLabels: Record<string, string> = {
  manual: '手动快照',
  publish: '发布',
  restore: '恢复',
}

export const bodyPolicyLabels: Record<CmsBodyPolicy, string> = {
  required: '必需正文',
  optional: '正文可选',
  forbidden: '无正文',
}

export function bodyPolicyLabel(policy: CmsBodyPolicy): string {
  return bodyPolicyLabels[policy] ?? policy
}

/** Tolerant lookup for API values that arrive as plain strings. */
export function contentKindLabel(value: string): string {
  return contentKindLabels[value as CmsContentKind] ?? value
}
