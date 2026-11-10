/**
 * Frames on the WebSocket (mirrors `omfp_protocol::envelope`): client requests, and daemon responses, events and
 * `resync_required`. Hand-written: their serde forms are custom, so they have no JSON Schema.
 */

import type { ThreadEvent } from "@omfp/core";
import type { ErrorBody, IndexEvent, MethodName, ParamsOf, Topic, WorkspaceEvent } from "./generated.ts";

/** A client request (PRT-010); `id` is chosen by the client, unique per connection, and echoed by the response. */
export type Request<M extends MethodName = MethodName> = {
  [K in M]: { type: "request"; id: string; method: K; params: ParamsOf<K> };
}[M];

/** A successful response: `result` is the method's result (`ResultOf<M>` of the request's method). */
export type SuccessResponse = { type: "response"; id: string; ok: true; result: unknown };

/** A failed response (PRT-011). */
export type FailureResponse = { type: "response"; id: string; ok: false; error: ErrorBody };

/** The answer to a request. */
export type Response = SuccessResponse | FailureResponse;

/** The `event` of an event frame; its kind follows from the topic. */
export type TopicEvent = IndexEvent | ThreadEvent | WorkspaceEvent;

/** One event of a topic (PRT-022): `seq` is strictly increasing, without gaps, within an `epoch`. */
export type EventFrame = { type: "event"; topic: Topic; epoch: string; seq: number; event: TopicEvent };

/** The client's queue overflowed: the topic was dropped and must be resubscribed (PRT-023). */
export type ResyncRequired = { type: "resync_required"; topic: Topic };

/** A frame from the daemon. */
export type ServerFrame = Response | EventFrame | ResyncRequired;

/** A request frame for `method`. */
export function request<M extends MethodName>(id: string, method: M, params: ParamsOf<M>): Request<M> {
  // The mapped type collapses to this object for a single method; TypeScript cannot see it through the generic.
  return { type: "request", id, method, params } as Request<M>;
}
