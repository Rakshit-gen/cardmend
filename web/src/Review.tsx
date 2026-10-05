import { useEffect, useRef, useState } from "react";
import { type Preview, type Ready, type SingleField, preview } from "./api";
import Card from "./Card";
import { birthdayText } from "./fields";
import { type Action, type ReviewState, counts, draftOf, keyAction, membersOf } from "./decisions";

const TIER_TEXT = {
  sure: "Sure: the strongest matches, usually the same name plus a shared number or email.",
  likely: "Likely: good evidence but weaker, such as an initial or a typo in the name.",
  check: "Check: weak evidence, such as only a first name, or a number with no name. Could be two people.",
};

const FIELDS: { field: SingleField; title: string }[] = [
  { field: "name", title: "name" },
  { field: "org", title: "company" },
  { field: "title", title: "job title" },
  { field: "birthday", title: "birthday" },
  { field: "photo", title: "photo" },
];

function Choices({
  p,
  number,
  onChoose,
}: {
  p: Preview;
  number: (id: number) => number;
  onChoose: (field: SingleField, id: number) => void;
}) {
  const from = (ids: number[]) => `card ${ids.map(number).join(", ")}`;
  return (
    <div className="choices">
      {FIELDS.map(({ field, title }) => {
        const alts = p.alternatives[field];
        const picked = p.choices[field];
        if (!alts.length) return null;
        const show = (v: string) => (field === "birthday" ? birthdayText(v) : v);
        if (alts.length === 1)
          return (
            <div className="row" key={field}>
              <dt>{title}</dt>
              <dd className="data">{show(alts[0]!.value)}</dd>
            </div>
          );
        return (
          <fieldset key={field}>
            <legend>{title}: pick one</legend>
            {alts.map((a) => {
              const id = a.from[0]!;
              const on = picked !== undefined && a.from.includes(picked);
              return (
                <label key={id} className={on ? "on" : ""}>
                  <input
                    type="radio"
                    name={field}
                    checked={on}
                    onChange={() => onChoose(field, id)}
                  />
                  <span className="data">{show(a.value)}</span>
                  <span className="label"> {from(a.from)}</span>
                </label>
              );
            })}
          </fieldset>
        );
      })}
    </div>
  );
}

