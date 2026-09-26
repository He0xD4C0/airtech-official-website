---
name: airtek-website-growth
description: >-
  Plan or implement AIRTEKPOWER's B2B website growth system: information architecture,
  product discovery, SEO and content relationships, selection and comparison, RFQ flows,
  analytics, Feishu sync, APIs, platform boundaries, and delivery decisions. Use for website
  requirements, routes, page flows, CMS/data models, lead generation, deployment, or roadmap
  work. Do not treat draft schedules, cloud choices, costs, or HTML demos as approved behavior.
metadata:
  version: "1.1.0"
---

# AIRTEKPOWER Website Growth

Turn the supplied strategy into an evidence-led B2B product-discovery and inquiry system. Separate durable capability contracts from disputed implementation choices.

## Route the task

- Read [information-architecture.md](references/information-architecture.md) for positioning, sitemap, routes, navigation, page types, CMS relationships, and public-site scope.
- Read [conversion-and-product-experience.md](references/conversion-and-product-experience.md) for product detail, selector, comparison, FAQ, Contact, RFQ, uploads, and lead handoff.
- Read [data-seo-and-analytics.md](references/data-seo-and-analytics.md) for SEO, structured data, internal links, product/content data flows, APIs, analytics, consent, or MCP.
- Read [delivery-decisions.md](references/delivery-decisions.md) before estimating, changing the approved stack, deploying, purchasing services, or extending the management platform.
- Also load `$airtek-brand` for identity, public copy, and visual decisions.
- Also load `$airtek-product-knowledge` for taxonomy, exact specifications, selector rules, engineering statements, and cases.
- Load `$airtek-knowledge-governance` when changing canonical knowledge or resolving a source conflict.

## Hard rules

- Optimize the journey `search or referral → useful technical/product content → discovery or selection → qualified RFQ`, not raw traffic or page count alone.
- Keep product facts in a validated product-data source; keep content relations and page presentation in the CMS/application layer.
- Do not promise rankings, recrawl timing, autonomous AI publishing, selector accuracy without data, or business outcomes without evidence.
- Treat the HTML files as UX sketches, not implemented behavior, data, styling, or acceptance tests.
- Keep optional analytics consent-aware and data-minimized. Do not put RFQ contents or direct personal identifiers into analytics event properties.
- Give public clients and MCP controlled APIs, authentication, authorization, rate limits, and auditability; never direct production-database access.
- Use the owner-approved first-production provider/region recorded in
  [delivery decisions](references/delivery-decisions.md). Do not hard-code an
  unapproved instance size, cost, domain, object-storage provider, launch status,
  or schedule, and require a new explicit decision before changing the approved
  provider or region. Preserve the repository's implemented framework and
  platform boundaries unless the user explicitly approves a change.

## Definition of done

Tie each implementation decision to a normalized requirement, authoritative data source, validation and error behavior, accessibility/privacy expectations, measurable event, and acceptance test. Surface unresolved decisions explicitly.
