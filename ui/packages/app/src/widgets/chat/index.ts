/** The conversation widgets (`omfp:chat`, `omfp:transcript`, `omfp:interactions`, `omfp:composer`). */

import type { ComponentType } from "react";
import { ChatWidget as Chat } from "./chat.tsx";
import { ComposerWidget as Composer } from "./composer.tsx";
import { InteractionsWidget as Interactions } from "./interactions.tsx";
import type { WidgetProps } from "./props.ts";
import { TranscriptWidget as Transcript } from "./transcript.tsx";

/** `omfp:chat`: transcript with the pending interactions inline, queue chips and composer. */
export const ChatWidget: ComponentType<WidgetProps> = Chat;
/** `omfp:transcript`: the virtualized transcript of the selected thread. */
export const TranscriptWidget: ComponentType<WidgetProps> = Transcript;
/** `omfp:interactions`: the selected thread's pending requests and closed rows. */
export const InteractionsWidget: ComponentType<WidgetProps> = Interactions;
/** `omfp:composer`: queue chips and the composer of the selected thread. */
export const ComposerWidget: ComponentType<WidgetProps> = Composer;
