import type { AiChatResult } from "./api";

export type AiStreamEvent =
  | { type: "start" }
  | { type: "delta"; text: string }
  | { type: "reasoning"; text: string }
  | { type: "validation"; result: AiChatResult }
  | { type: "done" }
  | { type: "error"; message: string };

export interface AiStreamState {
  active: boolean;
  text: string;
  reasoningChars: number;
  validation: AiChatResult | null;
  done: boolean;
  error: string | null;
}

export function initialStreamState(): AiStreamState {
  return {
    active: false,
    text: "",
    reasoningChars: 0,
    validation: null,
    done: false,
    error: null
  };
}

export function applyStreamEvent(
  state: AiStreamState,
  event: AiStreamEvent
): AiStreamState {
  switch (event.type) {
    case "start":
      return { ...initialStreamState(), active: true };
    case "delta":
      return { ...state, active: true, text: state.text + event.text };
    case "reasoning":
      return {
        ...state,
        active: true,
        reasoningChars: state.reasoningChars + event.text.length
      };
    case "validation":
      return { ...state, validation: event.result };
    case "done":
      return { ...state, active: false, done: true, error: null };
    case "error":
      return { ...state, active: false, error: event.message };
  }
}
