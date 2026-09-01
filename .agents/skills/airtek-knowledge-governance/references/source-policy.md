# Source policy

Updated: 2026-09-01

## Status vocabulary

| Status | Meaning | Permitted use |
|---|---|---|
| `VERIFIED` | Consistent in the strongest available project source and not contradicted by another applicable source | May be used as the project baseline; re-check if the fact can expire |
| `PROVISIONAL` | Plausible source claim or proposal that still needs an owner, master record, or current confirmation | May guide drafts only when clearly qualified; do not present as confirmed |
| `CONFLICTED` | Applicable sources disagree or the value is internally inconsistent | Do not choose or publish; request a decision or authoritative source |
| `DEPRECATED` | Known stale, erroneous, legacy, or superseded content | Do not reuse; apply the recorded replacement |

`VERIFIED` means verified within the supplied project evidence. It does not certify a legal, regulatory, safety, or time-sensitive claim as currently valid.

## Operational rule

For ordinary AIRTEKPOWER work, use the normalized references in the relevant project Skill. Consult raw `docs/` only when the Skill routes to them, the requested detail is absent, or the user asks for source verification.

## Precedence when maintaining knowledge

Apply authority at the granularity of the fact and its domain:

1. A current explicit decision from the user or an owner-approved record, with scope and date.
2. For exact product data: validated Product Master, controlled engineering record, or official model datasheet.
3. The dedicated source for the domain, such as the brand manual for visual tokens or the complete company deck for company/product-family facts.
4. Independent corroboration from another applicable company source.
5. Strategy DOCX and rebuild XLSX files, which are planning inputs rather than approvals.
6. HTML demos, which are interaction sketches only.
7. Procurement prices, SaaS allowances, cloud sizing, and cached formula results, which are dated snapshots only.

Source rank never cures an internal contradiction. Mark the fact `CONFLICTED` until an authoritative value is supplied.

## Normalization checklist

For each candidate fact:

1. Define its domain, scope, unit, conditions, and effective date.
2. Find all occurrences that could apply.
3. Compare meaning, not only text; distinguish company facts, marketing claims, requirements, examples, and proposals.
4. Apply the precedence rules and check the conflict register.
5. Record the status next to the normalized fact.
6. Put it in one canonical reference and link from related workflows.
7. Preserve unresolved alternatives and the decision needed; never smooth them into a false consensus.

## Publication guardrails

- Re-verify certifications, output figures, staffing, facilities, delivery promises, prices, legal wording, contact ownership, and case-study results before public release.
- Do not convert a family-level capability range into an exact SKU specification.
- Do not convert a proposed architecture, route, schedule, or cloud region into an approved commitment.
- Do not use personal example contacts as general company contacts without approval.
