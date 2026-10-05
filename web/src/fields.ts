// Turning contact fields into the short text shown on a card.

import type { Address, Contact, Field } from "./api";

const TYPE_WORDS: Record<string, string> = {
  cell: "mobile",
  mobile: "mobile",
  iphone: "iPhone",
  home: "home",
  work: "work",
  main: "main",
  fax: "fax",
  pager: "pager",
  other: "other",
};

/** "mobile", "work fax", or the custom label someone typed. */
export function fieldLabel(f: Field<unknown>): string {
  if (f.label) return f.label;
  const words = f.types.map((t) => TYPE_WORDS[t]).filter((w): w is string => !!w);
  // vCard 2.1 marks a fax as WORK;FAX; "work fax" reads better than "fax work".
  return [...new Set(words)].sort((a, b) => (a === "fax" ? 1 : b === "fax" ? -1 : 0)).join(" ");
}

export function addressText(a: Address): string {
  const cityLine = [a.locality, a.region, a.postal_code].filter(Boolean).join(" ");
  return [a.po_box, a.extended, a.street, cityLine, a.country]
    .map((s) => s.trim())
    .filter(Boolean)
    .join(", ");
}

/** "Sales, Acme Ltd" style line for the job and company. */
export function workLine(c: Contact): string {
  const org = [c.org, c.department].filter(Boolean).join(", ");
  return [c.title, org].filter(Boolean).join(", ");
}

/** "1984-03-07" stays; "--03-07" (no year) reads as "7 March". */
export function birthdayText(b: string): string {
  const m = /^--(\d\d)-(\d\d)$/.exec(b);
  if (!m) return b;
  const month = new Date(2000, Number(m[1]) - 1, 1).toLocaleString("en-GB", { month: "long" });
  return `${Number(m[2])} ${month}, no year`;
}
