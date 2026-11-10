/**
 * Types of the UI ↔ daemon protocol (docs/specs/03-protocol.md).
 *
 * - `generated.ts`: params, results, DTOs, topic events and errors, generated from the daemon's JSON Schemas by
 *   `just gen-ts` (AGT-007), with the `Methods` table (method name → params and result types).
 * - `envelope.ts`: request, response, event and `resync_required` frames.
 * - `topic.ts`: topic names.
 */

export * from "./envelope.ts";
export * from "./generated.ts";
export * from "./topic.ts";
