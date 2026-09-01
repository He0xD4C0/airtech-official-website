# Delivery decisions and unresolved scope

Updated: 2026-09-01

The supplied plan contains useful capability ideas but no agreed code stack, product master, signed scope, final schedule, or deployment decision. Do not convert its examples into project constants.

## Confirm before implementation or estimate

1. Phase scope: public site only, public site plus core APIs, or also an internal management platform.
2. Product/content owners, source systems, approval workflow, locales, and migration volume.
3. Framework/runtime, database, search, CMS/admin approach, identity, email, file processing, and integration constraints.
4. Target markets, data residency, privacy/legal requirements, accessibility target, and security expectations.
5. Cloud provider and region, network/CDN/DNS, object storage, backups, monitoring, recovery objectives, environments, and support model.
6. Product-selector engineering ownership and the authoritative curves/specifications needed for acceptance.
7. RFQ routing, business-system handoff, notification ownership, spam controls, retention, and service expectations.
8. SEO baseline, redirects/domain transition, analytics consent, success metrics, and launch readiness.

## Recorded conflicts

- Deployment: growth DOCX text-order paragraphs 903 and 906 name Alibaba Cloud Hong Kong with 2 vCPU/4 GB and 40–60 GB plus CDN/object storage; procurement workbook `Sheet1!C3:C5` names Singapore with 2 vCPU/4 GB/50 GB, Singapore 500 GB object storage, and a non-mainland-China CDN. Status: `CONFLICTED` and date-sensitive.
- Storage wording: “Alibaba Cloud S3” is incorrect as a product name. Use Alibaba Cloud OSS unless an S3-compatible API is explicitly selected.
- Staging: rebuild workbook `总览!A4:E11` uses stages 1–8 while the detailed sheet names/titles are `阶段0!A1`, `阶段3!A1`, `阶段4!A1`, `阶段6!A1`, and `阶段7!A1`. Work is shifted or missing between them.
- Duration: `总览!A7:E7` assigns three weeks to the product portal, while `阶段3!A1` labels substantially the same capability as three days. Status: `CONFLICTED`.
- Scope: `阶段0!A7:D9` includes management-backend pages/interfaces and MCP implementation, while the strategy DOCX frames the portal as a separate second phase; `总览!A9:E9` also places the management platform later. Working interpretation is public site plus controlled core interfaces first, but owner confirmation is required.

## Procurement snapshot

The visible line-item arithmetic is:

- Server: CNY 112 × 3 = CNY 336.
- Object storage: CNY 70 × 3 = CNY 210.
- CDN: CNY 56 × 1 = CNY 56.
- ChatGPT Plus: CNY 1,344.40 × 1.
- Arithmetic total: CNY 1,946.40.

Inputs are in procurement workbook `Sheet1!E3:F6`; the displayed total formula is `G1` and row formulas are `H3:H6`. Those formula cells parse/render as `#NAME?` even though the visible inputs yield the arithmetic above. The workbook omits or does not settle domain/DNS, backups, monitoring, transactional email, CAPTCHA/anti-spam, environments, security, support, taxes, bandwidth/egress, and other operating costs. Prices, allowances, products, currency, and cloud sizing are `PROVISIONAL` dated snapshots and must be refreshed from current primary sources before any purchase or estimate.

## Delivery model

Replace the conflicting stage numbers with capability slices and explicit dependencies:

1. Discovery and decisions: scope, data, content, brand assets, stack, hosting, compliance, acceptance criteria.
2. Foundation: environments, data models, CMS/product ingestion, design system, routing, security, observability.
3. Public discovery: IA, category/solution/resource/company pages, search foundation, content migration.
4. Product experience: validated product detail, assets, search/filter, selector and comparison where data supports them.
5. Conversion: four RFQ paths, uploads, routing, business handoff, privacy and anti-spam.
6. Launch: redirects, performance/accessibility/security QA, analytics consent, backups/recovery, operational runbook.
7. Later platform: independent management portal and approved AI/MCP capabilities over controlled APIs.

Estimate only after dependencies and acceptance tests exist. Keep options and tradeoffs visible when the owner has not chosen.

## Prototype gap

Neither HTML demo has real search logic, authoritative data binding, dynamic comparison, functional downloads, persistent forms, backend validation, storage, notification, authentication, accessibility acceptance, analytics contract, or production error handling. Use them only to discuss interaction ideas.
