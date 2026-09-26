#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const ROOT = process.cwd();
const STAGING = path.resolve(
  process.argv[2] ?? "tmp/airtekpower.com-brand-assets-2026-09-14",
);
const DECISIONS_PATH = path.join(ROOT, "scripts/airtek-brand-asset-decisions.json");
const execFileAsync = promisify(execFile);

async function readJson(file) {
  return JSON.parse(await readFile(file, "utf8"));
}

async function dimensions(file) {
  try {
    const { stdout } = await execFileAsync("identify", ["-format", "%w\t%h", file], {
      encoding: "utf8",
    });
    const [width, height] = stdout.trim().split("\t").map(Number);
    return { width, height };
  } catch {
    return { width: null, height: null };
  }
}

function flattenDecisions(review) {
  const map = new Map();
  for (const group of review.groups) {
    for (const id of group.ids) {
      if (map.has(id)) throw new Error(`Duplicate decision for ${id}`);
      map.set(id, {
        disposition: group.disposition,
        governanceStatus: group.governanceStatus,
        category: group.category,
        reason: group.reason,
      });
    }
  }
  return map;
}

function uniqueByHash(assets) {
  const map = new Map();
  for (const asset of assets) {
    const existing = map.get(asset.sha256);
    if (!existing) {
      map.set(asset.sha256, { ...asset, urls: [asset.url] });
      continue;
    }
    existing.urls.push(asset.url);
    existing.references.push(...asset.references);
  }
  return [...map.values()];
}

function markdown(manifest) {
  const lines = [
    "# AIRTEKPOWER legacy-site brand asset staging",
    "",
    `Captured: ${manifest.capturedAt}`,
    "",
    "This directory is an isolated review package. Nothing here is approved for public use or uploaded to the CMS.",
    "",
    "## Outcome",
    "",
    `- Ready to publish: ${manifest.counts.readyToPublish}`,
    `- Review required: ${manifest.counts.reviewRequired}`,
    `- Held as expired evidence: ${manifest.counts.heldExpired}`,
    `- Excluded and removed: ${manifest.counts.excluded}`,
    "",
    "## Coverage gaps",
    "",
    ...manifest.coverage.map((entry) => `- ${entry.area}: ${entry.result}`),
    "",
    "## Retained files",
    "",
    ...manifest.assets
      .filter((asset) => asset.localFile)
      .map(
        (asset) =>
          `- ${asset.localFile} — ${asset.disposition}; ${asset.reason} Source: ${asset.urls.join(", ")}`,
      ),
    "",
    "See `brand-manifest.json` for hashes and full source-page references; `brand-manifest.csv` is the review-friendly index.",
    "",
  ];
  return lines.join("\n");
}

function csvCell(value) {
  const text = value == null ? "" : String(value);
  return `"${text.replaceAll('"', '""')}"`;
}

function csv(manifest) {
  const header = [
    "id",
    "disposition",
    "governance_status",
    "category",
    "mime",
    "bytes",
    "width",
    "height",
    "local_file",
    "source_urls",
    "source_pages",
    "reason",
  ];
  const rows = manifest.assets.map((asset) => [
    asset.id,
    asset.disposition,
    asset.governanceStatus,
    asset.category,
    asset.mime,
    asset.bytes,
    asset.width,
    asset.height,
    asset.localFile,
    asset.urls.join(" | "),
    [...new Set(asset.references.map((reference) => reference.pageUrl))].sort().join(" | "),
    asset.reason,
  ]);
  return [header, ...rows].map((row) => row.map(csvCell).join(",")).join("\n") + "\n";
}

