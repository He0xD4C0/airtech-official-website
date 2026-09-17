# Brand asset governance and register

Last reviewed: 2026-09-14

Use this reference for logos, favicons, company or facility photography, corporate video, certificate imagery, and any asset recovered from a public or legacy website. It does not govern product media.

## Authority and release states

Apply authority to the asset itself, not merely to the page where it appeared:

1. Owner-approved original artwork or media with recorded usage rights and release scope.
2. The controlled brand manual for logo construction and visual rules.
3. The controlled company deck for corroborating company identity and the intended presentation style.
4. A legacy public website capture, which is discovery evidence only and never approval by itself.

Use these release states in addition to the governance status vocabulary:

| Release state | Meaning | Permitted use |
|---|---|---|
| `APPROVED_FOR_PRODUCTION` | Owner-approved master, rights, scope, integrity, and current identity are recorded | May be copied into a production asset library and published within its approved scope |
| `REVIEW_REQUIRED` | Potentially relevant, but one or more release checks are missing | Keep in isolated staging; do not upload to the CMS or ship publicly |
| `BLOCKED` | Known noncompliance, expiry, wrong scope, or superseded identity | Evidence only; do not publish or use as a design source |

No reviewed asset currently has `APPROVED_FOR_PRODUCTION` status.

## Logo construction

The brand manual PDF pp. 7–8 defines the logo as a fixed relationship between the graphic mark and text mark, says the text may not be used alone, and instructs users to copy the approved final artwork rather than redraw it. The company deck PDF pp. 1–6 and 34, and the eight-page About subset, repeatedly corroborate a combined graphic-plus-`AIRTEKPOWER` presentation.

Therefore:

- Do not treat a graphic-only crop as the AIRTEKPOWER logo.
- Do not crop a logo out of a PDF, trace it, rebuild the lettering, or infer clear space and minimum size.
- Do not separate, re-space, recolor, stretch, outline, shadow, or otherwise reconstruct the mark.
- A favicon may use a distinct small-format treatment only after the owner supplies or explicitly approves that treatment; the manual does not establish a favicon exception.
- The Chinese display text `艾特克（中国）` and the subline `Green-Energy Saving Ventilation Fans` shown in some compositions are not approved as standalone name or slogan assets. Preserve them only when they are inseparable parts of an owner-approved master file.

## Legacy-site capture decision register

The following records reconcile the 2026-09-14 visual review of 47 unique candidates from 11 pages on `https://www.airtekpower.com` with the controlled brand documents. IDs are the first 12 hexadecimal characters of each file's SHA-256 checksum. Use [brand-asset-register.json](brand-asset-register.json) for full checksums, source URLs, and machine-readable decisions.

| ID | Asset | Governance status | Release state | Decision |
|---|---|---|---|---|
| `3fd9c775dd54` | 72×72 PNG graphic crop | `DEPRECATED` for logo use | `BLOCKED` | Graphic mark only; violates the required combined logo construction |
| `62480adeeb26` | 73×73 PNG graphic crop | `DEPRECATED` for logo use | `BLOCKED` | Graphic mark only; violates the required combined logo construction |
| `89598065eb49` | 72×72 PNG graphic crop | `DEPRECATED` for logo use | `BLOCKED` | Graphic mark only; violates the required combined logo construction |
| `93694fd4a67c` | 32×32 ICO favicon | `PROVISIONAL` | `REVIEW_REQUIRED` | Small-format mark has no documented approval or favicon rule |
| `147bf238e1f6` | 800×500 factory exterior photo | `PROVISIONAL` | `REVIEW_REQUIRED` | Visible sign uses `AIRTEK POWER`; current site identity, date, and rights are unconfirmed |
| `f59781891cb0` | 850×400 aerial facility photo | `PROVISIONAL` | `REVIEW_REQUIRED` | Published in a branch article, but location, ownership, date, and rights are unconfirmed |
| `4bb27c9c94df` | 453×640 ISO 9001:2015 image | `DEPRECATED` | `BLOCKED` | Displayed validity ended 2026-03-30; historical evidence only |

The capture found no approved vector/high-resolution production logo, identifiable team photo, company-owned corporate video, or public brand-manual download. Three video slots referenced the generic Leadong demo file and are excluded. Product photos, banners, product compliance reports, curves, and specifications remain outside this Skill.

## Promotion checklist

Before changing any record to `APPROVED_FOR_PRODUCTION`, record all of the following:

- original file, cryptographic checksum, source, capture/receipt date, and accountable owner;
- confirmed company identity, subject, location, date, and intended placement;
- copyright or licence holder, permitted channels/territories/duration, and any required credit;
- owner approval for the exact file and variant, including logo lockup and colorway;
- current validity and publication scope for certificates or other time-sensitive evidence;
- media-safety result of `clean`, with `pending` and `quarantined` remaining non-public;
- accessible alternative text and any privacy or personal-consent record;
- final production derivatives linked back to the approved master without overwriting the original.

Store unapproved downloads in isolated review staging. Do not place them in this Skill's `assets/` directory or the CMS. When an approved master arrives, preserve it unchanged, add a machine-readable record with the full SHA-256 checksum, and keep web-ready derivatives separate.

## Source basis

- `docs/Internal-docs/品牌视觉系统管理.pdf`, especially PDF pp. 7–15
- `docs/Internal-docs/PDF版本：企业介绍 (English Version) .pdf`, especially PDF pp. 1–6 and 34
- `docs/Internal-docs/About AIRTEK.pdf`, eight-page corroborating subset
- `https://www.airtekpower.com`, scoped capture performed 2026-09-14; lower-authority, mutable public source

The internal PDFs are intentionally excluded from Git. If they are unavailable, report `internal evidence unavailable`. Do not lower the approval bar or substitute a public-site crop.
