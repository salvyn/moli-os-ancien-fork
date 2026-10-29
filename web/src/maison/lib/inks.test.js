import { test } from 'node:test';
import assert from 'node:assert/strict';
import { monochrome } from './inks.js';

test('a laser with black toner and a drum prints black and white only', () => {
  assert.equal(monochrome(['Toner noir', 'Tambour']), true);
  assert.equal(monochrome(['Black toner', 'Drum']), true);
});

test('a colour cartridge, or none known, keeps the choice', () => {
  assert.equal(monochrome(['Encre noire', 'Encre couleur']), false);
  assert.equal(monochrome(['Black', 'Cyan', 'Magenta', 'Yellow']), false);
  assert.equal(monochrome([]), false);
});
