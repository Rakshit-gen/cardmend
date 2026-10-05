// What the user has decided so far, kept apart from React so it can be
// tested and saved as plain JSON.

import type { Choices, Group, SingleField } from "./api";

export type Decision = "merge" | "apart";

export interface Draft {
  /** Contacts split out of the group: they stay as their own entries. */
  removed: number[];
  choices: Choices;
}

export interface Snapshot {
  current: number;
  drafts: Record<number, Draft>;
  decisions: Record<number, Decision>;
}

export interface ReviewState extends Snapshot {
  /** Earlier snapshots, newest last, for undo. */
  history: Snapshot[];
}

export type Action =
  | { type: "go"; to: number }
  | { type: "next" }
  | { type: "prev" }
  | { type: "choose"; field: SingleField; id: number }
  | { type: "split"; id: number }
  | { type: "merge" }
  | { type: "apart" }
  | { type: "acceptSure" }
  | { type: "undo" };

export const initial: ReviewState = { current: 0, drafts: {}, decisions: {}, history: [] };

const HISTORY = 200;

export const draftOf = (s: Snapshot, g: number): Draft =>
  s.drafts[g] ?? { removed: [], choices: {} };

/** Members still in the group after any split-outs. */
export const membersOf = (group: Group, d: Draft): number[] =>
  group.members.filter((m) => !d.removed.includes(m));

/** The first undecided group after `from`, wrapping round; -1 if none. */
export function nextOpen(groups: Group[], s: Snapshot, from: number): number {
  for (let i = 1; i <= groups.length; i++) {
    const g = (from + i) % groups.length;
    if (!(g in s.decisions)) return g;
  }
  return -1;
}

function snap(s: ReviewState): Snapshot {
  return { current: s.current, drafts: s.drafts, decisions: s.decisions };
}

/** Apply a change and remember the state before it, for undo. */
function change(s: ReviewState, next: Snapshot): ReviewState {
  return { ...next, history: [...s.history, snap(s)].slice(-HISTORY) };
}

export function reduce(groups: Group[], s: ReviewState, a: Action): ReviewState {
  const last = groups.length - 1;
  const g = s.current;
  const group = groups[g];
  switch (a.type) {
    case "go":
      return { ...s, current: Math.max(0, Math.min(last, a.to)) };
    case "next":
      return { ...s, current: Math.min(last, g + 1) };
    case "prev":
      return { ...s, current: Math.max(0, g - 1) };
    case "undo": {
      const prev = s.history[s.history.length - 1];
      return prev ? { ...prev, history: s.history.slice(0, -1) } : s;
    }
    case "acceptSure": {
      const decisions = { ...s.decisions };
      let n = 0;
      groups.forEach((grp, i) => {
        if (grp.tier === "sure" && !(i in decisions)) {
          decisions[i] = "merge";
          n++;
        }
      });
      if (n === 0) return s;
      const next = { ...snap(s), decisions };
      const open = nextOpen(groups, next, g - 1);
      return change(s, { ...next, current: open === -1 ? g : open });
    }
  }
  if (!group) return s;
  const d = draftOf(s, g);
  switch (a.type) {
    case "choose":
      return change(s, {
        ...snap(s),
        drafts: { ...s.drafts, [g]: { ...d, choices: { ...d.choices, [a.field]: a.id } } },
      });
    case "split": {
      if (!group.members.includes(a.id)) return s;
      const removed = d.removed.includes(a.id)
        ? d.removed.filter((x) => x !== a.id)
        : [...d.removed, a.id];
      // A pick that came from a contact now split out no longer applies.
      const choices: Choices = {};
      for (const [k, v] of Object.entries(d.choices)) {
        if (v !== undefined && !removed.includes(v)) choices[k as SingleField] = v;
      }
      return change(s, { ...snap(s), drafts: { ...s.drafts, [g]: { removed, choices } } });
    }
    case "merge":
    case "apart": {
      // Fewer than two left after split-outs: nothing to merge.
      const decision: Decision =
        a.type === "merge" && membersOf(group, d).length >= 2 ? "merge" : "apart";
      const next = { ...snap(s), decisions: { ...s.decisions, [g]: decision } };
      const open = nextOpen(groups, next, g);
      return change(s, { ...next, current: open === -1 ? g : open });
    }
  }
}

/** The merges to send for export: every group decided as "merge". */
export function merges(groups: Group[], s: Snapshot) {
  return groups.flatMap((group, g) => {
    if (s.decisions[g] !== "merge") return [];
    const d = draftOf(s, g);
    return [{ members: membersOf(group, d), choices: d.choices }];
  });
}

export function counts(groups: Group[], s: Snapshot) {
  let merge = 0;
  let apart = 0;
  for (let g = 0; g < groups.length; g++) {
    if (s.decisions[g] === "merge") merge++;
    else if (s.decisions[g] === "apart") apart++;
  }
  return { merge, apart, open: groups.length - merge - apart };
}

/** Keyboard shortcuts on the review screen. */
export function keyAction(e: {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}): Action | "split-n" | null {
  if (e.altKey) return null;
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") return { type: "undo" };
  if (e.ctrlKey || e.metaKey) return null;
  switch (e.key) {
    case "j":
    case "ArrowRight":
      return { type: "next" };
    case "k":
    case "ArrowLeft":
      return { type: "prev" };
    case "Enter":
    case "m":
      return { type: "merge" };
    case "n":
      return { type: "apart" };
    case "u":
      return { type: "undo" };
    case "A":
      return { type: "acceptSure" };
  }
  return /^[1-9]$/.test(e.key) ? "split-n" : null;
}

// Saved per set of files, so dropping different files starts fresh and a
// reload carries on where you were.
const storeKey = (fingerprint: string) => `cardmend:review:${fingerprint}`;

export function load(fingerprint: string, groups: number): ReviewState {
  try {
    const raw = localStorage.getItem(storeKey(fingerprint));
    if (!raw) return initial;
    const s = JSON.parse(raw) as ReviewState;
    return { ...initial, ...s, current: Math.min(s.current ?? 0, Math.max(0, groups - 1)) };
  } catch {
    return initial;
  }
}

export function save(fingerprint: string, s: ReviewState) {
  try {
    localStorage.setItem(storeKey(fingerprint), JSON.stringify(s));
  } catch {
    // Storage full or blocked: the review still works, it just won't
    // survive a reload.
  }
}
