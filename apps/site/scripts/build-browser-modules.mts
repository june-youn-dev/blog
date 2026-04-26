import { mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { build } from "esbuild";

type BuildEntry = {
  input: string;
  output: string;
};

const entries: BuildEntry[] = [
  {
    input: resolve(import.meta.dirname, "..", "src", "js", "admin-auth.mts"),
    output: resolve(import.meta.dirname, "..", "src", "js", "admin-auth.js"),
  },
];

for (const entry of entries) {
  await mkdir(dirname(entry.output), { recursive: true });

  await build({
    entryPoints: [entry.input],
    outfile: entry.output,
    bundle: true,
    format: "esm",
    platform: "browser",
    target: "es2022",
    sourcemap: false,
    logLevel: "silent",
    external: ["https://www.gstatic.com/*"],
  });
}

console.log(`Built ${entries.length} browser module(s).`);
