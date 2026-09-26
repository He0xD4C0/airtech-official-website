# Conflict and correction register

Last reviewed: 2026-09-26

This is a cross-domain index of known traps. The linked domain reference is the canonical record; details are not copied here so they cannot drift independently.

## Brand and company

| Issue | Status | Canonical record |
|---|---|---|
| Legacy company/brand names remain in the visual manual | `DEPRECATED` | [Brand identity and copy](../../airtek-brand/references/identity-and-copy.md) and the hard rules in `$airtek-brand` |
| The palette prose mislabels the green swatch, and the two demos introduce unrelated blues | `DEPRECATED` | [Brand visual system](../../airtek-brand/references/visual-system.md) |
| `PANTONE 166C` and CMYK/spot-color guidance are not reconciled to the canonical RGB palette | `CONFLICTED` | [Brand visual system](../../airtek-brand/references/visual-system.md) |
| Latin-font guidance names two inconsistent families | `CONFLICTED` | [Brand visual system](../../airtek-brand/references/visual-system.md) |
| Source emails contain whitespace; static copyright has an expired end year | `DEPRECATED` | [Brand identity and copy](../../airtek-brand/references/identity-and-copy.md) and [visual system](../../airtek-brand/references/visual-system.md) |
| Standalone `Airtek`, `艾特克（中国）`, and the logo subline lack an approved naming/slogan rule | `PROVISIONAL` or `DEPRECATED` for legal use | [Brand identity and copy](../../airtek-brand/references/identity-and-copy.md) |
| Three responsive legacy-site logo crops contain only the graphic mark, while the controlled manual requires a fixed graphic-plus-wordmark construction | `DEPRECATED` for logo use; `BLOCKED` from publication | [Brand asset governance](../../airtek-brand/references/brand-assets.md) and [brand visual system](../../airtek-brand/references/visual-system.md) |
| Personal example contacts, company counts, facility/output figures, and certifications lack current approval or complete scope | `PROVISIONAL` | [Brand claim guardrails](../../airtek-brand/references/claim-guardrails.md) |

## Product data and evidence

| Issue | Status | Canonical record |
|---|---|---|
| The two HTML demos give mutually inconsistent specifications for the same model | `DEPRECATED` as a live conflict after the 2026-09-02 owner-approved Product Master snapshot; both demo sets remain prohibited. Use only a published record or controlled snapshot matching the registered provenance; otherwise return `Published data unavailable`. | [Product data rules](../../airtek-product-knowledge/references/product-data-rules.md#known-demo-conflict) |
| Selector, match score, comparison, download, curve, and RFQ behaviors are hard-coded or placeholders | `DEPRECATED` as production behavior | [Product data rules](../../airtek-product-knowledge/references/product-data-rules.md) and [conversion experience](../../airtek-website-growth/references/conversion-and-product-experience.md) |
| A flat category list mixes fan form with motor technology and other facets | `PROVISIONAL` | [Product taxonomy](../../airtek-product-knowledge/references/taxonomy-and-capabilities.md) |
| Product/application labels contain spelling errors, and `BLEC Motor` is ambiguous | `DEPRECATED` or `CONFLICTED` | [Product source terminology corrections](../../airtek-product-knowledge/references/taxonomy-and-capabilities.md#source-terminology-corrections) |
| Case economics do not reconcile; all other numeric outcomes and delivery times lack a controlled claim record | `CONFLICTED` or `PROVISIONAL` | [Cases and claims](../../airtek-product-knowledge/references/cases-and-claims.md) |

## Website, SEO, data, and delivery

| Issue | Status | Canonical record |
|---|---|---|
| Source typos include `HEVC`, `robot.txt`, `Webiste`, and `考前位置` | `DEPRECATED` | Use `HVAC`, `robots.txt`, `Website`, and `靠前位置`; SEO behavior is in [data, SEO, analytics, and integration](../../airtek-website-growth/references/data-seo-and-analytics.md) |
| “Alibaba Cloud S3” is used as a product name | `DEPRECATED` unless compatibility is intended | [Delivery decisions](../../airtek-website-growth/references/delivery-decisions.md) |
| Sitemap/push language promises recrawl, exposure, or ranking; AI content is framed as autonomous publishing | `DEPRECATED` | [Data, SEO, analytics, and integration](../../airtek-website-growth/references/data-seo-and-analytics.md) |
| Sitemap indentation is damaged and route naming is inconsistent | `CONFLICTED` | [Information architecture](../../airtek-website-growth/references/information-architecture.md) |
| Historical Hong Kong and Singapore deployment proposals conflict | `DEPRECATED` as an active conflict after the 2026-09-26 owner decision | [Delivery decisions](../../airtek-website-growth/references/delivery-decisions.md); use Alibaba Cloud ECS in Singapore for the first production deployment and do not revive the Hong Kong proposal |
| Stage numbering, duration, and portal/MCP scope disagree across planning files | `CONFLICTED` | [Delivery decisions](../../airtek-website-growth/references/delivery-decisions.md) |
| Procurement formula parsing, incomplete cost scope, SaaS allowance, and pricing are dated | `PROVISIONAL` and date-sensitive | [Delivery decisions](../../airtek-website-growth/references/delivery-decisions.md) |
| “Track every user action” conflicts with data minimization and consent-aware analytics | `DEPRECATED` | [Data, SEO, analytics, and integration](../../airtek-website-growth/references/data-seo-and-analytics.md) |

## Resolution rule

When a source reintroduces any issue above, do not paste the raw value into a deliverable. Load the linked domain Skill, apply its canonical handling, and use [source-policy.md](source-policy.md) if a new decision or source could change the record.
