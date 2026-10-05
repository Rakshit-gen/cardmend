// Calls to cardmend-web. Shapes mirror src/bin/web.rs, src/contact.rs and
// src/merge.rs.

export type Tier = "sure" | "likely" | "check";

export interface Field<T = string> {
  value: T;
  types: string[];
  label: string | null;
}

export interface Address {
  po_box: string;
  extended: string;
  street: string;
  locality: string;
  region: string;
  postal_code: string;
  country: string;
}

export interface Contact {
  id: number;
  display: string;
  source: { file: string; index: number; line: number };
  formatted_name: string;
  name: { family: string; given: string; additional: string; prefix: string; suffix: string };
  nicknames: string[];
  org: string;
  department: string;
  title: string;
  phones: Field[];
  emails: Field[];
  addresses: Field<Address>[];
  urls: Field[];
  birthday?: string;
  note: string;
  photo?: string;
  categories: string[];
}

export interface Evidence {
  text: string;
  weight: number;
}

export interface Group {
  tier: Tier;
  score: number;
  members: number[];
  pairs: { a: number; b: number; score: number; why: Evidence[] }[];
}

export interface Issue {
  file: string;
  line: number;
  message: string;
}

export interface Ready {
  state: "ready";
  fingerprint: string;
  region: string;
  files: [string, number][];
  issues: Issue[];
  total: number;
  contacts: Record<string, Contact>;
  groups: Group[];
  shared: { key: string; display: string; contacts: number[] }[];
  problems: { no_name: number[]; no_country: [number, string][]; empty: number[] };
}

export type Analysis = { state: "empty"; region: string } | Ready;

export type SingleField = "name" | "birthday" | "org" | "title" | "photo";
export type Choices = Partial<Record<SingleField, number>>;

export interface Alternative {
  from: number[];
  value: string;
}

export interface Preview {
  contact: Contact;
  choices: Choices;
  alternatives: Record<SingleField, Alternative[]>;
}

export interface Summary {
  contacts_in: number;
  contacts_out: number;
  groups_merged: number;
  empty_dropped: number;
}

async function json<T>(res: Response): Promise<T> {
  const body = await res.json().catch(() => null);
  if (!res.ok) throw new Error(body?.error ?? `cardmend answered ${res.status}.`);
  return body as T;
}

const call = <T>(url: string, init?: RequestInit) =>
  fetch(url, init).then(
    (r) => json<T>(r),
    () => {
      throw new Error("Couldn't reach cardmend. Is cardmend-web still running?");
    },
  );

const post = (body: unknown): RequestInit => ({
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(body),
});

export const getAnalysis = () => call<Analysis>("/api/analysis");

export const addFile = (file: File) =>
  call<Analysis>("/api/files", {
    method: "POST",
    headers: { "x-filename": encodeURIComponent(file.name) },
    body: file,
  });

export const clearFiles = () => call<Analysis>("/api/files", { method: "DELETE" });

export const preview = (members: number[], choices: Choices) =>
  call<Preview>("/api/preview", post({ members, choices }));

export async function exportFile(
  merges: { members: number[]; choices: Choices }[],
): Promise<{ blob: Blob; summary: Summary }> {
  let res: Response;
  try {
    res = await fetch("/api/export", post({ merges }));
  } catch {
    throw new Error("Couldn't reach cardmend. Is cardmend-web still running?");
  }
  if (!res.ok) await json(res);
  const summary = JSON.parse(res.headers.get("x-cardmend-summary") ?? "{}") as Summary;
  return { blob: await res.blob(), summary };
}
