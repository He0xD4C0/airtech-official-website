# Source register

Last reviewed: 2026-09-14

Paths are relative to the project root. Original files remain evidence in `docs/`. Files marked `INTERNAL_LOCAL` are intentionally excluded from Git; their absence in a fresh checkout must be reported as `internal evidence unavailable`, never treated as permission to use a substitute.

| Source | Availability | Role | Authority and limitations |
|---|---|---|---|
| `docs/Internal-docs/PDF版本：企业介绍 (English Version) .pdf` | `INTERNAL_LOCAL` | Complete English company deck; 34 pages | Primary supplied source for company profile, product families, capabilities, facilities, testing, and reported case material. Marketing and time-sensitive claims remain `PROVISIONAL`. |
| `docs/Internal-docs/About AIRTEK.pdf` | `INTERNAL_LOCAL` | Eight-page company subset | Corroborates company and contact material; does not outrank the complete deck. |
| `docs/Internal-docs/Product Summary_AIRTEK.pdf` | `INTERNAL_LOCAL` | Eight-page product subset | Family-level overview only; not an exact SKU master. |
| `docs/Internal-docs/Successful Projects Showcase.pdf` | `INTERNAL_LOCAL` | Fifteen-page case subset | Source for reported outcomes. Metrics and client-sensitive claims require publication verification. |
| `docs/Internal-docs/品牌视觉系统管理.pdf` | `INTERNAL_LOCAL` | Brand visual manual; 24 pages | Primary supplied source for logo, colors, and typography, subject to the recorded legacy-name, color-label, and font inconsistencies. |
| `https://www.airtekpower.com` scoped capture, reviewed 2026-09-14 | `PUBLIC_EXTERNAL` | Discovery source for missing company-brand asset candidates; 11 pages and 47 unique files visually reviewed | Mutable, lower-authority legacy website. Appearance on the site does not establish current identity, rights, approval, or validity. Canonical decisions are in [brand asset governance](../../airtek-brand/references/brand-assets.md). |
| `docs/Plan & Solution/Airtek Power Website Growth Solution.docx` | `REPOSITORY` | Website strategy draft; 29 pages | Historical planning input for positioning, information architecture, SEO, content, RFQ, analytics, and platform direction. It is not the current implementation specification. |
| `docs/Plan & Solution/Airtek Power Website Rebuild Solution.xlsx` | `REPOSITORY` | Rebuild scope and staged backlog; six sheets | Historical planning input with stage, duration, and scope conflicts. Use capabilities and acceptance intent only after normalization. |
| `docs/Plan & Solution/采购清单.xlsx` | `REPOSITORY` | Procurement cost snapshot | Date-sensitive and incomplete. Do not treat as total cost, approved architecture, or durable Skill constant. |
| `docs/Plan & Solution/ProductDetailDemo.html` | `REPOSITORY` | Static product-detail prototype | Interaction and layout sketch only. Hard-coded specifications, curve, files, colors, and form behavior are not authoritative. |
| `docs/Plan & Solution/SelectionToolDemo.html` | `REPOSITORY` | Static selector prototype | Interaction sketch only. Inputs, matching, comparison, product values, downloads, and RFQ behavior are not implemented contracts. |
| Product Master CSV `e3b944d5d979c72d963ba353416ef452f9dac0bf63182bb09fc6b1201c043800` | `CONTROLLED_EXTERNAL` | Owner-approved exact-model source snapshot supplied 2026-09-02 and imported through the audited database pipeline | `VERIFIED` by the current explicit owner decision for the initial `airtek-basic-v1` import. The raw CSV is not repository-resident. Exact values require a matching controlled snapshot or published record with this provenance; otherwise return `Published data unavailable`. It contains 370 structurally valid model rows and five malformed trailing rows that must be rejected. Commercial price columns remain encrypted internal staging; filename-only assets remain unresolved; noise is not public until its measurement conditions are supplied. |
| `docs/Plan & Solution/~$Airtek Power Website Rebuild Solution.xlsx` | `IGNORED_NOISE` | Office lock file | `DEPRECATED` project noise; ignored by Git and never used as evidence. |

## Missing authoritative sources

The original `docs/` materials do not include controlled per-model datasheets, an approved standalone logo master, documented favicon treatment, a current certification register, an approved case-claim register, an agreed cloud deployment decision, or a signed implementation scope. A validated Product Master was supplied separately and is authoritative only when its full registered checksum matches the audited database source snapshot. Keep decisions that depend on the other missing sources provisional or conflicted.
