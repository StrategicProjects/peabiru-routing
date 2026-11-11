// Reading the points of the sample, in Node and in browsers alike.

// ANCHOR: parsePoints
/**
 * The rows of a CSV file of points (a header with id, lon, lat and any
 * other columns; no quoted fields) as columns: `id` as strings, the
 * others as numbers.
 */
export function parsePoints(text) {
  const [header, ...lines] = text.trim().split(/\r?\n/);
  const names = header.split(",");
  const columns = Object.fromEntries(names.map((name) => [name, []]));
  for (const line of lines) {
    line.split(",").forEach((value, k) => {
      columns[names[k]].push(names[k] === "id" ? value : Number(value));
    });
  }
  return columns;
}
// ANCHOR_END: parsePoints
