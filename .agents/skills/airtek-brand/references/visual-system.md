# Visual system

Updated: 2026-09-01

## Core palette

| Token | RGB | Hex | Typical role | Status |
|---|---:|---|---|---|
| Brand blue | 12, 116, 151 | `#0C7497` | Primary actions, links, key brand surfaces | `VERIFIED`; brand manual PDF p. 10 |
| Brand green | 39, 151, 87 | `#279757` | Sustainability/efficiency accent and secondary emphasis | `VERIFIED`; brand manual PDF p. 10. The page's word “orange” is `DEPRECATED` because the shown RGB swatch is green. |
| Dark gray | 58, 78, 77 | `#3A4E4D` | Text, dark surfaces, supporting UI | `VERIFIED`; brand manual PDF p. 10 |
| Black | 0, 0, 0 | `#000000` | High-contrast text or approved monochrome use | `VERIFIED`; brand manual PDF p. 10 |

Derive accessible hover, focus, tint, and surface tokens from these colors during implementation; do not replace the brand colors with the unrelated demo blues `#1976d2` or `#1464d2`. Verify WCAG contrast for the actual foreground/background pair rather than assuming a brand color is accessible in every combination.

No reliable approved Pantone or CMYK mapping is present. The manual discusses Pantone generally and shows `PANTONE 166C` in an application example on PDF p. 15, but that value is not reconciled to the canonical RGB palette. Status: `CONFLICTED`; do not convert, print, or publish it as the official spot color until the brand owner supplies a controlled print-color specification.

## Typography

- Latin canonical family: `CONFLICTED`. On brand manual PDF p. 12, the Chinese instruction names Helvetica, while the English instruction names both Bree Serif and Helvetica.
- Provisional implementation fallback: use a configurable Helvetica/Arial/system sans-serif stack for prototypes only. Do not describe it as the approved brand primary until the owner confirms the family, weights, files, licensing, and contexts.
- Chinese primary: the manual names Source Han Sans (`思源黑体`) on PDF p. 13, but the sentence also contains a legacy company name. Treat the font instruction as `PROVISIONAL` until the approved font asset and license are confirmed; Noto Sans CJK SC or a system sans-serif may be a configurable fallback.
- Bree Serif: do not use merely because it appears once in the English sentence. It requires the same owner and asset confirmation.
- The demos' Inter/system stacks are not brand authority.

Keep typography tokens configurable until the conflict is resolved. Avoid loading font files that are not present or licensed.

## Logo

The manual states that the identity combines a graphic mark with a wordmark and that the elements should not be redrawn (PDF pp. 7–8). Apply that rule to `AIRTEKPOWER`, replacing the legacy names in those pages. Use an approved vector or high-resolution source asset when one becomes available. Do not trace a PDF screenshot, redraw the symbol, alter proportions, recolor outside approved variants, add effects, or substitute a legacy wordmark.

No approved standalone logo asset or clear-space/minimum-size production file is present in the repository. Treat those implementation details as `PROVISIONAL` until assets are supplied.

## UI application

- Use a restrained industrial visual language with clear hierarchy and generous technical-data spacing.
- Use blue as the main interactive color and green as a purposeful accent, not as a decorative second primary everywhere.
- Keep charts, PQ curves, status colors, and data visualizations semantically distinguishable; they need not use only brand colors.
- Provide visible keyboard focus, sufficient target sizes, readable tables, reduced-motion behavior where relevant, and text alternatives for meaningful graphics.
- Use a dynamic current year in the copyright line.

## Source basis

- `docs/品牌视觉系统管理.pdf`
- Demo HTML files are negative references only; they do not define brand tokens.
