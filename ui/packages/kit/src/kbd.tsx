import type { HTMLAttributes } from "react";
import { cx } from "./class.ts";

export type KbdProps = HTMLAttributes<HTMLElement>;

/** A key chip (UI-148): a key or key sequence as lowercase words (`spc n`, `enter`), e.g. in hints and buttons. */
export function Kbd({ className, ...props }: KbdProps) {
  return <kbd {...props} className={cx("omfp-kbd", className)} />;
}
