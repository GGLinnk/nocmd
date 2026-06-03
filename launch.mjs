// Resolve the platform-specific nocmd binary bundled under bin/ and run it,
// forwarding stdin and stdout so the PreToolUse event reaches the binary and
// its decision is returned unchanged. Fails open (exit 0) when the platform is
// unsupported or the binary is absent, so the hook never blocks the Bash tool.
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(fileURLToPath(import.meta.url));

const TARGET = {
  "win32-x64": "x86_64-pc-windows-msvc",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
};

const triple = TARGET[`${process.platform}-${process.arch}`];
if (!triple) {
  process.exit(0);
}

const ext = process.platform === "win32" ? ".exe" : "";
const binary = join(root, "bin", `nocmd-${triple}${ext}`);

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 0);
