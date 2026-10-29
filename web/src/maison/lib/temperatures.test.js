import { test } from 'node:test';
import assert from 'node:assert/strict';
import { probesOf, roomTemperature, lineOf, statsOf, scaleOf, valueAt } from './temperatures.js';

const dev = (id, ...keys) => ({ id, points: keys.map((key) => ({ key })) });

test('a room is measured by its sensors and its heaters alike', () => {
  const sensor = dev('z2m:s', 'temperature', 'humidity');
  const radiator = dev('tuya:r', 'temp_current', 'target_temperature');
  const stove = dev('micronova:p', 'temperature', 'flue_temperature');
  const door = dev('z2m:d', 'contact');
  assert.deepEqual(probesOf({ sensors: [sensor, door], climates: [radiator, stove] }), [
    { id: 'z2m:s', point: 'z2m:s/temperature' },
    { id: 'tuya:r', point: 'tuya:r/temp_current' },
    { id: 'micronova:p', point: 'micronova:p/temperature' },
  ]);
  assert.deepEqual(probesOf({ sensors: [door], climates: [radiator] }), [{ id: 'tuya:r', point: 'tuya:r/temp_current' }]);
  assert.deepEqual(probesOf({ sensors: [], climates: [] }), []);
});

test('the room temperature is the average of all it reads, offline left out', () => {
  const reads = { 'z2m:s': 19, 'tuya:r': 18.5, 'micronova:p': 18.5, 'z2m:off': 30, 'z2m:none': null };
  const read = (id) => reads[id];
  const devices = [
    dev('z2m:s', 'temperature'),
    dev('tuya:r', 'temp_current'),
    dev('micronova:p', 'temperature', 'flue_temperature'),
    { ...dev('z2m:off', 'temperature'), online: false },
    dev('z2m:none', 'temperature'),
    dev('z2m:d', 'contact'),
  ];
  assert.equal(roomTemperature(devices, read), (19 + 18.5 + 18.5) / 3);
  assert.equal(roomTemperature([dev('z2m:d', 'contact')], read), null);
});

test('a series becomes a line, from the value before the window to its end', () => {
  const raw = { from: 0, to: 100, before: [-5, 19], raw: [[40, 20.5], [70, 'x']], buckets: [] };
  assert.deepEqual(lineOf(raw), [[0, 19], [40, 19], [40, 20.5], [100, 20.5]]);
  const bucketed = { from: 0, to: 100, raw: [], last: [80, 21.5], buckets: [{ ts: 0, avg: 19 }, { ts: 50, avg: null }, { ts: 60, avg: 21 }] };
  assert.deepEqual(lineOf(bucketed), [[0, 19], [60, 21], [100, 21.5]]);
  assert.deepEqual(statsOf(lineOf(raw)), { min: 19, max: 20.5, last: 20.5 });
});

test('the rooms share whole degrees, at least four tall', () => {
  assert.deepEqual(scaleOf([[[0, 19.4]], [[0, 20.6]]]), { min: 18, max: 22, ticks: [18, 19, 20, 21, 22] });
  const wide = scaleOf([[[0, 4]], [[0, 23]]]);
  assert.deepEqual(wide.ticks, [0, 5, 10, 15, 20, 25]);
});

test('the value at a moment is the nearest one, not too far', () => {
  const line = [[0, 18], [100, 19], [200, 20]];
  assert.equal(valueAt(line, 140, 60), 19);
  assert.equal(valueAt(line, 160, 60), 20);
  assert.equal(valueAt(line, 400, 60), null);
});

test('a reading of a few seconds that falls back is a glitch', () => {
  const s = { from: 0, to: 1_000_000, raw: [[0, 18], [100_000, 20], [104_000, 18], [500_000, 18.5]], buckets: [] };
  assert.deepEqual(lineOf(s), [[0, 18], [500_000, 18], [500_000, 18.5], [1_000_000, 18.5]]);
  // Held long enough, it stays.
  const held = { ...s, raw: [[0, 18], [100_000, 20], [300_000, 18]] };
  assert.equal(lineOf(held).length, 6);
});
