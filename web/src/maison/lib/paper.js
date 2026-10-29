// Printing on a paper printer (an IPP printer Moli knows: `printer: true`):
// the pages are made here, in the browser (a PDF rendered page by page, a
// photo re-encoded), then sent one by one to Moli, which queues them.

import { t } from '../../lib/i18n.svelte.js';
import { request } from '../../lib/api.js';
import { sheetSize, cells, place, sides, passes, Dither, pbmHeader } from './sheet.js';

/** A page's longest side, at most (A4 at 300 dpi): bigger is only heavier. */
const MAX_SIDE = 3508;
/** A PDF page is rendered at this resolution (text stays sharp). */
const PDF_DPI = 200;
const QUALITY = 0.9;
/** Moli's queue full, or too many pages at once: wait, then again. */
const RETRY_WAIT = { 409: 15_000, 429: 4_000 };
const RETRIES = 40;

export const isPaperPrinter = (d) => d?.printer === true;

/** On for this long, a printer is worth a word; for this long, it is a worry. */
const LONG_ON = 3 * 3_600_000;
const TOO_LONG_ON = 12 * 3_600_000;

/** How long a paper printer has been on, in minutes (Moli says since when
 *  it answers); null when it is off or Moli does not know. */
export function onMinutes(d, now = Date.now()) {
  if (!d || d.online !== true || typeof d.online_since !== 'number') return null;
  return Math.max(0, Math.floor((now - d.online_since) / 60_000));
}

/** « long » after 3 hours on, « too_long » after 12; null before. */
export function onLevel(d, now = Date.now()) {
  const m = onMinutes(d, now);
  if (m == null) return null;
  return m * 60_000 >= TOO_LONG_ON ? 'too_long' : m * 60_000 >= LONG_ON ? 'long' : null;
}

export const isPdf = (file) => file.type === 'application/pdf' || /\.pdf$/i.test(file.name);

/** The canvas as a JPEG, then freed (Safari caps the canvases' memory). */
function jpeg(canvas) {
  return new Promise((resolve, reject) =>
    canvas.toBlob(
      (b) => {
        canvas.width = 0;
        canvas.height = 0;
        if (b) resolve(b);
        else reject(new Error(t('salon.papier.page_impossible')));
      },
      'image/jpeg',
      QUALITY,
    ),
  );
}

async function photo(file) {
  // EXIF orientation applied (a phone photo stands the right way up).
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(bitmap.width * scale);
  canvas.height = Math.round(bitmap.height * scale);
  const g = canvas.getContext('2d');
  g.fillStyle = '#fff';
  g.fillRect(0, 0, canvas.width, canvas.height);
  g.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close?.();
  return jpeg(canvas);
}

/** The pages of a file, made one at a time (a long PDF is never all in
 *  memory): `{ count, page(n) → Promise<Blob>, close() }`, n from 1. */
