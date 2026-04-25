import { rm } from "node:fs/promises";
import { resolve } from "node:path";

const generatedPaths = [
  resolve(import.meta.dirname, "..", "_site"),
  resolve(import.meta.dirname, "..", "src", "posts"),
  resolve(import.meta.dirname, "..", "src", "permalinks"),
  resolve(import.meta.dirname, "..", "src", "js", "admin-auth.js"),
] as const;

for (const target of generatedPaths) {
  await rm(target, {
    recursive: true,
    force: false,
  }).catch((error: NodeJS.ErrnoException) => {
    if (error.code === "ENOENT") {
      return;
    }
    throw error;
  });
}

console.log(`Removed ${generatedPaths.length} generated path(s).`);
