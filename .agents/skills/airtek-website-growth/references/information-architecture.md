# Information architecture

Updated: 2026-09-01

## Strategic objective

The supplied growth plan proposes the search-facing positioning `Industrial Air Movement & EC Fan Engineering Manufacturer`. Status: `PROVISIONAL` strategy, not a legal company name or approved replacement for AIRTEKPOWER messaging.

The normalized site objective is:

`Relevant search/referral traffic → useful technical and application content → product discovery, selection, or comparison → contextual RFQ → qualified lead`

Measure qualified progression and inquiry quality, not traffic alone.

## Proposed top-level information architecture

1. Home
2. Products
   - Fan Selector
   - Product categories and families
   - Catalogs/download entry points
3. Solutions
   - HVAC
   - Refrigeration
   - Data centers
   - Energy storage
   - Air purification
   - Cleanroom
   - Industrial ventilation
   - Commercial buildings
4. Technology
   - EC motor
   - Aerodynamics
   - Airflow and pressure
   - Control
   - Efficiency
   - Noise and vibration
5. Resources
   - Technical articles
   - Downloads: CAD, PDFs, manuals, and approved certificates
   - FAQs
   - Case studies
6. Company
   - About
   - Contact
7. Request a Quote

Status: `PROVISIONAL` normalized IA from the strategy draft. Final labels, slugs, locales, product membership, and resource availability require content and technical validation. The source's damaged indentation must not make Industrial Ventilation or Commercial Buildings children of Cleanroom.

## Route conventions

- Use stable, readable, lowercase routes and one canonical URL per content entity.
- The source alternates between `/tech/` and “Technology”; choose one final route before implementation and redirect any replaced route.
- Separate category, product, solution, article, FAQ, case, download, company, and RFQ content types even when a frontend template is shared.
- Plan locale strategy, translations, canonical/hreflang rules, and fallback behavior before duplicating routes.
- Generate navigation, breadcrumbs, sitemaps, related-content slots, and redirects from the content model rather than scattered hard-coded arrays.

## Content relationship model

Support explicit many-to-many relationships among:

- Product families and models.
- Applications and solutions.
- Technologies and engineering topics.
- Articles, FAQs, cases, and downloads.
- RFQ entry points and prefill context.

A product page can link to applicable solutions, technical explanations, approved cases, FAQs, downloads, related products, and an RFQ. A solution page can link back to products, selection guidance, cases, articles, and FAQs. Store relationships explicitly and let editors review them; do not generate claims by keyword coincidence.

## Page contracts

Each index/detail page should define purpose, audience and search intent, canonical data owner, required/optional fields, related-content slots, primary CTA, empty/error state, structured-data eligibility, analytics events, accessibility behavior, locale behavior, and acceptance tests.

## Scope boundary

The working interpretation of the strategy is a public website plus controlled core-platform interfaces. A full internal management portal is a later independent phase. This boundary is `PROVISIONAL` because the rebuild spreadsheet also places substantial portal/MCP work in an early stage; confirm scope before implementation.
