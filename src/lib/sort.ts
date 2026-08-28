/**
 * Sorting for the library tabs.
 *
 * A sort describes the value to order by rather than supplying a comparator,
 * so direction and the treatment of missing values behave the same on every
 * tab and cannot drift apart as more sorts are added.
 */

export type SortDirection = "asc" | "desc";

export interface SortOption<T> {
  id: string;
  label: string;
  /** `null` or `""` means "not tagged", which sorts differently from zero. */
  value: (item: T) => string | number | null;
}

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/**
 * Order a copy of `items`.
 *
 * Untagged values collect at the end whichever way round the list is sorted: a
 * missing year is not "earlier" than 1965, it is simply unknown, and burying
 * the tagged records under it when reversing would be no help to anyone.
 */
export function sortItems<T>(
  items: readonly T[],
  option: SortOption<T>,
  direction: SortDirection,
): T[] {
  const sign = direction === "asc" ? 1 : -1;

  return [...items].sort((a, b) => {
    const left = option.value(a);
    const right = option.value(b);
    const leftMissing = left === null || left === "";
    const rightMissing = right === null || right === "";

    if (leftMissing || rightMissing) {
      if (leftMissing && rightMissing) return 0;
      return leftMissing ? 1 : -1;
    }
    if (typeof left === "number" && typeof right === "number") {
      return sign * (left - right);
    }
    return sign * collator.compare(String(left), String(right));
  });
}

/** The requested sort, or the first one when the tab does not offer it. */
export function resolveSort<T>(
  options: ReadonlyArray<SortOption<T>>,
  requested: unknown,
): SortOption<T> {
  return options.find((option) => option.id === requested) ?? options[0];
}
