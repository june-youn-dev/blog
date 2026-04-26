import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const API_TYPES_DIR = join(import.meta.dirname, "../../site/src/api-types");

async function listTypeFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".ts"))
    .map((entry) => join(dir, entry.name))
    .sort();
}

async function snapshotBindings(): Promise<Map<string, string>> {
  const files = await listTypeFiles(API_TYPES_DIR);
  const snapshot = new Map<string, string>();

  for (const file of files) {
    const contents = await readFile(file);
    const digest = createHash("sha256").update(contents).digest("hex");
    snapshot.set(file, digest);
  }

  return snapshot;
}

function diffSnapshots(before: Map<string, string>, after: Map<string, string>): string[] {
  const paths = new Set([...before.keys(), ...after.keys()]);
  return [...paths]
    .filter((path) => before.get(path) !== after.get(path))
    .map((path) => path.replace(`${API_TYPES_DIR}/`, ""))
    .sort();
}

async function main(): Promise<void> {
  const before = await snapshotBindings();

  const cargoRun = await execFileAsync("cargo", ["test", "-p", "blog-api-core", "--lib", "export_bindings"], {
    cwd: join(import.meta.dirname, ".."),
    maxBuffer: 10 * 1024 * 1024,
  });
  if (cargoRun.stdout) {
    process.stdout.write(cargoRun.stdout);
  }
  if (cargoRun.stderr) {
    process.stderr.write(cargoRun.stderr);
  }

  const formatRun = await execFileAsync("pnpm", ["run", "bindings:format"], {
    cwd: join(import.meta.dirname, ".."),
    maxBuffer: 10 * 1024 * 1024,
  });
  if (formatRun.stdout) {
    process.stdout.write(formatRun.stdout);
  }
  if (formatRun.stderr) {
    process.stderr.write(formatRun.stderr);
  }

  const after = await snapshotBindings();
  const changed = diffSnapshots(before, after);

  if (changed.length > 0) {
    throw new Error(
      `Generated TypeScript bindings were out of date: ${changed.join(", ")}.\nRun \`pnpm run bindings:generate\` and commit the updated files.`,
    );
  }

  console.log("TypeScript bindings are up to date.");
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
