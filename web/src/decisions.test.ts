import { describe, expect, it } from "vitest";
import type { Group } from "./api";
import { type Action, counts, initial, keyAction, merges, reduce, type ReviewState } from "./decisions";

const group = (tier: Group["tier"], members: number[]): Group => ({
  tier,
  score: 1,
  members,
  pairs: [],
});

const groups = [
  group("sure", [0, 1]),
  group("likely", [2, 3, 4]),
  group("sure", [5, 6]),
  group("check", [7, 8]),
];

const run = (...actions: Action[]): ReviewState =>
  actions.reduce((s, a) => reduce(groups, s, a), initial);

describe("review", () => {
  it("moves to the next undecided group after a decision", () => {
    const s = run({ type: "merge" });
    expect(s.decisions).toEqual({ 0: "merge" });
    expect(s.current).toBe(1);
    const t = reduce(groups, { ...s, current: 3 }, { type: "apart" });
    // Wraps round to the first group still open.
    expect(t.current).toBe(1);
  });

  it("accepts every sure group at once and lands on the first open one", () => {
    const s = run({ type: "acceptSure" });
    expect(s.decisions).toEqual({ 0: "merge", 2: "merge" });
    expect(s.current).toBe(1);
    expect(counts(groups, s)).toEqual({ merge: 2, apart: 0, open: 2 });
  });

  it("leaves sure groups the user already decided alone", () => {
    const s = run({ type: "apart" }, { type: "acceptSure" });
    expect(s.decisions[0]).toBe("apart");
    expect(s.decisions[2]).toBe("merge");
  });

  it("splits a contact out and exports the rest", () => {
    const s = run({ type: "go", to: 1 }, { type: "split", id: 3 }, { type: "merge" });
    expect(merges(groups, s)).toEqual([{ members: [2, 4], choices: {} }]);
  });

  it("can't merge a group that's down to one contact", () => {
    const s = run({ type: "split", id: 1 }, { type: "merge" });
    expect(s.decisions[0]).toBe("apart");
    expect(merges(groups, s)).toEqual([]);
  });

  it("drops a field pick when its contact is split out", () => {
    const s = run(
      { type: "go", to: 1 },
      { type: "choose", field: "name", id: 3 },
      { type: "choose", field: "photo", id: 4 },
      { type: "split", id: 3 },
    );
    expect(s.drafts[1]?.choices).toEqual({ photo: 4 });
  });

  it("undoes decisions, picks and bulk accepts one step at a time", () => {
    const s = run(
      { type: "choose", field: "name", id: 1 },
      { type: "merge" },
      { type: "acceptSure" },
    );
    const u1 = reduce(groups, s, { type: "undo" });
    expect(u1.decisions).toEqual({ 0: "merge" });
    const u2 = reduce(groups, u1, { type: "undo" });
    expect(u2.decisions).toEqual({});
    expect(u2.current).toBe(0);
    expect(u2.drafts[0]?.choices).toEqual({ name: 1 });
    const u3 = reduce(groups, u2, { type: "undo" });
    expect(u3.drafts).toEqual({});
    expect(reduce(groups, u3, { type: "undo" })).toBe(u3);
  });

  it("does not record moving around as something to undo", () => {
    const s = run({ type: "next" }, { type: "next" }, { type: "prev" });
    expect(s.current).toBe(1);
    expect(s.history).toEqual([]);
    expect(run({ type: "prev" }).current).toBe(0);
    expect(run({ type: "go", to: 99 }).current).toBe(3);
  });

  it("maps keys to actions", () => {
    expect(keyAction({ key: "j" })).toEqual({ type: "next" });
    expect(keyAction({ key: "Enter" })).toEqual({ type: "merge" });
    expect(keyAction({ key: "n" })).toEqual({ type: "apart" });
    expect(keyAction({ key: "z", metaKey: true })).toEqual({ type: "undo" });
    expect(keyAction({ key: "A" })).toEqual({ type: "acceptSure" });
    expect(keyAction({ key: "a" })).toBeNull();
    expect(keyAction({ key: "2" })).toBe("split-n");
    expect(keyAction({ key: "j", ctrlKey: true })).toBeNull();
  });
});
