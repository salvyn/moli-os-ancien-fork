import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sheetSize, cells, place, sides, passes, Dither, MARGIN, DPI } from './sheet.js';

test('a sheet is drawn at the size Moli checks it against', () => {
  // The same as the Rust side (`hbp::dots`): points × 600 / 72, rounded down.
  assert.deepEqual(sheetSize('A4'), [4958, 7016]);
  assert.deepEqual(sheetSize('A5'), [3500, 4958]);
  assert.deepEqual(sheetSize('LETTER'), [5100, 6600]);
  assert.deepEqual(sheetSize('nothing'), [4958, 7016]);
});

test('an A4 page fits inside the margins, a landscape one turned', () => {
  const [w, h] = sheetSize('A4');
  const [cell] = cells(w, h, 1);
  const page = place([595, 842], cell);
  assert.equal(page.turn, 0);
  assert.ok(page.x >= MARGIN - 0.5 && page.y >= MARGIN - 0.5);
  assert.ok(page.x + page.w <= w - MARGIN + 0.5 && page.y + page.h <= h - MARGIN + 0.5);
  const wide = place([842, 595], cell);
  assert.equal(wide.turn, 90);
  assert.ok(Math.abs(wide.w - page.w) < 1);
  // At its real size: 600 dots an inch, centred on the sheet.
  const real = place([595, 842], cell, { actual: true, sheet: [w, h] });
  assert.equal(real.scale, DPI / 72);
  assert.ok(Math.abs(real.x + real.w / 2 - w / 2) < 0.01);
});

test('two pages a sheet go one above the other, turned', () => {
  const [w, h] = sheetSize('A4');
  const two = cells(w, h, 2);
  assert.equal(two.length, 2);
  assert.ok(two[0].y + two[0].h < two[1].y);
  const first = place([595, 842], two[0]);
  // Turned clockwise: read with the sheet turned, page 1 is on the left.
  assert.equal(first.turn, 90);
  assert.ok(first.y + first.h <= two[1].y);
  const four = cells(w, h, 4);
  assert.deepEqual(
    four.map((c) => [c.x < w / 2, c.y < h / 2]),
    [
      [true, true],
      [false, true],
      [true, false],
      [false, false],
    ],
  );
  assert.equal(place([595, 842], four[0]).turn, 0);
});

test('pages go sheet side by sheet side', () => {
  assert.deepEqual(sides([1, 2, 3, 4, 5], 2), [[1, 2], [3, 4], [5]]);
  assert.deepEqual(sides([2, 7], 1), [[2], [7]]);
});

test('both sides by hand: the backs first, a blank one for an odd count', () => {
  assert.deepEqual(passes(3), [[0, 1, 2]]);
  assert.deepEqual(passes(2, { copies: 2 }), [[0, 1, 0, 1]]);
  assert.deepEqual(passes(5, { twoSided: true }), [
    [1, 3, null],
    [0, 2, 4],
  ]);
  assert.deepEqual(passes(4, { twoSided: true, copies: 2 }), [
    [1, 3, 1, 3],
    [0, 2, 0, 2],
  ]);
  // One side only: nothing to turn.
  assert.deepEqual(passes(1, { twoSided: true }), [[0]]);
});

test('greys become dots, black and white stay as they are', () => {
  const width = 16;
  const rgba = (v) => new Uint8ClampedArray(width * 4).fill(v);
  const run = (v, rows) => {
    const d = new Dither(width);
    const bits = new Uint8Array(2 * rows);
    for (let y = 0; y < rows; y++) d.row(rgba(v), 0, bits, 2 * y);
    return bits;
  };
  assert.deepEqual([...run(0, 2)], [255, 255, 255, 255]);
  assert.deepEqual([...run(255, 2)], [0, 0, 0, 0]);
  // Mid grey: about half the dots, from one row to the next.
  const grey = run(128, 16);
  const black = [...grey].reduce((n, b) => n + b.toString(2).replaceAll('0', '').length, 0);
  assert.ok(Math.abs(black - 128) <= 12, `${black} dots`);
  // The leftmost dot is the high bit.
  const one = new Dither(8);
  const px = new Uint8ClampedArray(32).fill(255);
  px.set([0, 0, 0], 0);
  const bits = new Uint8Array(1);
  one.row(px, 0, bits, 0);
  assert.equal(bits[0], 0x80);
});
