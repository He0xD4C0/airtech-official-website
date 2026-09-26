#!/usr/bin/env node

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

const SITE_ORIGIN = "https://www.airtekpower.com";
const CAPTURE_DATE = "2026-09-14";
const DEFAULT_OUTPUT = `tmp/airtekpower.com-brand-assets-${CAPTURE_DATE}`;
const OUTPUT_DIR = path.resolve(process.argv[2] ?? DEFAULT_OUTPUT);
const FILES_DIR = path.join(OUTPUT_DIR, "candidates");
const MAX_ASSET_BYTES = 80 * 1024 * 1024;
const CONCURRENCY = 4;
const USER_AGENT =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) " +
  "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140 Safari/537.36";
const CURL_MARKER = "AIRTEK_BRAND_CURL_METADATA_fdd4db86";
const execFileAsync = promisify(execFile);

const PAGE_SCOPE = [
  { path: "/", category: "home-brand" },
  { path: "/Company-Introduction.html", category: "company-profile" },
  { path: "/corporate-culture.html", category: "culture" },
  { path: "/development-history.html", category: "history" },
  { path: "/videos.html", category: "corporate-video" },
  { path: "/authority.html", category: "certification-display" },
  { path: "/gallery.html", category: "company-gallery" },
  { path: "/contactus.html", category: "company-location" },
  {
    path: "/Airtek-Power-Solution-Establishes-a-New-Branch-in-Suzhou-id42258216.html",
    category: "company-news",
  },
  {
    path: "/Airtek-Power-Solution-Team-Road-Adventure-id41281216.html",
    category: "team-culture",
  },
  {
    path: "/Airtek-Power-Solution-Shines-at-the-American-Refrigeration-Institute-ARI-Show-Airtek-Power-Solution-recently-made-a-significant-mark-at-the-prestigious-American-Refrigeration-Institute-ARH-Show-he-id48581216.html",
    category: "company-event",
  },
];

function decodeHtml(value) {
  return value
    .replaceAll("&amp;", "&")
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'")
    .replaceAll("\\/", "/");
}

function stripTags(value) {
  return decodeHtml(value.replace(/<[^>]*>/g, " ")).replace(/\s+/g, " ").trim();
}

