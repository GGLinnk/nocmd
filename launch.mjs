// Run the platform-specific nocmd binary, forwarding stdin/stdout/exit so the
// PreToolUse event reaches it and its decision is returned unchanged. The
// platform package and binary names are read from this package's own
// package.json (generated from Cargo.toml), so nothing here is hardcoded. Fails
// open (exit 0) when no matching platform package is installed.
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(fileURLToPath(import.meta.url));
const key = `${process.platform}-${process.arch}`;
const ext = process.platform === "win32" ? ".exe" : "";

let pkg;
try {
  pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
} catch {
  process.exit(0);
}

const dependency = Object.keys(pkg.optionalDependencies ?? {}).find((name) => name.endsWith(`-${key}`));
const binaryName = (pkg.name ?? "").split("/").pop();
if (!dependency || !binaryName) {
  process.exit(0);
}

const require = createRequire(import.meta.url);
let binary;
try {
  binary = require.resolve(`${dependency}/${binaryName}${ext}`);
} catch {
  process.exit(0);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 0);
