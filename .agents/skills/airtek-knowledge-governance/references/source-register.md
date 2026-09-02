# Source register

Updated: 2026-09-01

Paths are relative to the project root. Original files remain evidence in `docs/`.

| Source | Role | Authority and limitations |
|---|---|---|
| `docs/PDF版本：企业介绍 (English Version) .pdf` | Complete English company deck; 34 pages | Primary supplied source for company profile, product families, capabilities, facilities, testing, and reported case material. Marketing and time-sensitive claims remain `PROVISIONAL`. |
| `docs/About AIRTEK.pdf` | Eight-page company subset | Corroborates company and contact material; does not outrank the complete deck. |
| `docs/Product Summary_AIRTEK.pdf` | Eight-page product subset | Family-level overview only; not an exact SKU master. |
| `docs/Successful Projects Showcase.pdf` | Fifteen-page case subset | Source for reported outcomes. Metrics and client-sensitive claims require publication verification. |
| `docs/品牌视觉系统管理.pdf` | Brand visual manual; 24 pages | Primary supplied source for logo, colors, and typography, subject to the recorded legacy-name, color-label, and font inconsistencies. |
| `docs/Plan & Solution/Airtek Power Website Growth Solution.docx` | Website strategy draft; 29 pages | Planning input for positioning, information architecture, SEO, content, RFQ, analytics, and platform direction. It is not a final implementation specification. |
| `docs/Plan & Solution/Airtek Power Website Rebuild Solution.xlsx` | Rebuild scope and staged backlog; six sheets | Planning input with stage, duration, and scope conflicts. Use capabilities and acceptance intent only after normalization. |
| `docs/Plan & Solution/采购清单.xlsx` | Procurement cost snapshot | Date-sensitive and incomplete. Do not treat as total cost, approved architecture, or durable Skill constant. |
| `docs/Plan & Solution/ProductDetailDemo.html` | Static product-detail prototype | Interaction and layout sketch only. Hard-coded specifications, curve, files, colors, and form behavior are not authoritative. |
| `docs/Plan & Solution/SelectionToolDemo.html` | Static selector prototype | Interaction sketch only. Inputs, matching, comparison, product values, downloads, and RFQ behavior are not implemented contracts. |
| Product Master CSV `e3b944d5d979c72d963ba353416ef452f9dac0bf63182bb09fc6b1201c043800` | Owner-approved exact-model source snapshot supplied 2026-09-02 and imported through the audited database pipeline | `VERIFIED` by the current explicit owner decision for the initial `airtek-basic-v1` import. It contains 370 structurally valid model rows and five malformed trailing rows that must be rejected. Commercial price columns remain encrypted internal staging; filename-only assets remain unresolved; noise is not public until its measurement conditions are supplied. |
| `docs/Plan & Solution/~$Airtek Power Website Rebuild Solution.xlsx` | Office lock file | `DEPRECATED` project noise; ignored by Git and never used as evidence. |

## Missing authoritative sources

The original `docs/` materials do not include controlled per-model datasheets, approved logo assets, a current certification register, an approved case-claim register, an agreed cloud deployment decision, or a signed implementation scope. A validated Product Master was supplied separately and is authoritative only when its full registered checksum matches the audited database source snapshot. Keep decisions that depend on the other missing sources provisional or conflicted.
