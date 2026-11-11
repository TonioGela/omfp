/**
 * Release bundle of the web UI (`just build-web`) into `dist/`, which the daemon serves (DMN-008).
 *
 * One `Bun.build` with code splitting over the app entry and one entry per module the import map gives widgets
 * (`react`, `react/jsx-runtime`, `react-dom`, `omfp/sdk`, `omfp/kit`, AGT-012), so the app and every widget share
 * one React and one SDK instance. React is CommonJS: its entries re-export its named exports, listed at build time.
 * Besides the bundle, `dist/` gets:
 * - `index.html`: the import map, the bundle's CSS, then the user theme (`/theme.css`, UI-091, so its tokens win), and
 *   the app entry;
 * - `widget-exports.json`: every export name of each import-map module, which the daemon checks widget imports against
 *   (DMN-085);
 * - `builtin-widgets/<name>/`: the source of each built-in widget as a copyable widget folder (AGT-021, DMN-087).
 */
import { copyFile, mkdir, readdir, rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { basename, join } from "node:path";

const require = createRequire(import.meta.url);
const root = import.meta.dir;
const outdir = join(root, "dist");
const entries = join(root, ".entries");
const THEME_LINK = '<link rel="stylesheet" href="/theme.css" data-omfp-theme>';

/** An import-map module: its specifier, the name of its bundle entry, and how its entry is generated. */
interface Shared {
  readonly specifier: string;
  readonly name: string;
  /** CommonJS package re-exported name by name, or an ES module re-exported with `export *`. */
  readonly source: { readonly kind: "commonjs" } | { readonly kind: "esm"; readonly path: string };
}

const SHARED: readonly Shared[] = [
  { specifier: "react", name: "react", source: { kind: "commonjs" } },
  { specifier: "react/jsx-runtime", name: "jsx-runtime", source: { kind: "commonjs" } },
  { specifier: "react-dom", name: "react-dom", source: { kind: "commonjs" } },
  { specifier: "omfp/sdk", name: "sdk", source: { kind: "esm", path: join(root, "src/sdk/index.ts") } },
  { specifier: "omfp/kit", name: "kit", source: { kind: "esm", path: join(root, "../kit/src/index.ts") } },
];

/** Built-in widgets shipped as folders: `<name>` ← `src/widgets/<folder>`, with `part` as the entry's default export
 * when the folder's own entry is another widget. */
const BUILTINS: readonly { name: string; folder: string; part?: { file: string; exportName: string } }[] = [
  { name: "header", folder: "header" },
  { name: "workspace", folder: "workspace" },
  { name: "chat", folder: "chat" },
  { name: "transcript", folder: "chat", part: { file: "transcript.tsx", exportName: "TranscriptWidget" } },
  { name: "interactions", folder: "chat", part: { file: "interactions.tsx", exportName: "InteractionsWidget" } },
  { name: "composer", folder: "chat", part: { file: "composer.tsx", exportName: "ComposerWidget" } },
];

const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;

/** Export names of a shared module, `default` included when it has one. */
async function exportNames(shared: Shared): Promise<string[]> {
  if (shared.source.kind === "commonjs") {
    const names = Object.keys(require(shared.specifier)).filter((name) => name !== "default" && IDENTIFIER.test(name));
    return [...names, "default"].sort();
  }
  // The module to inspect comes from the SHARED table.
  return Object.keys(await import(shared.source.path)).sort();
}

function entrySource(shared: Shared, names: readonly string[]): string {
  if (shared.source.kind === "esm") {
    return `export * from ${JSON.stringify(shared.source.path)};\n`;
  }
  const named = names.filter((name) => name !== "default");
  return [
    `import * as M from ${JSON.stringify(shared.specifier)};`,
    "const m = M as unknown as Record<string, unknown>;",
    `export const { ${named.join(", ")} } = m;`,
    "export default m.default ?? M;",
    "",
  ].join("\n");
}

await rm(outdir, { recursive: true, force: true });
await rm(entries, { recursive: true, force: true });
await mkdir(entries, { recursive: true });

const exportsBySpecifier: Record<string, string[]> = {};
const entrypoints = [join(root, "src/main.tsx")];
// The generated entries live in the package, where `react` resolves; they go away whatever happens.
let result: Awaited<ReturnType<typeof Bun.build>>;
try {
  for (const shared of SHARED) {
    const names = await exportNames(shared);
    exportsBySpecifier[shared.specifier] = names;
    const path = join(entries, `${shared.name}.ts`);
    await Bun.write(path, entrySource(shared, names));
    entrypoints.push(path);
  }
  result = await Bun.build({
    entrypoints,
    outdir,
    splitting: true,
    format: "esm",
    minify: true,
    publicPath: "/",
    sourcemap: "linked",
    naming: { entry: "[name]-[hash].[ext]", chunk: "chunk-[hash].[ext]", asset: "[name]-[hash].[ext]" },
    define: { "process.env.NODE_ENV": JSON.stringify("production") },
  });
} finally {
  await rm(entries, { recursive: true, force: true });
}
if (!result.success) {
  for (const log of result.logs) {
    console.error(log);
  }
  process.exit(1);
}

/** The URL of the entry chunk built from `<name>.ts`/`.tsx`. */
function entryUrl(name: string): string {
  const output = result.outputs.find(
    (candidate) => candidate.kind === "entry-point" && basename(candidate.path).startsWith(`${name}-`),
  );
  if (output === undefined) {
    throw new Error(`no entry chunk for ${name}`);
  }
  return `/${basename(output.path)}`;
}

const importMap = { imports: Object.fromEntries(SHARED.map((shared) => [shared.specifier, entryUrl(shared.name)])) };
const styles = result.outputs
  .filter((output) => output.path.endsWith(".css"))
  .map((output) => `<link rel="stylesheet" href="/${basename(output.path)}">`);
await Bun.write(
  join(outdir, "index.html"),
  [
    "<!doctype html>",
    '<html lang="en">',
    "<head>",
    '<meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1">',
    "<title>omfp</title>",
    `<script type="importmap">${JSON.stringify(importMap)}</script>`,
    ...styles,
    THEME_LINK,
    "</head>",
    "<body>",
    '<div id="root"></div>',
    `<script type="module" src="${entryUrl("main")}"></script>`,
    "</body>",
    "</html>",
    "",
  ].join("\n"),
);
await Bun.write(join(outdir, "widget-exports.json"), `${JSON.stringify(exportsBySpecifier, null, 2)}\n`);

for (const { name, folder, part } of BUILTINS) {
  const from = join(root, "src/widgets", folder);
  const to = join(outdir, "builtin-widgets", name);
  await mkdir(to, { recursive: true });
  for (const file of await readdir(from)) {
    // Widgets are TypeScript modules; the folder's CSS is part of the app's stylesheet.
    if (/\.(ts|tsx)$/.test(file)) {
      await copyFile(join(from, file), join(to, file));
    }
  }
  if (part !== undefined) {
    await Bun.write(
      join(to, "index.ts"),
      [
        `/** \`omfp:${name}\`, copied from the built-in chat folder (AGT-021). */`,
        "",
        `import { ${part.exportName} } from "./${part.file}";`,
        "",
        `export default ${part.exportName};`,
        "",
      ].join("\n"),
    );
  }
}
