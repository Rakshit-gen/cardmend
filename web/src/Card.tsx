import type { ReactNode } from "react";
import type { Contact, Field } from "./api";
import { addressText, birthdayText, fieldLabel, workLine } from "./fields";

function Rows({ title, fields }: { title: string; fields: Field<string>[] }) {
  if (!fields.length) return null;
  return (
    <>
      {fields.map((f, i) => (
        <div className="row" key={`${title}${i}`}>
          <dt>{i === 0 ? title : ""}</dt>
          <dd>
            <span className="data">{f.value}</span>
            {fieldLabel(f) && <span className="label"> {fieldLabel(f)}</span>}
          </dd>
        </div>
      ))}
    </>
  );
}

/** One contact as an index card: name over the red rule, then its fields. */
export default function Card({
  contact: c,
  heading,
  corner,
  removed = false,
  children,
  name,
}: {
  contact: Contact;
  heading?: ReactNode;
  /** Small text in the top corner: where the card came from. */
  corner?: ReactNode;
  removed?: boolean;
  /** Replaces the name line, for the merged card's choices. */
  name?: ReactNode;
  children?: ReactNode;
}) {
  const work = workLine(c);
  return (
    <article className={`card${removed ? " removed" : ""}`}>
      <header>
        {heading}
        {corner && <span className="corner">{corner}</span>}
      </header>
      <div className="name">
        {name ?? (
          <>
            {c.photo && (
              // Google exports link to photos online; hide them when offline.
              <img
                src={c.photo}
                alt=""
                width={40}
                height={40}
                onError={(e) => (e.currentTarget.hidden = true)}
              />
            )}
            <h3 className="data">{c.display || "(no name)"}</h3>
          </>
        )}
      </div>
      <dl>
        {work && !name && (
          <div className="row">
            <dt>work</dt>
            <dd className="data">{work}</dd>
          </div>
        )}
        {c.nicknames.length > 0 && (
          <div className="row">
            <dt>also</dt>
            <dd className="data">{c.nicknames.join(", ")}</dd>
          </div>
        )}
        <Rows title="phone" fields={c.phones} />
        <Rows title="email" fields={c.emails} />
        {c.addresses.map((a, i) => (
          <div className="row" key={`a${i}`}>
            <dt>{i === 0 ? "address" : ""}</dt>
            <dd>
              <span className="data">{addressText(a.value)}</span>
              {fieldLabel(a) && <span className="label"> {fieldLabel(a)}</span>}
            </dd>
          </div>
        ))}
        <Rows title="web" fields={c.urls} />
        {c.birthday && !name && (
          <div className="row">
            <dt>birthday</dt>
            <dd className="data">{birthdayText(c.birthday)}</dd>
          </div>
        )}
        {c.note && (
          <div className="row">
            <dt>note</dt>
            <dd className="data note">{c.note}</dd>
          </div>
        )}
        {c.categories.length > 0 && (
          <div className="row">
            <dt>groups</dt>
            <dd className="data">{c.categories.join(", ")}</dd>
          </div>
        )}
      </dl>
      {children}
    </article>
  );
}
