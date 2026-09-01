# Data, SEO, analytics, and integration

Updated: 2026-09-01

## Product and content data integration

The canonical import, validation, review, and publishing lifecycle lives in [product data rules](../../airtek-product-knowledge/references/product-data-rules.md#import-and-publishing). This website layer consumes only approved published records and assets; it must not bypass staging or treat Feishu/Excel rows as live public data.

Keep content, relationships, routes, metadata, and editorial workflow distinct from exact product master fields while maintaining stable IDs between them. Integration transport must be authenticated, idempotent, observable, retryable, and auditable. Define ownership, field mapping, deletion/unpublish semantics, conflict behavior, rate limits, partial-failure handling, and rollback before scheduling a sync.

## Search foundation

- Configure Google Search Console and Bing Webmaster Tools for the verified production domain.
- Generate XML sitemap indexes from canonical, indexable published records.
- Use the correct filename `robots.txt`; do not block assets required for rendering.
- Provide unique titles/descriptions where appropriate, canonical URLs, redirects, status codes, structured headings, crawlable links, and intentional index/noindex decisions.
- Plan language/region URLs and hreflang only when translations and canonical strategy are real.
- Build internal links from explicit product/solution/technology/resource relationships.
- Track index coverage, crawl issues, queries, qualified landings, product engagement, downloads, selector use, and RFQ progression.

Sitemaps and URL submission support discovery; they do not guarantee immediate crawling, indexing, exposure, or ranking. Never make ranking guarantees.

## Structured data

Add schema only when visible page content and source data support it. Potential types include Organization, WebSite, BreadcrumbList, Product, Article, FAQPage, and VideoObject where applicable. Validate required/recommended fields, model offers carefully, and do not invent ratings, price, availability, certifications, or FAQs solely to obtain rich results.

## Editorial workflow

Use search/market research to inform an editorial brief, then require technical evidence, brand/product review, human editing, approval, publication, measurement, and scheduled refresh. AI can assist research and drafting but must not autonomously scrape “hot keywords” and publish news or technical claims.

## Analytics and privacy

The plan mentions Google Analytics, Google Tag Manager, and Microsoft Clarity. Their use is `PROVISIONAL` pending target markets, consent design, privacy review, retention, and vendor configuration.

Maintain an approved event dictionary. Candidate event families include page/content view, internal search, filter/select/compare, download, selector step/result, FAQ expand, CTA, RFQ route/start/upload/submit/error/abandonment, and outbound contact actions.

- Collect only properties needed for a stated measurement purpose.
- Separate anonymous/session analytics identifiers from RFQ/lead records and direct personal data.
- Link anonymous activity to a lead only after a user submits, for a declared purpose, with appropriate permissions and retention.
- Do not send form contents, personal details, confidential product requirements, or upload data to analytics vendors.
- Respect consent and regional requirements before enabling optional measurement, replay, or advertising features.

## API and MCP boundary

Public/internal APIs need authentication where appropriate, authorization scopes, input/schema validation, rate limits, pagination/filter bounds, audit logs, error contracts, secrets management, and least-privilege data access.

MCP or other AI integrations may use a controlled Data API with purpose-specific permissions and auditable operations. Never provide direct production-database credentials or unrestricted file/object-storage access. Treat write operations, publishing, and personal/business data as separate high-trust capabilities.
