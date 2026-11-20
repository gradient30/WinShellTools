import { describe, expect, it } from "vitest";
import { applyStreamEvent, initialStreamState } from "./aiStream";

describe("ai stream state", () => {
  it("appends delta before the final validation", () => {
    let state = initialStreamState();
    state = applyStreamEvent(state, { type: "start" });
    state = applyStreamEvent(state, { type: "delta", text: "hello" });
    expect(state.text).toBe("hello");
    expect(state.validation).toBeNull();
    expect(state.done).toBe(false);
  });

  it("moves errors to a terminal state", () => {
    let state = applyStreamEvent(initialStreamState(), { type: "start" });
    state = applyStreamEvent(state, { type: "error", message: "timeout" });
    expect(state.active).toBe(false);
    expect(state.error).toBe("timeout");
  });
});
