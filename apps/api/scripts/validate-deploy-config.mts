import { readFile } from "node:fs/promises";
import { join } from "node:path";

const relativeConfigPath = process.argv[2] || "public/wrangler.toml";
const WRANGLER_TOML_PATH = join(import.meta.dirname, "..", relativeConfigPath);
const NIL_UUID = "00000000-0000-0000-0000-000000000000";

const wranglerToml = await readFile(WRANGLER_TOML_PATH, "utf8");
const databaseIdMatch = wranglerToml.match(/^\s*database_id\s*=\s*"([^"]+)"\s*$/m);

if (!databaseIdMatch) {
  throw new Error(`${relativeConfigPath} does not define a D1 database_id.`);
}

const databaseId = databaseIdMatch[1].trim();

if (databaseId === NIL_UUID) {
  throw new Error(
    [
      `${relativeConfigPath} still contains the nil UUID placeholder for D1.`,
      "Replace [[d1_databases]].database_id with the real production D1 UUID before deployment.",
    ].join(" "),
  );
}

console.log(
  `Deployment config check passed for ${relativeConfigPath}: D1 database_id is set to ${databaseId}.`,
);
