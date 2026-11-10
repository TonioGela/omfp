/**
 * Release bundle of the web UI (`just build-web`): `src/index.html` and everything it imports, minified, into `dist/`,
 * which the daemon serves (DMN-008). The user theme is not part of the bundle: the daemon serves it at `/theme.css`
 * (UI-091), linked after the bundle's own styles so its tokens win.
 */
import { rm } from "node:fs/promises";
import { join } from "node:path";

const outdir = join(import.meta.dir, "dist");
const THEME_LINK = '<link rel="stylesheet" href="/theme.css" data-omfp-theme>';

await rm(outdir, { recursive: true, force: true });
const result = await Bun.build({
  entrypoints: [join(import.meta.dir, "src/index.html")],
  outdir,
  minify: true,
  publicPath: "/",
  sourcemap: "linked",
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
});
if (!result.success) {
  for (const log of result.logs) {
    console.error(log);
  }
  process.exit(1);
}
const index = Bun.file(join(outdir, "index.html"));
const html = await index.text();
if (!html.includes("</head>")) {
  throw new Error("dist/index.html has no </head>");
}
await Bun.write(index, html.replace("</head>", `${THEME_LINK}</head>`));
