/**
 * YAML for widgets (UI-571): `parseYaml(text)` gives the values of a YAML text's documents with the `yaml` package
 * (YAML 1.2, aliases capped against alias bombs), loaded on first use as its own chunk of the bundle, or its first
 * error with the line and column (from 1) where it is.
 */

export type YamlResult =
  | { readonly ok: true; readonly documents: readonly unknown[] }
  | { readonly ok: false; readonly message: string; readonly line: number; readonly column: number };

/** How many times the documents may repeat an alias's node (the `yaml` package's default). */
const MAX_ALIAS_COUNT = 100;

/** `yaml` appends the position (and the lines around it) to its messages: the position is given apart. */
const POSITION = /(?: at line \d+, column \d+)?:?\n[\s\S]*$/;

export async function parseYaml(text: string): Promise<YamlResult> {
  // Dynamic: a chunk loaded only when a YAML file is shown, as the editor's languages (S-13).
  const { parseAllDocuments, YAMLError } = await import("yaml");
  const documents = parseAllDocuments(text);
  const list = Array.isArray(documents) ? documents : [];
  const error = list.flatMap((document) => document.errors)[0];
  if (error !== undefined) {
    const position = error.linePos?.[0];
    return {
      ok: false,
      message: error.message.replace(POSITION, ""),
      line: position?.line ?? 1,
      column: position?.col ?? 1,
    };
  }
  try {
    return { ok: true, documents: list.map((document) => document.toJS({ maxAliasCount: MAX_ALIAS_COUNT })) };
  } catch (thrown) {
    // An alias bomb: the count of an alias's repeats is over the cap.
    if (thrown instanceof ReferenceError || thrown instanceof YAMLError) {
      return { ok: false, message: thrown.message, line: 1, column: 1 };
    }
    throw thrown;
  }
}
