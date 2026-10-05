import { useCallback, useEffect, useReducer, useState } from "react";
import { type Analysis, type Ready, addFile, clearFiles, exportFile, getAnalysis } from "./api";
import DropZone from "./DropZone";
import Overview from "./Overview";
import Review from "./Review";
import { type Action, type ReviewState, counts, initial, load, merges, reduce, save } from "./decisions";

type View = "overview" | "review";

// The view lives in the URL hash so a reload lands on the same screen.
const viewFromHash = (): View => (location.hash === "#review" ? "review" : "overview");

type Store = { fp: string; groups: Ready["groups"]; s: ReviewState };
type StoreAction = Action | { type: "load"; data: Ready };

function storeReducer(st: Store, a: StoreAction): Store {
  if (a.type === "load") {
    if (a.data.fingerprint === st.fp) return { ...st, groups: a.data.groups };
    return { fp: a.data.fingerprint, groups: a.data.groups, s: load(a.data.fingerprint, a.data.groups.length) };
  }
  return { ...st, s: reduce(st.groups, st.s, a) };
}

export default function App() {
  const [data, setData] = useState<Analysis | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const [view, setView] = useState<View>(viewFromHash);
  const [store, dispatch] = useReducer(storeReducer, { fp: "", groups: [], s: initial });
  const [exported, setExported] = useState<string | null>(null);

  const show = useCallback((a: Analysis) => {
    setData(a);
    if (a.state === "ready") dispatch({ type: "load", data: a });
  }, []);

  useEffect(() => {
    getAnalysis().then(show, (e: Error) => setErrors([e.message]));
    const onHash = () => setView(viewFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, [show]);

  useEffect(() => {
    if (store.fp) save(store.fp, store.s);
  }, [store]);

  const go = (v: View) => {
    location.hash = v === "review" ? "review" : "";
    setView(v);
    window.scrollTo(0, 0);
  };

  async function onFiles(files: File[]) {
    const errs: string[] = [];
    for (const [i, f] of files.entries()) {
      setBusy(
        files.length > 1 ? `Reading ${f.name} (${i + 1} of ${files.length})` : `Reading ${f.name}`,
      );
      try {
        show(await addFile(f));
      } catch (e) {
        errs.push((e as Error).message);
      }
    }
    setBusy(null);
    setErrors(errs);
    setExported(null);
  }

  async function startOver() {
    if (!confirm("Forget the files you dropped and your review decisions on this page?")) return;
    try {
      const fp = store.fp;
      show(await clearFiles());
      try {
        localStorage.removeItem(`cardmend:review:${fp}`);
      } catch {
        // Nothing saved, or storage blocked.
      }
      setErrors([]);
      setExported(null);
      go("overview");
    } catch (e) {
      setErrors([(e as Error).message]);
    }
  }

  async function download() {
    if (data?.state !== "ready") return;
    try {
      const { blob, summary } = await exportFile(merges(data.groups, store.s));
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "contacts-clean.vcf";
      a.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
      const dropped = summary.empty_dropped
        ? `, ${summary.empty_dropped} empty ${summary.empty_dropped === 1 ? "entry" : "entries"} left out`
        : "";
      setExported(
        `Saved contacts-clean.vcf: ${summary.contacts_in} contacts in, ${summary.contacts_out} out, ${summary.groups_merged} ${summary.groups_merged === 1 ? "group" : "groups"} merged${dropped}. Your original files are unchanged.`,
      );
    } catch (e) {
      setErrors([(e as Error).message]);
    }
  }

  const ready = data?.state === "ready" ? data : null;
  const c = ready ? counts(ready.groups, store.s) : null;
  const reviewing = ready && view === "review" && ready.groups.length > 0;

  return (
    <>
      <header className="top">
        <a className="brand" href="#" onClick={() => go("overview")}>
          <img src="/logo.svg" alt="" width={28} height={28} />
          cardmend
        </a>
        {ready && (
          <nav className="views" aria-label="Screens">
            <button
              type="button"
              aria-current={view === "overview" ? "page" : undefined}
              onClick={() => go("overview")}
            >
              Overview
            </button>
            {ready.groups.length > 0 && (
              <button
                type="button"
                aria-current={view === "review" ? "page" : undefined}
                onClick={() => go("review")}
              >
                Review
              </button>
            )}
          </nav>
        )}
        {ready && (
          <div className="export">
            <button type="button" className="primary" onClick={download}>
              Download clean file
            </button>
            {c && ready.groups.length > 0 && (
              <span className="muted">
                Merges {c.merge} {c.merge === 1 ? "group" : "groups"}
                {c.open > 0 && `; ${c.open} not reviewed stay as they are`}
              </span>
            )}
          </div>
        )}
      </header>

      <main>
        {errors.length > 0 && (
          <div className="errors" role="alert">
            {errors.map((e, i) => (
              <p key={i}>{e}</p>
            ))}
          </div>
        )}
        {exported && (
          <p className="saved" role="status">
            {exported}
          </p>
        )}

        {data === null && !errors.length && <p className="muted">Loading</p>}

        {data?.state === "empty" && (
          <div className="start">
            <h1>Clean up duplicate contacts</h1>
            <p className="lede">
              cardmend finds contacts that are the same person, shows you why it thinks so, and
              lets you merge them field by field into one clean vCard file. Your files stay on
              this computer and are never changed.
            </p>
            <DropZone onFiles={onFiles} busy={busy} />
            <p className="muted">
              Numbers without a country code are read as {data.region} numbers. Start
              cardmend-web with <code>--region</code> to change that.
            </p>
          </div>
        )}

        {ready &&
          (reviewing ? (
            <Review data={ready} state={store.s} dispatch={dispatch} onDone={download} />
          ) : (
            <>
              <Overview
                data={ready}
                state={store.s}
                onReview={() => go("review")}
                onAcceptSure={() => dispatch({ type: "acceptSure" })}
              />
              <section className="more">
                <h2>Files</h2>
                <DropZone onFiles={onFiles} busy={busy} compact />
                <button type="button" className="quiet" onClick={startOver}>
                  Start over with other files
                </button>
              </section>
            </>
          ))}
      </main>
    </>
  );
}
