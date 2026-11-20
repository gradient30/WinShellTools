import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const studioRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(studioRoot, "src-tauri");
const errors = [];

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

const packageJson = readJson(join(studioRoot, "package.json"));
if (packageJson.dependencies?.["@tauri-apps/plugin-fs"]) {
  errors.push("@tauri-apps/plugin-fs must not be a frontend dependency");
}

const cargo = readFileSync(join(tauriRoot, "Cargo.toml"), "utf8");
if (/^\s*tauri-plugin-fs\s*=/m.test(cargo)) {
  errors.push("tauri-plugin-fs must not be a Rust dependency");
}
const tauriLine = cargo.match(/^\s*tauri\s*=\s*\{([^}]+)\}/m)?.[1] ?? "";
if (/devtools/.test(tauriLine)) {
  errors.push("tauri devtools feature must not be enabled in production");
}

const config = readJson(join(tauriRoot, "tauri.conf.json"));
if (config.app?.withGlobalTauri === true) {
  errors.push("withGlobalTauri must be disabled");
}
const scriptSrc = String(config.app?.security?.csp ?? "").match(/script-src([^;]*)/i)?.[1] ?? "";
if (scriptSrc.includes("'unsafe-inline'")) {
  errors.push("script-src must not allow 'unsafe-inline'");
}

const capability = readJson(join(tauriRoot, "capabilities", "main.json"));
if ((capability.permissions ?? []).some((permission) => String(permission).startsWith("fs:"))) {
  errors.push("filesystem capabilities must not be exposed to the WebView");
}

const lib = readFileSync(join(tauriRoot, "src", "lib.rs"), "utf8");
if (lib.includes("tauri_plugin_fs")) {
  errors.push("tauri_plugin_fs must not be initialized");
}

if (errors.length > 0) {
  console.error("Security configuration check failed:");
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log("Security configuration check passed.");
