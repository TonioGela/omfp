/** Props every widget receives (the view node's props). */
export type WidgetProps = Readonly<Record<string, unknown>>;

/** A `data-testid` for the inner element of a kit field (`inputProps`), which takes no `data-*` literal. */
export function testId(id: string): { readonly [key: `data-${string}`]: string } {
  return { "data-testid": id };
}
