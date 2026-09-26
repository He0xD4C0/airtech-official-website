# Conversion and product experience

Last reviewed: 2026-09-26

Status: the source proposal remains `PROVISIONAL` for unimplemented details, but
the current repository has `VERIFIED` implementations for database-backed
product detail, anonymous selector and comparison, four RFQ journeys, and the
Admin business inbox. Treat the live README, architecture, OpenAPI, and tests as
the implementation contract; the controls below remain guardrails for any
extension.

## Product detail

A proposed production product detail page should support, when authoritative data exists:

- Overview and exact model identity/revision.
- Structured specifications with units and conditions.
- PQ curve and declared operating points.
- Dimensional drawings.
- Approved datasheet, CAD, manual, and certificate downloads.
- Control/integration information.
- Applicable industries/solutions.
- Related technical content, FAQs, cases, and products.
- A contextual product RFQ.

Use `$airtek-product-knowledge` for schema, exact-data authority, and performance rules. If a required asset or field is absent, show an honest availability state or offer engineering contact; never fill it from the static demo.

## Selector and comparison

The canonical selector inputs, constraints, ranking, and comparison-data contract live in [product data rules](../../airtek-product-knowledge/references/product-data-rules.md#selector-contract). The current public site implements anonymous selection and client-side comparison without an account. Comparison state is browser-local; server-side selection returns matched, no-validated-candidate, or engineering-review-required outcomes from published records.

The interface should preserve units and user-entered conditions, explain matched and disqualified results supplied by the product service, expose incomplete-data/engineering-review states, and pass visible editable context into an RFQ only when the user chooses. Comparison should render the selected live records with accessible headings and clear missing/incomparable states.

The supplied selector demo ignores inputs, displays all five hard-coded products, uses fixed match scores, renders a static comparison, offers placeholder downloads, and makes RFQ an alert. These behaviors are `DEPRECATED`; only the broad interaction concept may inform design.

## RFQ journeys

The current public router and OpenAPI implement four distinct journeys:

1. Product RFQ — begins with a known model/product context.
2. Fan Selection RFQ — begins with duty point, environment, dimensions, electrical/control, and selector context.
3. Project RFQ — begins with application, project conditions, scale, schedule, and engineering needs.
4. Replacement RFQ — begins with existing model/nameplate, installation, duty point, constraints, and replacement goals.

The implemented requests define typed fields, validation, visible context, consent,
idempotent submission, retention, and a submission identifier, then enter the
Admin business inbox for assignment, status, PII access, and internal notes.
Public RFQs do not accept file uploads; the source proposal's document/photo
uploads remain unimplemented and require a separate quarantine, malware,
retention, and access-control design before addition. Do not promise price, MOQ,
lead time, payment, Incoterms, certification, or selection suitability in an
automatic response.

## Lead handoff

Capture source page/campaign, selected product(s), selector inputs, locale, consent state, and the user's submitted business context in the business system—not as unrestricted analytics properties. Define ownership, acknowledgement, routing, duplicate handling, status lifecycle, service expectations, and audit history before automating notifications.

## Contact, support, and FAQ boundaries

Contact is for general routing and should not replace the structured RFQ paths. Distinguish sales inquiry, existing-order support, technical support, careers, suppliers, and media/general contact when those channels exist.

Plan FAQ types for Product, Selection, Technical, Application, Customization/OEM, Ordering/Commercial, Shipping/Delivery, Installation/Maintenance, and Replacement/After-sales. FAQ answers must follow the same claim and exact-data rules as product pages. Do not publish generic installation or safety advice as a substitute for an approved model manual.

## Funnel events

Measure meaningful steps such as CTA click, RFQ route selected, form started,
field-group completed, validation error, submit success/failure, and abandonment
at a coarse approved stage. Upload events apply only if a future approved upload
flow exists. Never log free-text fields, uploaded-file names/content, email,
phone, or other direct identifiers in analytics.