function normalizeUrl(raw, pageUrl) {
  let value = decodeHtml(raw.trim()).replace(/^url\((.*)\)$/i, "$1");
  value = value.replace(/^["']|["']$/g, "").trim();
  if (!value || /^(data:|blob:|javascript:|about:|#)/i.test(value)) return null;
  try {
    const url = new URL(value, pageUrl);
    url.hash = "";
    return url;
  } catch {
    return null;
  }
}

function isContentAsset(url) {
  const hostname = url.hostname.toLowerCase();
  const pathname = url.pathname.toLowerCase();
  const allowedHost =
    hostname === "airtekpower.com" ||
    hostname === "www.airtekpower.com" ||
    hostname.endsWith(".ldycdn.com");
  if (!allowedHost) return false;
  if (
    pathname.includes("/static/assets/") ||
    pathname.includes("/concat/") ||
    pathname.includes("/develop/") ||
    pathname.includes("/theme/") ||
    pathname.includes("/site-res/") ||
    pathname.includes("/phoenix/")
  ) {
    return false;
  }
  return (
    pathname.includes("/cloud/") ||
    pathname.includes("/upload/") ||
    /\.(?:avif|bmp|gif|ico|jpe?g|png|svg|webp|pdf|docx?|xlsx?|pptx?|zip|rar|mp4|mov|webm)$/i.test(
      pathname,
    )
  );
}

function canonicalUrl(url) {
  const result = new URL(url);
  for (const key of [...result.searchParams.keys()]) {
    if (/^(?:x-oss-process|imageview2?|image_process|resize|width|height|w|h)$/i.test(key)) {
      result.searchParams.delete(key);
    }
  }
  return result.href;
}

function attributesFromTag(tag) {
  const attributes = {};
  for (const match of tag.matchAll(/([\w:-]+)\s*=\s*(["'])(.*?)\2/gs)) {
    attributes[match[1].toLowerCase()] = decodeHtml(match[3]);
  }
  return attributes;
}

function addReference(found, rawUrl, page, metadata) {
  const url = normalizeUrl(rawUrl, page.url);
  if (!url || !isContentAsset(url)) return;
  const normalized = canonicalUrl(url);
  const references = found.get(normalized) ?? [];
  references.push({
    pageUrl: page.url,
    pageCategory: page.category,
    sourceKind: metadata.sourceKind,
    alt: metadata.alt || "",
    title: metadata.title || "",
    context: metadata.context || "",
  });
  found.set(normalized, references);
}

function extractReferences(html, page) {
  const found = new Map();
  for (const match of html.matchAll(/<img\b[^>]*>/gis)) {
    const attributes = attributesFromTag(match[0]);
    for (const key of ["data-original", "data-src", "src"]) {
      if (!attributes[key]) continue;
      addReference(found, attributes[key], page, {
        sourceKind: `img:${key}`,
        alt: attributes.alt,
        title: attributes.title,
        context: stripTags(html.slice(Math.max(0, match.index - 180), match.index + 180)),
      });
    }
    for (const entry of (attributes.srcset ?? "").split(",")) {
      addReference(found, entry.trim().split(/\s+/)[0], page, {
        sourceKind: "img:srcset",
        alt: attributes.alt,
        title: attributes.title,
      });
    }
  }
  for (const match of html.matchAll(/<(?:video|source)\b[^>]*>/gis)) {
    const attributes = attributesFromTag(match[0]);
    for (const key of ["src", "data-src", "poster"]) {
      if (!attributes[key]) continue;
      addReference(found, attributes[key], page, {
        sourceKind: `media:${key}`,
        title: attributes.title,
        context: stripTags(html.slice(Math.max(0, match.index - 180), match.index + 180)),
      });
    }
  }
  for (const match of html.matchAll(/<a\b[^>]*>/gis)) {
    const attributes = attributesFromTag(match[0]);
    if (!attributes.href) continue;
    addReference(found, attributes.href, page, {
      sourceKind: "link:href",
      title: attributes.title,
      context: stripTags(html.slice(Math.max(0, match.index - 120), match.index + 220)),
    });
  }
  for (const match of html.matchAll(/url\(\s*(["']?[^)'"\s]+["']?)\s*\)/gi)) {
    addReference(found, match[1], page, {
      sourceKind: "css:url",
      context: stripTags(html.slice(Math.max(0, match.index - 140), match.index + 180)),
    });
  }
  for (const match of html.matchAll(/(?:https?:)?\/\/[^\s"'<>\\)]+/gi)) {
    addReference(found, match[0], page, { sourceKind: "embedded-url" });
  }
  return found;
}

function parseCurlOutput(stdout, fallbackUrl, allowPartial = false) {
  if (!Buffer.isBuffer(stdout) || stdout.length === 0) return null;
  const marker = Buffer.from(`\n${CURL_MARKER}\t`);
  const markerIndex = stdout.lastIndexOf(marker);
  if (markerIndex === -1) {
    return allowPartial
      ? { body: stdout, finalUrl: fallbackUrl, contentType: "", size: stdout.length }
      : null;
  }
  const metadata = stdout.subarray(markerIndex + marker.length).toString().trim();
  const [finalUrl, contentType, sizeText] = metadata.split("\t");
  return {
    body: stdout.subarray(0, markerIndex),
    finalUrl,
    contentType,
    size: Number(sizeText),
  };
}

async function curl(url, purpose) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      const { stdout } = await execFileAsync(
        "curl",
        [
          "--compressed",
          "--location",
          "--silent",
          "--show-error",
          "--fail-with-body",
          "--max-time",
          "45",
          "--max-filesize",
          String(MAX_ASSET_BYTES),
          "--user-agent",
          USER_AGENT,
          "--write-out",
          `\\n${CURL_MARKER}\\t%{url_effective}\\t%{content_type}\\t%{size_download}\\n`,
          url,
        ],
        { encoding: "buffer", maxBuffer: MAX_ASSET_BYTES + 1024 * 1024 },
      );
      const parsed = parseCurlOutput(stdout, url);
      if (!parsed) throw new Error("curl response metadata missing");
      return parsed;
    } catch (error) {
      const partialResponse =
        error?.code === 23 || /curl: \(23\)/.test(error?.stderr ?? "")
          ? parseCurlOutput(error.stdout, url, true)
          : null;
      if (partialResponse) return partialResponse;
      lastError = error;
      if (attempt < 3) await new Promise((resolve) => setTimeout(resolve, 400 * attempt));
    }
  }
  throw new Error(`${purpose}: ${lastError?.message ?? "unknown error"}`);
}

async function runPool(items, worker) {
  let cursor = 0;
  await Promise.all(
    Array.from({ length: CONCURRENCY }, async () => {
      while (cursor < items.length) {
        const index = cursor;
        cursor += 1;
        await worker(items[index], index);
      }
    }),
  );
}

function extensionFor(mime, url) {
  const urlExtension = path.extname(new URL(url).pathname).toLowerCase();
  if (/^\.[a-z0-9]{2,5}$/.test(urlExtension)) return urlExtension;
  const byMime = {
    "image/jpeg": ".jpg",
    "image/png": ".png",
    "image/webp": ".webp",
    "image/svg+xml": ".svg",
    "application/pdf": ".pdf",
    "video/mp4": ".mp4",
    "video/webm": ".webm",
  };
  return byMime[mime] ?? "";
}

function safeFilename(url, mime, sha256) {
  const original = decodeURIComponent(path.basename(new URL(url).pathname));
  const extension = extensionFor(mime, url);
  const stem = path
    .basename(original, path.extname(original))
    .replace(/[^a-zA-Z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 90);
  return `${sha256.slice(0, 12)}--${stem || "asset"}${extension}`;
}

async function main() {
  await mkdir(FILES_DIR, { recursive: true });
  const pages = PAGE_SCOPE.map((entry) => ({ ...entry, url: new URL(entry.path, SITE_ORIGIN).href }));
  const pageResults = [];
  const assetReferences = new Map();

  await runPool(pages, async (page) => {
    try {
      const response = await curl(page.url, "page fetch failed");
      const html = response.body.toString("utf8");
      const references = extractReferences(html, page);
      for (const [url, items] of references) {
        assetReferences.set(url, [...(assetReferences.get(url) ?? []), ...items]);
      }
      pageResults.push({ ...page, status: "downloaded", bytes: response.body.length });
      console.log(`Page ${page.category}: ${references.size} candidate URLs`);
    } catch (error) {
      pageResults.push({ ...page, status: "error", error: error.message });
      console.error(`Page ${page.category}: ${error.message}`);
    }
  });

  const candidates = [...assetReferences.keys()].sort();
  const assets = [];
  const errors = [];
  const fileByHash = new Map();
  await runPool(candidates, async (url, index) => {
    try {
      const response = await curl(url, "asset fetch failed");
      const bytes = response.body;
      const mime = (response.contentType || "application/octet-stream")
        .split(";")[0]
        .trim()
        .toLowerCase();
      const sha256 = createHash("sha256").update(bytes).digest("hex");
      let relativeFile = fileByHash.get(sha256);
      if (!relativeFile) {
        relativeFile = path.join("candidates", safeFilename(response.finalUrl, mime, sha256));
        await writeFile(path.join(OUTPUT_DIR, relativeFile), bytes);
        fileByHash.set(sha256, relativeFile);
      }
      assets.push({
        id: sha256.slice(0, 12),
        url,
        finalUrl: response.finalUrl,
        file: relativeFile,
        mime,
        bytes: bytes.length,
        sha256,
        governanceStatus: "PROVISIONAL",
        scopeStatus: "REVIEW_REQUIRED",
        references: assetReferences.get(url),
      });
    } catch (error) {
      errors.push({ url, error: error.message, references: assetReferences.get(url) });
    }
    if ((index + 1) % 20 === 0 || index + 1 === candidates.length) {
      console.log(`Assets ${index + 1}/${candidates.length}; downloaded ${assets.length}`);
    }
  });

  assets.sort((a, b) => a.file.localeCompare(b.file) || a.url.localeCompare(b.url));
  errors.sort((a, b) => a.url.localeCompare(b.url));
  pageResults.sort((a, b) => a.url.localeCompare(b.url));
  const manifest = {
    source: SITE_ORIGIN,
    capturedAt: new Date().toISOString(),
    sourceAuthority: "Public legacy website; candidate evidence only",
    publicationRule: "Do not publish until rights, current brand approval, and media safety review pass",
    scope: {
      included: PAGE_SCOPE,
      excluded: [
        "product pages and category pages",
        "product launch news",
        "product photos, specification drawings, PQ curves, and catalogs",
        "platform scripts, styles, fonts, trackers, and captcha resources",
      ],
    },
    counts: {
      scopedPages: pages.length,
      downloadedPages: pageResults.filter((page) => page.status === "downloaded").length,
      pageErrors: pageResults.filter((page) => page.status === "error").length,
      candidateUrls: candidates.length,
      downloadedUrls: assets.length,
      uniqueFiles: fileByHash.size,
      assetErrors: errors.length,
    },
    pages: pageResults,
    errors,
    assets,
  };
  await writeFile(path.join(OUTPUT_DIR, "candidate-manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(JSON.stringify(manifest.counts));
}

main().catch(async (error) => {
  try {
    await mkdir(OUTPUT_DIR, { recursive: true });
    await writeFile(path.join(OUTPUT_DIR, "fatal-error.txt"), `${error.stack ?? error}\n`);
  } catch {}
  console.error(error);
  process.exitCode = 1;
});
