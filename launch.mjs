// Resolve the platform-specific nocmd binary from the matching optional
// dependency (npm installs only the package whose os/cpu match the host) and
// run it, forwarding stdin and stdout so the PreToolUse event reaches the
// binary and its decision is returned unchanged. Fails open (exit 0) when no
// platform package is present, so the hook never blocks the Bash tool.
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const key = `${process.platform}-${process.arch}`;
const ext = process.platform === "win32" ? ".exe" : "";

let binary;
try {
  binary = require.resolve(`@gglinnk/nocmd-${key}/nocmd${ext}`);
} catch {
  process.exit(0);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 0);
