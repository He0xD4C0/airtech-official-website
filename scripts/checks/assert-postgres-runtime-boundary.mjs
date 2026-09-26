import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const platform = path.join(root, "services/platform");

function rustFiles(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const target = path.join(directory, entry.name);
    if (entry.isDirectory()) return rustFiles(target);
    return entry.isFile() && entry.name.endsWith(".rs") ? [target] : [];
  });
}

const failures = [];
const files = [
  ...rustFiles(path.join(platform, "crates")),
  ...rustFiles(path.join(platform, "apps")),
  ...rustFiles(path.join(platform, "tests")),
];
const forbidden = [
  ["PlatformData", "legacy in-memory business store"],
  ["AppState::for_test", "database-free application state"],
  ["Option<PgPool>", "optional PostgreSQL runtime"],
  ["state.pool.is_none()", "in-memory persistence fallback"],
  ["state.pool.is_some()", "conditional persistence behavior"],
  ["if let Some(pool) = &state.pool", "conditional persistence behavior"],
  ["let Some(pool) = &state.pool", "conditional persistence behavior"],
  ["state.data.", "process-local business data"],
  ["self.data.", "process-local business data"],
];

for (const file of files) {
  const source = fs.readFileSync(file, "utf8");
  for (const [needle, reason] of forbidden) {
    if (source.includes(needle)) {
      failures.push(`${path.relative(root, file)}: ${reason} (${needle})`);
    }
  }
}

const api = fs.readFileSync(path.join(platform, "apps/api/src/main.rs"), "utf8");
const worker = fs.readFileSync(path.join(platform, "apps/worker/src/main.rs"), "utf8");
const maintenance = fs.readFileSync(path.join(platform, "apps/maintenance/src/main.rs"), "utf8");
for (const [name, source] of [["api", api], ["worker", worker]]) {
  if (!source.includes("verify_runtime_ready")) {
    failures.push(`${name}: startup must verify the preparation marker`);
  }
  for (const forbiddenStartup of ["migrate_legacy_content", "cms_dependency_backfill::run", "runtime_preparation::prepare"]) {
    if (source.includes(forbiddenStartup)) {
      failures.push(`${name}: startup performs maintenance work (${forbiddenStartup})`);
    }
  }
}
if (!maintenance.includes("runtime_preparation::prepare")) {
  failures.push("maintenance: prepare-runtime does not own runtime preparation");
}

if (failures.length > 0) {
  console.error("PostgreSQL runtime boundary check failed:\n" + failures.map((value) => `- ${value}`).join("\n"));
  process.exit(1);
}

console.log(`PostgreSQL runtime boundary verified across ${files.length} Rust files.`);
