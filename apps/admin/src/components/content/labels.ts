import type { CmsContentKind, ContentBlockKind } from '@airtek/contracts'

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
  scheduled: '计划发布',
  published: '已发布',
  archived: '已归档',
}

export const revisionKindLabels: Record<string, string> = {
  manual: '手动快照',
  publish: '发布',
  restore: '恢复',
}

/** Tolerant lookup for API values that arrive as plain strings. */
export function contentKindLabel(value: string): string {
  return contentKindLabels[value as CmsContentKind] ?? value
}