export default function Review({
  data,
  state,
  dispatch,
  onDone,
}: {
  data: Ready;
  state: ReviewState;
  dispatch: (a: Action) => void;
  onDone: () => void;
}) {
  const groups = data.groups;
  const g = state.current;
  const group = groups[g]!;
  const draft = draftOf(state, g);
  const members = membersOf(group, draft);
  const decided = state.decisions[g];
  const c = counts(groups, state);
  const [merged, setMerged] = useState<Preview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const lastG = useRef(g);
  const direction = g >= lastG.current ? "from-right" : "from-left";
  useEffect(() => {
    lastG.current = g;
  }, [g]);

  const key = JSON.stringify([members, draft.choices]);
  useEffect(() => {
    let live = true;
    setError(null);
    if (members.length < 2) {
      setMerged(null);
      return;
    }
    preview(members, draft.choices).then(
      (p) => live && setMerged(p),
      (e: Error) => live && setError(e.message),
    );
    return () => {
      live = false;
    };
    // `key` stands in for members and choices, which are new arrays on
    // every render.
  }, [key]);

  const number = (id: number) => group.members.indexOf(id) + 1;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select") && e.key !== "Enter") return;
      if (t.closest("button") && (e.key === "Enter" || e.key === " ")) return;
      const a = keyAction(e);
      if (!a) return;
      e.preventDefault();
      if (a === "split-n") {
        const id = group.members[Number(e.key) - 1];
        if (id !== undefined) dispatch({ type: "split", id });
      } else dispatch(a);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [group, dispatch]);

  const cards = group.members.map((id) => {
    const contact = data.contacts[id]!;
    const out = draft.removed.includes(id);
    return (
      <Card
        key={id}
        contact={contact}
        removed={out}
        heading={<span className="num">{number(id)}</span>}
        corner={`${contact.source.file}, line ${contact.source.line}`}
      >
        <button
          type="button"
          className="quiet"
          onClick={() => dispatch({ type: "split", id })}
          aria-keyshortcuts={String(number(id))}
        >
          {out ? "Put back in this group" : "Split out, keep separate"}
        </button>
      </Card>
    );
  });
  const half = Math.min(1, cards.length);
  const allDone = c.open === 0;

  return (
    <section className="review" aria-label="Review duplicates">
      <nav className="groupnav">
        <button type="button" onClick={() => dispatch({ type: "prev" })} disabled={g === 0}>
          Previous
        </button>
        <span className="where">
          <span className={`tab tier-${group.tier}`}>{group.tier}</span> Group {g + 1} of{" "}
          {groups.length}
        </span>
        <button
          type="button"
          onClick={() => dispatch({ type: "next" })}
          disabled={g === groups.length - 1}
        >
          Next
        </button>
        <span className="progress">
          {c.merge} to merge, {c.apart} kept apart, {c.open} left
        </span>
      </nav>

      {allDone && (
        <div className="done" role="status">
          <p>
            All {groups.length} groups reviewed. Download the clean file, or go back through
            them; you can still change any decision.
          </p>
          <button type="button" className="primary" onClick={onDone}>
            Download clean file
          </button>
        </div>
      )}

      <p className="tiernote">{TIER_TEXT[group.tier]}</p>
      {decided && (
        <p className="decided" role="status">
          {decided === "merge"
            ? `Decided: merge ${members.length} cards into one.`
            : "Decided: keep these as separate contacts."}
        </p>
      )}

      <div className={`table ${direction}`} key={g}>
        <div className="side">{cards.slice(0, half)}</div>
        <div className="middle">
          {members.length < 2 ? (
            <div className="card blank">
              <p>
                Only one card is left in this group, so there is nothing to merge. Put a card
                back, or mark the group as not duplicates.
              </p>
            </div>
          ) : error ? (
            <div className="card blank">
              <p className="error">{error}</p>
            </div>
          ) : merged ? (
            <Card
              contact={merged.contact}
              heading={<span className="merged-title">Merged card</span>}
              name={
                <Choices
                  p={merged}
                  number={number}
                  onChoose={(field, id) => dispatch({ type: "choose", field, id })}
                />
              }
            />
          ) : (
            <div className="card blank">
              <p>Merging</p>
            </div>
          )}
        </div>
        <div className="side">{cards.slice(half)}</div>
      </div>

      <section className="why" aria-label="Why these matched">
        <h2>Why they matched</h2>
        {group.pairs.map((p) => (
          <div key={`${p.a}-${p.b}`}>
            <h3>
              Cards {number(p.a)} and {number(p.b)}
            </h3>
            <ul>
              {p.why.map((w) => (
                <li key={w.text} className={w.weight < 0 ? "against" : ""}>
                  {w.weight < 0 && <span className="sr">Against: </span>}
                  {w.text}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </section>

      <div className="decide">
        <button
          type="button"
          className="primary"
          onClick={() => dispatch({ type: "merge" })}
          disabled={members.length < 2}
          aria-keyshortcuts="Enter"
        >
          Merge {members.length >= 2 ? `${members.length} cards` : ""} <kbd>Enter</kbd>
        </button>
        <button type="button" onClick={() => dispatch({ type: "apart" })} aria-keyshortcuts="n">
          Not duplicates <kbd>n</kbd>
        </button>
        <button
          type="button"
          onClick={() => dispatch({ type: "undo" })}
          disabled={!state.history.length}
          aria-keyshortcuts="u"
        >
          Undo <kbd>u</kbd>
        </button>
        <button
          type="button"
          className="quiet"
          onClick={() => dispatch({ type: "acceptSure" })}
          aria-keyshortcuts="Shift+A"
        >
          Merge every sure group <kbd>A</kbd>
        </button>
        <p className="keys">
          <kbd>j</kbd> <kbd>k</kbd> next and previous, <kbd>1</kbd> to <kbd>9</kbd> split a card
          out
        </p>
      </div>
    </section>
  );
}
