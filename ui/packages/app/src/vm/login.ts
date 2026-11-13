/**
 * Logging in to a provider from the window (UI-380..UI-389): which texts of omp say it needs a login, what `:login`
 * and `:logout` complete, and the messages of a login's and a logout's outcome.
 */

import type { AuthProvider, LoggedOut } from "@omfp/protocol";
import { strings } from "../strings/index.ts";

const text = strings.shell.login;

/**
 * omp's words for "no usable model or credential" (UI-381): its startup refusal (`No default model selected. Use
 * /login, …`), the prompt preflight (`No API key found for <provider>.`, `No model selected.`), the startup checks
 * (`No models available.`, `No model available matching enabledModels (…) with usable credentials.`) and every hint
 * naming `/login` as a word of its own (not a path inside a URL).
 */
const NEEDS_LOGIN =
  /(?:^|\s)\/login\b|\bNo API key found\b|\bNo default model selected\b|\bNo model selected\b|\bNo models available\b|\bNo model available matching enabledModels\b/i;

/** Whether `message` (an error, a failed start's message or stderr tail) says omp has no model it can use without a
 * login or a key (UI-381). */
export function needsLogin(message: string | null | undefined): boolean {
  return message !== null && message !== undefined && NEEDS_LOGIN.test(message);
}

/** `:login`'s completions (UI-386): the ids of the providers omp can log in to here, for the first argument only. */
export function loginCompletions(
  providers: readonly AuthProvider[] | undefined,
  before: readonly string[],
): readonly string[] {
  return before.length > 0 || providers === undefined
    ? []
    : providers.filter((provider) => provider.available).map((provider) => provider.id);
}

/** `:logout`'s completions (UI-386): the ids of the providers omp holds a credential for, for the first argument only. */
export function logoutCompletions(
  providers: readonly AuthProvider[] | undefined,
  before: readonly string[],
): readonly string[] {
  return before.length > 0 || providers === undefined
    ? []
    : providers.filter((provider) => provider.authenticated).map((provider) => provider.id);
}

/** The name `providerId` shows as: its name from `auth.providers`, else the id itself. */
export function providerName(providers: readonly AuthProvider[] | undefined, providerId: string): string {
  return providers?.find((provider) => provider.id === providerId)?.name ?? providerId;
}

/** The toast of `:logout`'s result (UI-386): what omp removed and, when something else still authenticates the
 * provider (an environment variable, omp's config), that source. */
export function logoutSummary(result: LoggedOut, name: string): string {
  const removed = result.removed === 0 ? text.removedNone(name) : text.removed(name, result.removed);
  const remaining = result.remainingSource;
  return remaining === null || remaining === undefined || remaining === ""
    ? removed
    : `${removed} ${text.stillAuthenticated(name, remaining)}`;
}

/** `providers` after a login or logout of `providerId` changed whether omp holds a credential for it. */
export function withAuthenticated(
  providers: readonly AuthProvider[],
  providerId: string,
  authenticated: boolean,
): readonly AuthProvider[] {
  return providers.map((provider) => (provider.id === providerId ? { ...provider, authenticated } : provider));
}
