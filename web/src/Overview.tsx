import type { Ready, Tier } from "./api";
import type { ReviewState } from "./decisions";
import { counts } from "./decisions";

const TIERS: { tier: Tier; what: string }[] = [
  { tier: "sure", what: "The strongest matches, usually the same name with a shared number or email." },
  { tier: "likely", what: "Good evidence but weaker, such as an initial or a typo in the name." },
  { tier: "check", what: "Weak evidence, such as only a first name. Often two different people." },
];

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export default function Overview({
  data,
  state,
  onReview,
  onAcceptSure,
}: {
  data: Ready;
  state: ReviewState;
  onReview: () => void;
  onAcceptSure: () => void;
}) {
  const { problems, groups } = data;
  const name = (id: number) => data.contacts[id]?.display || "(no name)";
  const where = (id: number) => {
    const s = data.contacts[id]?.source;
    return s ? `${s.file}, line ${s.line}` : "";
  };
  const c = counts(groups, state);
  const openSure = groups.filter((g, i) => g.tier === "sure" && !(i in state.decisions)).length;
  const inGroups = groups.reduce((n, g) => n + g.members.length, 0);

  return (
    <div className="overview">
      <section>
        <h2>What was read</h2>
        <table>
          <tbody>
            {data.files.map(([f, n]) => (
              <tr key={f}>
                <td className="data">{f}</td>
                <td className="num-cell">{plural(n, "contact", "contacts")}</td>
              </tr>
            ))}
            {data.files.length > 1 && (
              <tr className="total">
                <td>Together</td>
                <td className="num-cell">{plural(data.total, "contact", "contacts")}</td>
              </tr>
            )}
          </tbody>
        </table>
        {data.issues.length > 0 && (
          <div className="issues">
            <h3>Parts that couldn't be read</h3>
            <p>
              Everything else in these files was read. The lines below were skipped and won't
              be in the clean file, so check them in the original.
            </p>
            <ul>
              {data.issues.map((i, n) => (
                <li key={n}>
                  <span className="data">
                    {i.file}
                    {i.line > 0 && `, line ${i.line}`}
                  </span>
                  : {i.message}
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>

      <section>
        <h2>Duplicates</h2>
        {groups.length === 0 ? (
          <p>
            No duplicates found among {plural(data.total, "contact", "contacts")}. You can still
            download a clean copy, which leaves out empty entries and writes everything as vCard
            3.0.
          </p>
        ) : (
          <>
            <p>
              {plural(groups.length, "group", "groups")} of contacts look like the same person,{" "}
              {inGroups} entries in all.
              {c.open < groups.length &&
                ` You've decided ${groups.length - c.open}: ${c.merge} to merge, ${c.apart} kept apart.`}
            </p>
            <table className="tiers">
              <tbody>
                {TIERS.map(({ tier, what }) => (
                  <tr key={tier}>
                    <th scope="row">
                      <span className={`tab tier-${tier}`}>{tier}</span>
                    </th>
                    <td className="num-cell">{groups.filter((g) => g.tier === tier).length}</td>
                    <td>{what}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="actions">
              <button type="button" className="primary" onClick={onReview}>
                {c.open === groups.length ? "Review the groups" : "Carry on reviewing"}
              </button>
              {openSure > 0 && (
                <button type="button" onClick={onAcceptSure}>
                  Merge all {plural(openSure, "sure group", "sure groups")} without looking
                </button>
              )}
            </div>
          </>
        )}
      </section>

      {(problems.no_name.length > 0 ||
        problems.no_country.length > 0 ||
        problems.empty.length > 0 ||
        data.shared.length > 0) && (
        <section>
          <h2>Other things to fix</h2>
          <p className="muted">
            cardmend doesn't change these; fix them in your phone or mail app once the clean file
            is imported.
          </p>
          {problems.no_country.length > 0 && (
            <details>
              <summary>
                {plural(problems.no_country.length, "number", "numbers")} saved without a country
                code
              </summary>
              <p className="muted">
                They were read as {data.region} numbers. They stop working when you travel, and
                messaging apps may not find them.
              </p>
              <ul>
                {problems.no_country.slice(0, 200).map(([id, num], i) => (
                  <li key={i}>
                    <span className="data">{num}</span> on {name(id)}
                  </li>
                ))}
              </ul>
            </details>
          )}
          {problems.no_name.length > 0 && (
            <details>
              <summary>
                {plural(problems.no_name.length, "contact has", "contacts have")} no name
              </summary>
              <ul>
                {problems.no_name.slice(0, 200).map((id) => {
                  const ct = data.contacts[id];
                  const what = ct?.phones[0]?.value ?? ct?.emails[0]?.value ?? "";
                  return (
                    <li key={id}>
                      <span className="data">{what}</span> in {where(id)}
                    </li>
                  );
                })}
              </ul>
            </details>
          )}
          {problems.empty.length > 0 && (
            <details>
              <summary>
                {plural(problems.empty.length, "entry is", "entries are")} empty and will be left
                out
              </summary>
              <ul>
                {problems.empty.slice(0, 200).map((id) => (
                  <li key={id} className="data">
                    {where(id)}
                  </li>
                ))}
              </ul>
            </details>
          )}
          {data.shared.length > 0 && (
            <details>
              <summary>
                {plural(data.shared.length, "number or email is", "numbers and emails are")}{" "}
                shared by different people
              </summary>
              <p className="muted">
                A family landline, an office switchboard, a shared inbox. They aren't used as
                evidence that two contacts are the same person.
              </p>
              <ul>
                {data.shared.slice(0, 200).map((s) => (
                  <li key={s.key}>
                    <span className="data">{s.display}</span>:{" "}
                    {s.contacts.slice(0, 5).map(name).join(", ")}
                    {s.contacts.length > 5 && ` and ${s.contacts.length - 5} more`}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </section>
      )}
    </div>
  );
}