export async function pagesOf(file) {
  if (isPdf(file)) {
    // Loaded only when a PDF is printed (the library is big); the legacy
    // build also runs on older tablets (an iPad left on an older iPadOS).
    const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
    pdfjs.GlobalWorkerOptions.workerSrc = (await import('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url')).default;
    const task = pdfjs.getDocument({ data: new Uint8Array(await file.arrayBuffer()) });
    const doc = await task.promise;
    return {
      count: doc.numPages,
      async page(n) {
        const page = await doc.getPage(n);
        const base = page.getViewport({ scale: 1 });
        // 200 dpi, but never a side above A4 at 300 dpi (an A0 plan).
        const scale = Math.min(PDF_DPI / 72, MAX_SIDE / Math.max(base.width, base.height));
        const viewport = page.getViewport({ scale });
        const canvas = document.createElement('canvas');
        canvas.width = Math.ceil(viewport.width);
        canvas.height = Math.ceil(viewport.height);
        await page.render({ canvas, viewport, intent: 'print', background: '#ffffff' }).promise;
        page.cleanup();
        return jpeg(canvas);
      },
      // pdf.js 6: the loading task frees the document (and its worker).
      close: () => task.destroy(),
    };
  }
  if (file.type.startsWith('image/')) return { count: 1, page: () => photo(file), close() {} };
  throw new Error(t('salon.papier.choisir'));
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** One page to Moli; a full queue or a burst is waited out, not failed. */
async function send(id, blob, query, waiting) {
  for (let attempt = 0; ; attempt++) {
    let status;
    try {
      await request('POST', `/api/devices/${encodeURIComponent(id)}/print?${query}`, blob, { type: blob.type });
      return;
    } catch (err) {
      if (!err.status) throw err;
      status = err.status;
      const said = err.body.error ?? t('salon.papier.erreur', { status });
      const busy = status === 429 || (status === 409 && /pleine|full/.test(said));
      if (!busy || attempt >= RETRIES) throw new Error(said);
    }
    waiting();
    await sleep(RETRY_WAIT[status] ?? 10_000);
  }
}

/** The pages that went, said with the error (they print: sending them
 *  again would print them twice); `partial`: the card shows it as it is. */
function partly(err, done, total) {
  if (done === 0) return err;
  const partial = new Error(t('salon.papier.partiel', { count: done, total, message: err.message }));
  partial.partial = true;
  return partial;
}

/** Sends a file, page after page. `progress({ done, total, step })`, step
 *  « prepare », « envoie », « attend » (Moli's queue is full) or « fini ».
 *  A failure says how many pages went (they print: sending again would
 *  print them twice). */
export async function printFile(id, file, { copies = 1, color = true, pages: only = null } = {}, progress = () => {}) {
  const pages = await pagesOf(file);
  const name = file.name.replace(/\.[^.]+$/, '') || 'Document';
  const wanted = (only ?? Array.from({ length: pages.count }, (_, i) => i + 1)).filter((n) => n >= 1 && n <= pages.count);
  let done = 0;
  try {
    for (const n of wanted) {
      progress({ done, total: wanted.length, step: 'prepare' });
      const blob = await pages.page(n);
      progress({ done, total: wanted.length, step: 'envoie' });
      const query = new URLSearchParams({
        name: pages.count > 1 ? `${name} (p. ${n}/${pages.count})` : name,
        copies: String(copies),
        color: String(color),
      });
      await send(id, blob, query, () => progress({ done, total: wanted.length, step: 'attend' }));
      done += 1;
    }
    progress({ done, total: wanted.length, step: 'fini' });
  } catch (err) {
    throw partly(err, done, wanted.length);
  } finally {
    pages.close();
  }
}

// ---- pages as dots (a laser that takes only Brother's language) ----------------------

/** What Moli reads a sheet of dots from. */
const PBM = 'image/x-portable-bitmap';
/** A band of the sheet drawn at a time: a whole A4 at 600 dpi (35 million
 *  dots) is more than a tablet lets a canvas be. */
const BAND_PIXELS = 6_000_000;

const free = (canvas) => {
  canvas.width = 0;
  canvas.height = 0;
};

/** The part of a placed page inside the band, in whole dots (null: none). */
function clip(at, top, rows, width) {
  const x = Math.max(0, Math.floor(at.x));
  const y = Math.max(top, Math.floor(at.y));
  const right = Math.min(width, Math.ceil(at.x + at.w));
  const bottom = Math.min(top + rows, Math.ceil(at.y + at.h));
  return right > x && bottom > y ? { x, y, w: right - x, h: bottom - y } : null;
}

/** A file's pages, to be drawn on a sheet: `{ count, actual (a real size
 *  exists), size(n) → [w, h] in points (pixels for a photo), draw(n, at, g,
 *  top, rows, width), close() }`, n from 1. */
async function drawable(file) {
  if (isPdf(file)) {
    const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
    pdfjs.GlobalWorkerOptions.workerSrc = (await import('pdfjs-dist/legacy/build/pdf.worker.min.mjs?url')).default;
    const task = pdfjs.getDocument({ data: new Uint8Array(await file.arrayBuffer()) });
    const doc = await task.promise;
    return {
      count: doc.numPages,
      actual: true,
      async size(n) {
        const v = (await doc.getPage(n)).getViewport({ scale: 1 });
        return [v.width, v.height];
      },
      async draw(n, at, g, top, rows, width) {
        const box = clip(at, top, rows, width);
        if (!box) return;
        const page = await doc.getPage(n);
        const viewport = page.getViewport({ scale: at.scale, rotation: (page.rotate + at.turn) % 360 });
        // Only the part in this band, drawn apart (pdf.js whitens its whole canvas).
        const canvas = document.createElement('canvas');
        canvas.width = box.w;
        canvas.height = box.h;
        await page.render({ canvas, viewport, intent: 'print', transform: [1, 0, 0, 1, at.x - box.x, at.y - box.y] }).promise;
        g.drawImage(canvas, box.x, box.y - top);
        free(canvas);
        page.cleanup();
      },
      close: () => task.destroy(),
    };
  }
  if (file.type.startsWith('image/')) {
    // EXIF orientation applied (a phone photo stands the right way up).
    const bitmap = await createImageBitmap(file);
    return {
      count: 1,
      actual: false,
      size: async () => [bitmap.width, bitmap.height],
      draw(n, at, g, top) {
        g.save();
        g.translate(at.x, at.y - top);
        if (at.turn) {
          g.translate(at.w, 0);
          g.rotate(Math.PI / 2);
        }
        g.drawImage(bitmap, 0, 0, at.turn ? at.h : at.w, at.turn ? at.w : at.h);
        g.restore();
      },
      close: () => bitmap.close?.(),
    };
  }
  throw new Error(t('salon.papier.choisir'));
}

/** A sheet's dots: drawn band after band, greys turned into dots. */
async function sheetDots(src, placed, [width, height]) {
  const rowBytes = Math.ceil(width / 8);
  const bits = new Uint8Array(rowBytes * height);
  if (!placed.length) return bits;
  const bandRows = Math.max(64, Math.floor(BAND_PIXELS / width));
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = Math.min(bandRows, height);
  const g = canvas.getContext('2d', { willReadFrequently: true });
  const dither = new Dither(width);
  try {
    for (let top = 0; top < height; top += bandRows) {
      const rows = Math.min(bandRows, height - top);
      g.fillStyle = '#fff';
      g.fillRect(0, 0, width, canvas.height);
      for (const { n, at } of placed) await src.draw(n, at, g, top, rows, width);
      const rgba = g.getImageData(0, 0, width, rows).data;
      for (let y = 0; y < rows; y++) dither.row(rgba, y * width * 4, bits, (top + y) * rowBytes);
    }
  } finally {
    free(canvas);
  }
  return bits;
}

/** Sends a file to a printer that takes pages as dots, sheet after sheet:
 *  `paper`, `media`, `tonerSave`, `density` for the printer; `perSheet` (1,
 *  2, 4) and `actual` (real size) for the layout; `twoSided` by hand:
 *  `turnOver()` resolves when the person has put the sheets back (it
 *  rejects to stop). `progress({ done, total, step })` as `printFile`, and
 *  « retourner » while the sheets are being turned over. */
export async function printDots(id, file, options, progress = () => {}, turnOver = async () => {}) {
  const { copies = 1, pages: only = null, paper = 'A4', media = 'PLAIN', tonerSave = false, density = 0, perSheet = 1, actual = false, twoSided = false } = options;
  const src = await drawable(file);
  const name = file.name.replace(/\.[^.]+$/, '') || 'Document';
  let done = 0;
  let total = 0;
  try {
    const wanted = (only ?? Array.from({ length: src.count }, (_, i) => i + 1)).filter((n) => n >= 1 && n <= src.count);
    const size = sheetSize(paper);
    const spots = cells(...size, perSheet);
    const groups = sides(wanted, perSheet);
    // One sheet: its copies are said to the printer; more: sent again, sorted.
    const single = groups.length === 1;
    const runs = passes(groups.length, { twoSided, copies: single ? 1 : copies });
    total = runs.flat().length;
    for (const [k, run] of runs.entries()) {
      if (k > 0) {
        progress({ done, total, step: 'retourner' });
        await turnOver();
      }
      for (const side of run) {
        progress({ done, total, step: 'prepare' });
        const group = side === null ? [] : groups[side];
        const placed = [];
        for (const [i, n] of group.entries()) {
          placed.push({ n, at: place(await src.size(n), spots[i], { actual: actual && src.actual && perSheet === 1, sheet: size }) });
        }
        const bits = await sheetDots(src, placed, size);
        const blob = new Blob([pbmHeader(...size), bits], { type: PBM });
        const pagesSaid = group.length > 1 ? `${group[0]}-${group.at(-1)}` : String(group[0] ?? '');
        progress({ done, total, step: 'envoie' });
        const query = new URLSearchParams({
          name: side === null ? t('salon.papier.dos_blanc', { nom: name }) : src.count > 1 ? `${name} (p. ${pagesSaid}/${src.count})` : name,
          copies: String(single ? copies : 1),
          color: 'false',
          paper,
          media,
          toner_save: String(tonerSave),
          density: String(density),
        });
        await send(id, blob, query, () => progress({ done, total, step: 'attend' }));
        done += 1;
      }
    }
    progress({ done, total, step: 'fini' });
  } catch (err) {
    throw partly(err, done, total);
  } finally {
    src.close();
  }
}

/** « 1-3, 5 » → [1, 2, 3, 5]; empty: every page (null). */
export function parsePages(text, count) {
  const raw = String(text ?? '').trim();
  if (!raw) return null;
  const out = new Set();
  for (const part of raw.split(/[,;\s]+/).filter(Boolean)) {
    const m = /^(\d+)(?:-(\d+))?$/.exec(part);
    if (!m) return [];
    const a = Number(m[1]);
    const b = Number(m[2] ?? m[1]);
    for (let n = Math.min(a, b); n <= Math.max(a, b) && n <= count; n++) if (n >= 1) out.add(n);
  }
  return [...out].sort((x, y) => x - y);
}
