import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import ts from "typescript";

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
  const source = await readFile(entry.input, "utf8");
  const result = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ES2022,
      target: ts.ScriptTarget.ES2022,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      verbatimModuleSyntax: true,
      strict: true,
    },
    fileName: entry.input,
    reportDiagnostics: true,
  });

  const diagnostics = result.diagnostics ?? [];
  const hardErrors = diagnostics.filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error);
  if (hardErrors.length > 0) {
    throw new Error(ts.formatDiagnosticsWithColorAndContext(hardErrors, {
      getCanonicalFileName: (fileName) => fileName,
      getCurrentDirectory: () => process.cwd(),
      getNewLine: () => "\n",
    }));
  }

  await mkdir(dirname(entry.output), { recursive: true });
  await writeFile(entry.output, result.outputText, "utf8");
}

console.log(`Built ${entries.length} browser module(s).`);
