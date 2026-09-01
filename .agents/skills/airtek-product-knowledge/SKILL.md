---
name: airtek-product-knowledge
description: >-
  Use AIRTEKPOWER's normalized fan taxonomy, application domains, engineering
  capabilities, family-level ranges, testing evidence, case-study facts, and product-data
  quality rules. Use for fan/product solutions, engineering technical content, selectors,
  comparisons, SKU schemas, datasheets, catalogs, case studies, or product SEO. Exact model values require a
  validated Product Master or official datasheet; demo HTML is never authoritative.
metadata:
  version: "1.0.0"
---

# AIRTEKPOWER Product Knowledge

Use product-family knowledge to structure and explain the offer, but do not manufacture exact SKU facts from marketing decks or prototypes.

## Route the task

- Read [taxonomy-and-capabilities.md](references/taxonomy-and-capabilities.md) for product families, applications, engineering, manufacturing, and testing.
- Read [product-data-rules.md](references/product-data-rules.md) for models, specifications, units, filters, PQ curves, downloads, selector/comparison logic, or data import.
- Read [cases-and-claims.md](references/cases-and-claims.md) for solution proof, case studies, performance claims, delivery claims, or competitor comparisons.
- Also load `$airtek-brand` for publication-ready company or marketing copy.
- Also load `$airtek-website-growth` for page structure, SEO mechanics, RFQ handoff, analytics, or platform integration.
- Load `$airtek-knowledge-governance` when sources conflict or knowledge must be changed.

## Hard rules

- Distinguish fan form, impeller geometry, inlet arrangement, motor technology, voltage/control, and application; do not flatten them into one category list.
- Treat family ranges as capability context, not individual model specifications.
- Preserve original units and operating conditions; normalize units explicitly and never confuse airflow with pressure or input power with motor nameplate output.
- A selector must filter hard constraints before ranking preferences and must explain why a candidate matched or failed.
- Do not hard-code match percentages, comparison rows, curve points, downloads, certifications, availability, or RFQ responses.
- Never copy exact values from either HTML demo into production content or data.
- Do not provide installation, wiring, safety, or compliance instructions without an approved manual for the exact model.

## Output check

For every numeric product statement, verify model or family scope, unit, operating point, tolerance/test method, source status, and whether the claim is safe to publish.
