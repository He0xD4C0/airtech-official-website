# Delivery decisions and unresolved scope

Last reviewed: 2026-09-26

The supplied DOCX/XLSX plan is historical planning evidence. It did not itself approve a code stack, product master, signed scope, final schedule, or deployment decision, so do not convert its examples into project constants.

## Confirmed repository baseline

The implemented platform boundary is current repository fact, not a claim inferred from the planning files. Use the root [README](../../../../README.md), [platform architecture](../../../../ARCHITECTURE.md), and [production deployment contract](../../../../infra/deploy/README.md) as the live implementation sources before changing or reporting it. They currently establish Vue public/Admin applications, a Rust/Axum API and Worker, PostgreSQL runtime access through SQLx, Flyway as the sole schema-version owner, unified CMS V2, and the validated Product Master publication boundary. Re-inspect those files because implementation details can change.

The first-production cloud provider and region are approved below. Production
domain, service sizing, current price, final schedule, signed scope, live
external adapters, object-storage provider, and production launch remain
unapproved or environment-specific unless a later owner-approved record says
otherwise.

### Approved first-production provider and region — 2026-09-26

Status: `VERIFIED` by the owner's explicit 2026-09-26 decision. Use Alibaba
Cloud ECS in Singapore with the repository's implemented single-host,
separate-infrastructure/application Compose topology. This decision does not
approve an instance size, price, production domain, object-storage provider, or
production-launch status. Any change to the provider or region requires a new
explicit owner decision.

### Approved directory contract — 2026-09-26

Status: `VERIFIED` for the owner's explicit structure-only refactor decision and
its repository implementation; this is not production-launch approval. Admin
and Web implementations are feature-owned, Web retains its Vike route adapters,
and Platform uses domain/runtime/http/jobs crates with thin binary packages.
HTTP contracts, schema history, routes and image responsibilities are unchanged.
Use root pnpm entrypoints listed in the live README for tools now grouped under
`scripts/{checks,generation,testing,maintenance,brand}` and Compose variants in
`infra/compose`. `pnpm check:architecture` verifies directory/dependency
boundaries; `pnpm check:compose` validates the configurations. `.local/qa` may be
cleaned, but `.local/deliverables` must never be automatically cleaned. Brand
source evidence, approval decisions and approved assets remain in their original
locations. Reinspect the linked live implementation sources before reporting
test status or deployment readiness.

## Confirm before implementation or estimate

1. Requested increment and acceptance boundary across the implemented public site, core APIs, and internal management platform.
2. Product/content owners, source systems, approval workflow, locales, and migration volume.
3. Any proposed change to the implemented framework/runtime, database, search, CMS/Admin approach, identity, email, file processing, and integration constraints.
4. Target markets, data residency, privacy/legal requirements, accessibility target, and security expectations.
5. Any proposed change to the approved provider or region, plus network/CDN/DNS, object storage, backups, monitoring, recovery objectives, environments, and support model.
6. Product-selector engineering ownership and the authoritative curves/specifications needed for acceptance.
7. RFQ routing, business-system handoff, notification ownership, spam controls, retention, and service expectations.
8. SEO baseline, redirects/domain transition, analytics consent, success metrics, and launch readiness.

## Recorded conflicts

- Historical deployment conflict: growth DOCX text-order paragraphs 903 and 906 name Alibaba Cloud Hong Kong with 2 vCPU/4 GB and 40–60 GB plus CDN/object storage; procurement workbook `Sheet1!C3:C5` names Singapore with 2 vCPU/4 GB/50 GB, Singapore 500 GB object storage, and a non-mainland-China CDN. The provider/region conflict is `DEPRECATED` after the 2026-09-26 decision selecting Alibaba Cloud ECS Singapore. The source instance sizes, storage/CDN products, and prices remain `PROVISIONAL` and date-sensitive.
- Storage wording: “Alibaba Cloud S3” is incorrect as a product name. Use Alibaba Cloud OSS unless an S3-compatible API is explicitly selected.
- Staging: rebuild workbook `总览!A4:E11` uses stages 1–8 while the detailed sheet names/titles are `阶段0!A1`, `阶段3!A1`, `阶段4!A1`, `阶段6!A1`, and `阶段7!A1`. Work is shifted or missing between them.
- Duration: `总览!A7:E7` assigns three weeks to the product portal, while `阶段3!A1` labels substantially the same capability as three days. Status: `CONFLICTED`.
- Historical scope conflict: `阶段0!A7:D9` includes management-backend pages/interfaces and MCP implementation, while the strategy DOCX framed the portal as a separate second phase; `总览!A9:E9` also placed the management platform later. The repository now contains the management platform, so these stage labels are not current implementation scope. MCP and other external AI capabilities remain separately unapproved.

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
5. Conversion: four RFQ paths, routing, business handoff, privacy and anti-spam; add uploads only after a separately approved security and retention design.
6. Launch: redirects, performance/accessibility/security QA, analytics consent, backups/recovery, operational runbook.
7. Optional extensions: approved AI/MCP capabilities over controlled APIs and other explicitly scoped platform additions.

Estimate only after dependencies and acceptance tests exist. Keep options and tradeoffs visible when the owner has not chosen.

## Prototype gap

Neither HTML demo defines real search logic, authoritative data binding, dynamic comparison, functional downloads, persistent forms, backend validation, storage, notification, authentication, accessibility acceptance, analytics contracts, or production error handling. The current repository implements some of these capabilities independently of the demos; inspect the live implementation and tests rather than treating either the demos or this historical gap list as current status.
