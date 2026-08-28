import { describe, expect, it } from "vitest";
import { resolveSort, sortItems, type SortOption } from "../sort";

interface Row {
  name: string;
  year: number | null;
  plays: number;
}

const byName: SortOption<Row> = { id: "name", label: "Name", value: (r) => r.name };
const byYear: SortOption<Row> = { id: "year", label: "Year", value: (r) => r.year };
const byPlays: SortOption<Row> = { id: "plays", label: "Plays", value: (r) => r.plays };

const rows: Row[] = [
  { name: "Bravo", year: 1998, plays: 3 },
  { name: "alpha", year: null, plays: 10 },
  { name: "Charlie", year: 1965, plays: 7 },
];

const names = (items: Row[]) => items.map((r) => r.name);

describe("sortItems", () => {
  it("orders text case-insensitively", () => {
    expect(names(sortItems(rows, byName, "asc"))).toEqual(["alpha", "Bravo", "Charlie"]);
    expect(names(sortItems(rows, byName, "desc"))).toEqual(["Charlie", "Bravo", "alpha"]);
  });

  it("orders numbers numerically rather than as text", () => {
    const many: Row[] = [
      { name: "a", year: 2000, plays: 9 },
      { name: "b", year: 2000, plays: 10 },
      { name: "c", year: 2000, plays: 100 },
    ];
    expect(names(sortItems(many, byPlays, "asc"))).toEqual(["a", "b", "c"]);
  });

  it("keeps untagged values last whichever way round the list is sorted", () => {
    // An untagged year is unknown, not "earlier than 1965", so reversing must
    // not float it to the top over records that actually have one.
    expect(names(sortItems(rows, byYear, "asc"))).toEqual(["Charlie", "Bravo", "alpha"]);
    expect(names(sortItems(rows, byYear, "desc"))).toEqual(["Bravo", "Charlie", "alpha"]);
  });

  it("does not modify the array it was given", () => {
    const original = [...rows];
    sortItems(rows, byName, "desc");
    expect(rows).toEqual(original);
  });
});

describe("resolveSort", () => {
  it("returns the requested sort when the tab offers it", () => {
    expect(resolveSort([byName, byYear], "year")).toBe(byYear);
  });

  it("falls back to the first sort when the tab does not offer it", () => {
    // Switching tabs keeps `?sort=` in the URL, so an id that means nothing
    // here has to degrade rather than blow up.
    expect(resolveSort([byName, byYear], "plays")).toBe(byName);
    expect(resolveSort([byName, byYear], undefined)).toBe(byName);
  });
});
