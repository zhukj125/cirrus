import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

function hostTarget() {
  const details = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const host = details.split(/\r?\n/).find((line) => line.startsWith("host: "));
  if (!host) throw new Error("rustc did not report a host target");
  return host.slice("host: ".length).trim();
}

const target = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET || hostTarget();
const debug = process.argv.includes("--debug");
const profile = debug ? "debug" : "release";
const extension = process.platform === "win32" ? ".exe" : "";
const targetRoot = resolve(process.env.CARGO_TARGET_DIR || "target");
const destinationDir = join("src-tauri", "binaries");

execFileSync("cargo", ["build", "-p", "cirrus-mcp", "--target", target, ...(debug ? [] : ["--release"])], {
  stdio: "inherit",
});
mkdirSync(destinationDir, { recursive: true });

for (const executable of ["cirrus-mcp", "cirrus-mcp-server"]) {
  const source = join(targetRoot, target, profile, `${executable}${extension}`);
  const destination = join(destinationDir, `${executable}-${target}${extension}`);
  copyFileSync(source, destination);
  console.log(`MCP sidecar written to ${destination}`);
}
