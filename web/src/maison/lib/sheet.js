// A sheet as dots, for a laser that takes only Brother's own language
// (Moli says the page in it): where each page goes on the sheet, the order
// the sheets print in, and greys turned into black dots. No drawing here:
// `paper.js` draws, this decides.

/** The sizes the printer knows, in points (1/72 inch), as Moli has them. */
export const PAPERS = {
  A4: [595, 842],
  A5: [420, 595],
  A6: [297, 420],
  B5: [499, 709],
  LETTER: [612, 792],
  LEGAL: [612, 1008],
  EXECUTIVE: [522, 756],
};

/** The kinds of paper it is told (its fuser heats for them). */
export const MEDIA = ['PLAIN', 'THIN', 'THICK', 'THICKER', 'BOND'];

/** Darker or lighter than the printer's own (-6 to +6 for it). */
export const DENSITY = { lighter: -3, normal: 0, darker: 3 };

export const DPI = 600;
/** The printer leaves about 4 mm round the sheet: pages fit inside 5. */
export const MARGIN = Math.round((5 / 25.4) * DPI);

/** A sheet's size in dots. */
export function sheetSize(paper) {
  const [w, h] = PAPERS[paper] ?? PAPERS.A4;
  return [Math.floor((w * DPI) / 72), Math.floor((h * DPI) / 72)];
}

/** Where pages go on a sheet of `width` × `height` dots: 1 inside the
 *  margins, 2 one above the other, 4 two by two. */
export function cells(width, height, perSheet) {
  const m = MARGIN;
  const w = width - 2 * m;
  const h = height - 2 * m;
  if (perSheet === 2) {
    const half = (h - m) / 2;
    return [
      { x: m, y: m, w, h: half },
      { x: m, y: m + half + m, w, h: half },
    ];
  }
  if (perSheet === 4) {
    const cw = (w - m) / 2;
    const ch = (h - m) / 2;
    return [0, 1, 2, 3].map((i) => ({ x: m + (i % 2) * (cw + m), y: m + Math.floor(i / 2) * (ch + m), w: cw, h: ch }));
  }
  return [{ x: m, y: m, w, h }];
}

/** Where a page of `size` ([width, height] in points, or pixels for a
 *  photo) goes in its cell: as big as fits, turned a quarter (clockwise)
 *  when that fits bigger; or at its real size (`actual`), centred on the
 *  sheet. `scale`: dots a point; `turn`: 0 or 90. */
export function place([pw, ph], cell, { actual = false, sheet = null } = {}) {
  const straight = Math.min(cell.w / pw, cell.h / ph);
  const turned = Math.min(cell.w / ph, cell.h / pw);
  const turn = turned > straight * 1.001 ? 90 : 0;
  const scale = actual ? DPI / 72 : Math.max(straight, turned);
  const [w, h] = turn ? [ph * scale, pw * scale] : [pw * scale, ph * scale];
  const [cx, cy, cw, ch] = actual && sheet ? [0, 0, sheet[0], sheet[1]] : [cell.x, cell.y, cell.w, cell.h];
  return { x: cx + (cw - w) / 2, y: cy + (ch - h) / 2, w, h, scale, turn };
}

/** The pages (numbers) grouped sheet side by sheet side. */
export function sides(pages, perSheet) {
  const out = [];
  for (let i = 0; i < pages.length; i += perSheet) out.push(pages.slice(i, i + perSheet));
  return out;
}

/** The order the sides print in, in one or two passes (indexes into the
 *  sides; `null`: a blank side). Several copies come out sorted.
 *
 *  Both sides by hand: the backs first (sides 2, 4…, and a blank one when
 *  the count is odd, so the second pass finds a sheet for the last front),
 *  then the person turns the pile over and puts it back; the fronts then
 *  print in the same order on the sheets as they come back out. */
export function passes(count, { twoSided = false, copies = 1 } = {}) {
  const repeat = (list) => Array.from({ length: copies }, () => list).flat();
  const all = Array.from({ length: count }, (_, i) => i);
  if (!twoSided || count < 2) return [repeat(all)];
  const backs = all.filter((i) => i % 2 === 1);
  if (count % 2) backs.push(null);
  return [repeat(backs), repeat(all.filter((i) => i % 2 === 0))];
}

/** Greys into black dots, row after row (Floyd–Steinberg: the error of each
 *  dot spread on its neighbours, carried from one band to the next). */
export class Dither {
  constructor(width) {
    this.width = width;
    this.here = new Float32Array(width + 2);
    this.below = new Float32Array(width + 2);
  }

  /** One row of RGBA (from `at` in `rgba`) into bits (from `out` in `bits`,
   *  1 = black, the leftmost dot in the high bit). */
  row(rgba, at, bits, out) {
    const { width, here, below } = this;
    below.fill(0);
    for (let x = 0; x < width; x++) {
      const i = at + 4 * x;
      const grey = (rgba[i] * 299 + rgba[i + 1] * 587 + rgba[i + 2] * 114) / 1000 + here[x + 1];
      const black = grey < 128;
      const error = black ? grey : grey - 255;
      if (black) bits[out + (x >> 3)] |= 0x80 >> (x & 7);
      if (error !== 0) {
        here[x + 2] += (error * 7) / 16;
        below[x] += (error * 3) / 16;
        below[x + 1] += (error * 5) / 16;
        below[x + 2] += error / 16;
      }
    }
    this.here = below;
    this.below = here;
  }
}

/** The header of a PBM: what Moli reads the dots from. */
export const pbmHeader = (width, height) => `P4\n${width} ${height}\n`;
