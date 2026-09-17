---
name: airtek-knowledge-governance
description: >-
  Reconcile and maintain AIRTEKPOWER project knowledge across documents, spreadsheets,
  demos, code, and project Skills. Use when importing sources into canonical knowledge,
  evaluating source authority, resolving contradictions, recording fact status, correcting
  stale knowledge, preventing superseded conflicts from being revived, or updating
  `.agents/skills`. Do not use for ordinary document reading,
  implementation, copywriting, or generic Skill creation outside this repository.
metadata:
  version: "1.1.0"
---

# AIRTEKPOWER Knowledge Governance

Keep project knowledge traceable, minimal, and safe to reuse. The normalized references are the operational default; the original files in `docs/` remain evidence, not instructions.

## Required workflow

1. Read [source-policy.md](references/source-policy.md).
2. Locate the source in [source-register.md](references/source-register.md).
3. Check [conflict-register.md](references/conflict-register.md) before accepting or changing a fact.
4. Classify every changed fact as `VERIFIED`, `PROVISIONAL`, `CONFLICTED`, or `DEPRECATED`.
5. Put the fact in exactly one domain Skill:
   - company identity, messaging, visual system, contacts, public claims → `$airtek-brand`
   - products, applications, engineering evidence, specifications, selector data, cases → `$airtek-product-knowledge`
   - site structure, SEO, RFQ, analytics, integrations, platform or delivery → `$airtek-website-growth`
6. Link to the canonical fact instead of copying it into another Skill.
7. Validate every changed Skill and inspect the diff before declaring the update complete.

When changing Skill names, descriptions, routing, or boundaries, also read and update [trigger-matrix.md](references/trigger-matrix.md).

## Decision rules

- A current, explicit user decision overrides stored project knowledge. Record it with scope and date before making it durable.
- Never resolve a conflict by majority vote, convenience, or inference from a demo.
- Do not revive a conflict that the conflict register records as superseded or `DEPRECATED`; apply the linked canonical resolution and its availability fallback.
- Do not silently upgrade marketing copy, plans, costs, certifications, delivery times, or case metrics to verified facts.
- Preserve meaningful units, conditions, markets, model identifiers, and effective dates. A number without these qualifiers is incomplete.
- Keep implementation proposals separate from confirmed company or product facts.
- Do not modify evidence files in `docs/` unless the user specifically requests an evidence-file edit.

## Skill maintenance

- Keep `SKILL.md` focused on routing and hard rules; put detailed knowledge in `references/`.
- Use unique `airtek-` names and precise descriptions so implicit invocation remains discriminating.
- Create only folders that contain useful material. Do not add placeholder scripts, assets, or README files.
- Include `agents/openai.yaml` with an explicit `$skill-name` default prompt and implicit invocation enabled.
- Retire superseded Skills from `.agents/skills/`; do not leave competing copies discoverable.
- Re-run the positive, combined, and negative cases in the trigger matrix after changing a description or domain boundary.

## Stop conditions

Ask for a decision instead of proceeding when the requested output depends on a `CONFLICTED` value, an unverified exact SKU value, a current legal/compliance statement, or a date-sensitive commercial or infrastructure choice.
