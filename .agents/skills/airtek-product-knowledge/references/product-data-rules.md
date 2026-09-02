# Product data rules

Updated: 2026-09-01

## Authority for exact values

Use this order for a specific model:

1. Validated Product Master or controlled engineering record.
2. Current official datasheet for that exact model and revision.
3. Current project database/code only when its provenance is known.
4. Family-level company deck for broad capability context only.
5. Demo HTML for interaction reference only; never for facts.

On 2026-09-02 the owner explicitly approved the CSV registered as `Product Master CSV e3b944d5…` in the knowledge-governance source register as the validated Product Master for the initial database import. Exact SKU values are publishable only from the audited database source snapshot with that full checksum and the approved `airtek-basic-v1` mapping—not from copied CSV fragments, demos, or family-level material. A future source with a different checksum must enter staging and be approved independently.

## Family-level reported ranges

These are `PROVISIONAL` marketing-deck capability ranges. They are useful for information architecture and schema planning, not SKU records or performance guarantees.

| Family | Reported diameter/size | Reported airflow | Reported pressure | Reported power/speed | Evidence locator |
|---|---|---|---|---|---|
| Backward-curved centrifugal | 133–630 mm | 200–24,000 m³/h | 400–2,100 Pa static | 30 W–5.9 kW; AC/DC/EC | Company deck PDF p. 13 |
| Forward-curved centrifugal | 120–500 mm | 30–8,000 m³/h | 0–3,800 Pa static | 0.01–3 HP | Company deck PDF p. 14 |
| Axial | 195–900 mm | 200–30,200 m³/h | Not normalized from source | 70–4,400 W | Company deck PDF p. 15 |
| Cross-flow | 25–200 mm | Not normalized from source | Not normalized from source | Up to 3,600 RPM | Company deck PDF p. 16 |
| Inline duct | 100–355 mm inlet | 200–7,100 CMH | Not normalized from source | IP42/IP44/IP55/IP67 reported across the family | Company deck PDF p. 17 |

Do not infer missing columns. `CMH` and `m³/h` are dimensionally equivalent, but preserve the source label and document conversions in the data pipeline. IP ratings and motor technologies must be assigned per model, not to the entire family by implication.

## Minimum product record

A production product record should separate:

- Stable identity: product ID, exact model, revision, lifecycle/publication status, locales.
- Classification: primary fan form, subtype/impeller geometry, inlet arrangement, motor technology, application tags.
- Mechanical: dimensions, mounting, weight, materials, ingress protection, environment limits.
- Electrical/control: supply type, voltage, frequency, phase, input/current/power, speed, controls, feedback, protection.
- Performance: declared operating points and PQ-curve datasets with units, conditions, tolerance, speed, and test method.
- Acoustic: metric, value, operating point, measurement setup, and source.
- Compliance: certificate/standard, holder, issuer, model scope, market, validity, and asset.
- Assets: approved images, drawings, datasheet, CAD, manuals, certificates, locale, revision, access level, checksum.
- Commercial/workflow: availability for inquiry, custom options, related products, owners, source, validation result, publish timestamps.

Use explicit nullability. Missing, not applicable, not tested, confidential, and pending verification are different states and should not collapse to an empty string or zero.

## Import and publishing

Recommended flow:

`Feishu or controlled spreadsheet → staging import → schema/unit/reference validation → reviewer approval → publish → database/object storage`

Validate uniqueness, required fields, enum values, units, ranges, curve monotonic/order constraints, asset existence, model/certificate scope, locale completeness, and referential integrity. Never publish directly from a spreadsheet edit. Keep an audit trail and allow rollback to a prior published revision.

## Selector contract

Candidate inputs may include application, ambient temperature, installation dimensions, environment, certifications, airflow, pressure, voltage/frequency, fan/motor type, control, and priority.

1. Validate units and required fields.
2. Apply hard constraints first: environmental suitability, dimensions, electrical compatibility, required certification, and ability to meet the duty point.
3. Rank survivors by declared preferences such as efficiency, noise, size, or headroom.
4. Explain matches and disqualifications using source fields.
5. Show uncertainty and request engineering review when curves/conditions are incomplete.
6. Allow comparison of selected live records; do not render a fixed comparison table.
7. Pass selected model, duty point, inputs, and evidence into the correct RFQ journey with user consent.

## Known demo conflict

The two demos assign model `B23E280H128-102-B0` different values:

- Selector demo: 3,800 m³/h, 900 Pa, 350 W (`docs/Plan & Solution/SelectionToolDemo.html`, lines 1769–1775).
- Product-detail demo: 3,290 m³/h, 715 Pa, 0.75 kW (`docs/Plan & Solution/ProductDetailDemo.html`, model at line 741 and values at lines 807–833).

Status: `DEPRECATED` as a live conflict after the 2026-09-02 owner decision. The validated Product Master source snapshot is authoritative for this model; both demo value sets remain prohibited. The same rule applies to every other hard-coded demo product, curve, download, and match score.