async function main() {
  const [candidateManifest, review] = await Promise.all([
    readJson(path.join(STAGING, "candidate-manifest.json")),
    readJson(DECISIONS_PATH),
  ]);
  const decisions = flattenDecisions(review);
  const uniqueAssets = uniqueByHash(candidateManifest.assets);
  const candidateIds = new Set(uniqueAssets.map((asset) => asset.id));
  const missingDecisions = [...candidateIds].filter((id) => !decisions.has(id));
  const unknownDecisions = [...decisions.keys()].filter((id) => !candidateIds.has(id));
  if (missingDecisions.length || unknownDecisions.length) {
    throw new Error(
      `Decision coverage mismatch: missing=${missingDecisions.join(",")}; unknown=${unknownDecisions.join(",")}`,
    );
  }

  const reviewed = [];
  for (const asset of uniqueAssets) {
    const decision = decisions.get(asset.id);
    const sourceFile = path.join(STAGING, asset.file);
    const bytes = await readFile(sourceFile);
    const actualHash = createHash("sha256").update(bytes).digest("hex");
    if (actualHash !== asset.sha256) throw new Error(`Hash mismatch for ${asset.file}`);

    let localFile = null;
    if (!decision.disposition.startsWith("EXCLUDED_")) {
      const section = decision.disposition === "HOLD_EXPIRED" ? "hold" : "review-required";
      const filename = path.basename(asset.file);
      localFile = path.join(section, decision.category, filename);
      await mkdir(path.join(STAGING, section, decision.category), { recursive: true });
      await copyFile(sourceFile, path.join(STAGING, localFile));
    }

    reviewed.push({
      id: asset.id,
      sha256: asset.sha256,
      mime: asset.mime,
      bytes: asset.bytes,
      ...(await dimensions(sourceFile)),
      urls: [...new Set(asset.urls)].sort(),
      references: asset.references,
      ...decision,
      localFile,
    });
  }

  reviewed.sort((a, b) => a.id.localeCompare(b.id));
  const manifest = {
    source: candidateManifest.source,
    capturedAt: candidateManifest.capturedAt,
    reviewedAt: review.reviewedAt,
    reviewBasis: review.reviewBasis,
    authority: "Legacy public website; candidate evidence only",
    publicationStatus: "NOT_APPROVED",
    publicationRule:
      "Owner approval, usage rights, current identity, media safety review, and any certification validity checks are required before CMS upload.",
    counts: {
      scopedPages: candidateManifest.counts.scopedPages,
      downloadedPages: candidateManifest.counts.downloadedPages,
      uniqueCandidatesReviewed: reviewed.length,
      readyToPublish: reviewed.filter((asset) => asset.disposition === "READY").length,
      reviewRequired: reviewed.filter((asset) => asset.disposition === "REVIEW_REQUIRED").length,
      heldExpired: reviewed.filter((asset) => asset.disposition === "HOLD_EXPIRED").length,
      excluded: reviewed.filter((asset) => asset.disposition.startsWith("EXCLUDED_")).length,
      downloadErrors: candidateManifest.errors.length,
    },
    coverage: [
      {
        area: "Logo and visual identity",
        result: "Three 72x72 responsive logo rasters and one 32x32 favicon retained for review; no approved vector, high-resolution production logo, or clear-space asset found.",
      },
      {
        area: "Company and factory environment",
        result: "Two photos retained for owner and rights review; only one visibly carries AIRTEK POWER signage, using a legacy spaced wordmark.",
      },
      {
        area: "Team and corporate culture",
        result: "No AIRTEKPOWER-identifiable team photo found; generic event stock imagery excluded.",
      },
      {
        area: "Corporate video",
        result: "No company video found; all three legacy video slots point to the same generic Leadong demo file at https://video.leadongcdn.cn/leadong-index.mp4 and were excluded.",
      },
      {
        area: "Brand manual",
        result: "No public brand manual or downloadable brand guideline found on the scoped website pages.",
      },
      {
        area: "Enterprise certification",
        result: "One company ISO 9001:2015 certificate image found, but its displayed validity ended 2026-03-30; retained only in hold/ as expired evidence.",
      },
      {
        area: "Product resources",
        result: "Product photos, banners, product compliance reports, PQ/specification material, and unrelated consumer-demo imagery were excluded and are not retained in the final staging package.",
      },
    ],
    sourcePages: candidateManifest.pages,
    downloadErrors: candidateManifest.errors,
    assets: reviewed,
  };

  await writeFile(path.join(STAGING, "brand-manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  await writeFile(path.join(STAGING, "brand-manifest.csv"), csv(manifest));
  await writeFile(path.join(STAGING, "README.md"), markdown(manifest));
  console.log(JSON.stringify(manifest.counts));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
