import { describe, expect, it } from "vitest";
import { addressText, birthdayText, fieldLabel } from "./fields";

describe("fields", () => {
  it("labels numbers the way people say them", () => {
    expect(fieldLabel({ value: "", types: ["cell", "voice", "pref"], label: null })).toBe("mobile");
    expect(fieldLabel({ value: "", types: ["fax", "work"], label: null })).toBe("work fax");
    expect(fieldLabel({ value: "", types: ["home"], label: "Grandma's" })).toBe("Grandma's");
    expect(fieldLabel({ value: "", types: [], label: null })).toBe("");
  });

  it("writes an address on one line, skipping empty parts", () => {
    expect(
      addressText({
        po_box: "",
        extended: "Flat 4",
        street: "12 MG Road",
        locality: "Pune",
        region: "MH",
        postal_code: "411001",
        country: "India",
      }),
    ).toBe("Flat 4, 12 MG Road, Pune MH 411001, India");
  });

  it("says when a birthday has no year", () => {
    expect(birthdayText("1984-03-07")).toBe("1984-03-07");
    expect(birthdayText("--03-07")).toBe("7 March, no year");
  });
});
