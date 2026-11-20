import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const studioRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(studioRoot, "src-tauri");

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function auditSecurity({ packageJson, cargo, config, capability, lib }) {
  const errors = [];
  if (packageJson.dependencies?.["@tauri-apps/plugin-fs"]) {
    errors.push("@tauri-apps/plugin-fs must not be a frontend dependency");
  }
  if (/^\s*tauri-plugin-fs\s*=/m.test(cargo)) {
    errors.push("tauri-plugin-fs must not be a Rust dependency");
  }
  const tauriLine = cargo.match(/^\s*tauri\s*=\s*\{([^}]+)\}/m)?.[1] ?? "";
  if (/devtools/.test(tauriLine)) {
    errors.push("tauri devtools feature must not be enabled in production");
  }
  if (config.app?.withGlobalTauri === true) {
    errors.push("withGlobalTauri must be disabled");
  }
  const scriptSrc = String(config.app?.security?.csp ?? "").match(/script-src([^;]*)/i)?.[1] ?? "";
  if (scriptSrc.includes("'unsafe-inline'")) {
    errors.push("script-src must not allow 'unsafe-inline'");
  }
  if ((capability.permissions ?? []).some((permission) => String(permission).startsWith("fs:"))) {
    errors.push("filesystem capabilities must not be exposed to the WebView");
  }
  if (lib.includes("tauri_plugin_fs")) {
    errors.push("tauri_plugin_fs must not be initialized");
  }
  return errors;
}

function currentInputs() {
  return {
    packageJson: readJson(join(studioRoot, "package.json")),
    cargo: readFileSync(join(tauriRoot, "Cargo.toml"), "utf8"),
    config: readJson(join(tauriRoot, "tauri.conf.json")),
    capability: readJson(join(tauriRoot, "capabilities", "main.json")),
    lib: readFileSync(join(tauriRoot, "src", "lib.rs"), "utf8")
  };
}

function runSelfTest() {
  const safe = {
    packageJson: { dependencies: {} },
    cargo: 'tauri = { version = "2", features = [] }',
    config: {
      app: { withGlobalTauri: false, security: { csp: "script-src 'self'" } }
    },
    capability: { permissions: ["core:default"] },
    lib: "tauri::Builder::default()"
  };
  if (auditSecurity(safe).length !== 0) {
    throw new Error("security self-test: safe fixture unexpectedly failed");
  }
  const unsafe = {
    packageJson: { dependencies: { "@tauri-apps/plugin-fs": "1" } },
    cargo: 'tauri = { features = ["devtools"] }\ntauri-plugin-fs = "1"',
    config: {
      app: { withGlobalTauri: true, security: { csp: "script-src 'self' 'unsafe-inline'" } }
    },
    capability: { permissions: ["fs:default"] },
    lib: "tauri_plugin_fs::init()"
  };
  if (auditSecurity(unsafe).length < 6) {
    throw new Error("security self-test: unsafe fixture was not rejected");
  }
  console.log("Security configuration self-test passed.");
}

if (process.argv.includes("--self-test")) {
  runSelfTest();
} else {
  const errors = auditSecurity(currentInputs());
  if (errors.length > 0) {
    console.error("Security configuration check failed:");
    for (const error of errors) console.error(`- ${error}`);
    process.exit(1);
  }
  console.log("Security configuration check passed.");
}
