//! Brother's own printing language (`application/vnd.brother-hbp`), the
//! only one the cheapest Brother lasers take (HL-1110 and HL-1210W, DCP-1510
//! and DCP-1610W…): a few PJL settings (paper, toner saving, density), then
//! the page as dots at 600 dpi, line after line, each line said as what
//! changed from the one above it.
//!
//! Written for Moli from the format as the free driver `brlaser` and its
//! users describe it (not from its code): the lines go in blocks of at most
//! 16 350 bytes, and a new block starts every 64 lines, as Brother's own
//! driver does; longer blocks print nothing on the HL-1210W, without a word
//! (brlaser issue #40).
//!
//! The page comes from the dashboard as a PBM (`image/x-portable-bitmap`,
//! `P4`: one bit a dot, 1 for black, each row to a whole byte): the dashboard
//! lays the pages out on the sheet and turns greys into dots.

use std::fmt::Write as _;

/// What the printer takes.
pub const MIME: &str = "application/vnd.brother-hbp";
/// What the dashboard sends: a page as dots.
pub const PBM: &str = "image/x-portable-bitmap";
/// Dots per inch, both ways.
pub const DPI: usize = 600;

/// A new block every this many lines.
const BAND: usize = 64;
/// A block's lines, in bytes, stay under this.
const BLOCK: usize = 16_350;
/// At most this many changes in a line (its first byte; 255 means blank).
const MOST_EDITS: usize = 254;
/// A row longer than this is not a page (a sheet of Legal is 638 bytes).
const MOST_ROW: usize = 10_000;
const MOST_ROWS: usize = 20_000;

/// The sizes the printer knows, in points (1/72 inch), as CUPS gives them.
const PAPERS: &[(&str, usize, usize)] = &[
    ("A4", 595, 842),
    ("A5", 420, 595),
    ("A6", 297, 420),
    ("B5", 499, 709),
    ("LETTER", 612, 792),
    ("LEGAL", 612, 1008),
    ("EXECUTIVE", 522, 756),
];

/// The kinds of paper it is told (its fuser heats for them).
const MEDIA: &[&str] = &["PLAIN", "THIN", "THICK", "THICKER", "BOND"];

/// A paper's size in dots: what the dashboard draws.
#[must_use]
pub fn dots(paper: &str) -> Option<(usize, usize)> {
    PAPERS
        .iter()
        .find(|(name, ..)| *name == paper)
        .map(|&(_, w, h)| (w * DPI / 72, h * DPI / 72))
}

/// One page as dots, read from a PBM.
#[derive(Debug, PartialEq, Eq)]
pub struct Page<'a> {
    pub width: usize,
    pub height: usize,
    /// Bytes a row.
    row: usize,
    data: &'a [u8],
}

impl Page<'_> {
    fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.data.chunks_exact(self.row).take(self.height)
    }
}

/// Reads a binary PBM (`P4`); why not, in words.
pub fn read_pbm(bytes: &[u8]) -> Result<Page<'_>, String> {
    let bad = |why: &str| moli_i18n::tr!("pilotes.ipp.points_illisibles", why = why);
    let mut at = 0;
    let mut word = || -> Option<&[u8]> {
        // Spaces and `#` comments between the words of the header.
        loop {
            match bytes.get(at)? {
                b'#' => {
                    while *bytes.get(at)? != b'\n' {
                        at += 1;
                    }
                }
                c if c.is_ascii_whitespace() => at += 1,
                _ => break,
            }
        }
        let start = at;
        while bytes.get(at).is_some_and(|c| !c.is_ascii_whitespace()) {
            at += 1;
        }
        Some(&bytes[start..at])
    };
    if word() != Some(b"P4") {
        return Err(bad("P4"));
    }
    let mut number = || {
        word()
            .and_then(|w| std::str::from_utf8(w).ok())
            .and_then(|w| w.parse::<usize>().ok())
            .filter(|&n| n > 0)
    };
    let (Some(width), Some(height)) = (number(), number()) else {
        return Err(bad("size"));
    };
    let row = width.div_ceil(8);
    if row > MOST_ROW || height > MOST_ROWS {
        return Err(bad("size"));
    }
    // One space after the height, then the dots.
    let data = bytes.get(at + 1..).unwrap_or_default();
    if data.len() < row * height {
        return Err(bad("short"));
    }
    Ok(Page {
        width,
        height,
        row,
        data,
    })
}

/// What the printer is told before the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings<'a> {
    /// Shown on the printer's side (ASCII only, kept short).
    pub name: &'a str,
    pub copies: u32,
    pub paper: &'a str,
    pub media: &'a str,
    pub toner_save: bool,
    pub density: i8,
}

/// The whole print job for one page; why not, in words (a paper or a page
/// the printer would not take).
pub fn job(settings: &Settings, page: &Page) -> Result<Vec<u8>, String> {
    let Some((width, height)) = dots(settings.paper) else {
        return Err(moli_i18n::tr!(
            "pilotes.ipp.papier_inconnu",
            paper = settings.paper
        ));
    };
    if !MEDIA.contains(&settings.media) {
        return Err(moli_i18n::tr!(
            "pilotes.ipp.type_inconnu",
            media = settings.media
        ));
    }
    if (page.width, page.height) != (width, height) {
        return Err(moli_i18n::tr!(
            "pilotes.ipp.taille_page",
            got = format!("{}×{}", page.width, page.height),
            paper = settings.paper,
            want = format!("{width}×{height}")
        ));
    }
    let name = job_name(settings.name);
    let mut out = vec![0; 128];
    let mut head = String::new();
    // `write!` to a String does not fail.
    let _ = writeln!(head, "\x1b%-12345X@PJL\n@PJL JOB NAME=\"{name}\"");
    head.push_str("\x1b%-12345X@PJL\n");
    head.push_str("@PJL SET RAS1200MODE = FALSE\n");
    let _ = writeln!(head, "@PJL SET RESOLUTION = {DPI}");
    let _ = writeln!(
        head,
        "@PJL SET ECONOMODE = {}",
        if settings.toner_save { "ON" } else { "OFF" }
    );
    head.push_str("@PJL SET SOURCETRAY = AUTO\n");
    let _ = writeln!(head, "@PJL SET MEDIATYPE = {}", settings.media);
    // Only when asked: the printer's own setting otherwise.
    let density = settings.density.clamp(-6, 6);
    if density != 0 {
        let _ = writeln!(head, "@PJL SET DENSITY={density}");
        let _ = writeln!(head, "@PJL SET DEVBIASADJUST={density}");
    }
    let _ = writeln!(head, "@PJL SET PAPER = {}", settings.paper);
    head.push_str("@PJL SET PAGEPROTECT = AUTO\n@PJL SET ORIENTATION = PORTRAIT\n");
    head.push_str("@PJL ENTER LANGUAGE = PCL\n");
    // Reset, copies, resolution, one side, then the dots.
    let _ = write!(
        head,
        "\x1bE\x1b&l{}X\x1b&u{DPI}D\x1b*t{DPI}R\x1b&l0S\x1b*b1030m",
        settings.copies.max(1)
    );
    out.extend_from_slice(head.as_bytes());
    dots_of(&mut out, page);
    out.extend_from_slice(b"1030M\x0c");
    let tail = format!("\x1b%-12345X@PJL\n@PJL EOJ NAME=\"{name}\"\n\x1b%-12345X\n");
    out.extend_from_slice(tail.as_bytes());
    Ok(out)
}

/// The name in plain ASCII, without what would end the PJL string.
fn job_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| match c {
            ' '..='~' if c != '"' && c != '\\' => c,
            _ => ' ',
        })
        .take(79)
        .collect();
    if name.trim().is_empty() {
        "Moli".into()
    } else {
        name
    }
}

/// The page's lines, in blocks: `{bytes}w`, a zero, the number of lines,
/// then the lines. A block's first line stands alone; the others say what
/// changed from the line above.
fn dots_of(out: &mut Vec<u8>, page: &Page) {
    let mut block = Vec::with_capacity(BLOCK);
    let mut lines = 0_usize;
    let flush = |out: &mut Vec<u8>, block: &mut Vec<u8>, lines: &mut usize| {
        if *lines > 0 {
            out.extend_from_slice(format!("{}w", block.len() + 2).as_bytes());
            // At most 64 lines a block: always one byte.
            out.extend_from_slice(&[0, u8::try_from(*lines).unwrap_or(u8::MAX)]);
            out.append(block);
            *lines = 0;
        }
    };
    let mut line = Vec::with_capacity(page.row + 16);
    let mut above: Option<&[u8]> = None;
    for (y, row) in page.rows().enumerate() {
        line.clear();
        if y % BAND == 0 {
            flush(out, &mut block, &mut lines);
            encode(&mut line, row, None);
        } else {
            encode(&mut line, row, above);
            if block.len() + line.len() >= BLOCK {
                flush(out, &mut block, &mut lines);
                line.clear();
                encode(&mut line, row, None);
            }
        }
        block.extend_from_slice(&line);
        lines += 1;
        above = Some(row);
    }
    flush(out, &mut block, &mut lines);
}

/// One line: 255 when blank; else how many changes, then each change
/// (bytes copied as they are, or one byte repeated), each placed after the
/// bytes it leaves as they were above. Without a line above, all its bytes.
fn encode(out: &mut Vec<u8>, row: &[u8], above: Option<&[u8]>) {
    if row.iter().all(|&b| b == 0) {
        out.push(0xff);
        return;
    }
    let Some(above) = above else {
        out.push(1);
        copy(out, 0, row);
        return;
    };
    let count_at = out.len();
    out.push(0);
    // Past the last byte that changed, the line is the one above.
    let end = row
        .iter()
        .zip(above)
        .rposition(|(a, b)| a != b)
        .map_or(0, |i| i + 1);
    let same = |i: usize| row[i] == above[i];
    let run = |i: usize| row[i..end].iter().take_while(|&&b| b == row[i]).count();
    let mut edits = 0;
    let mut done = 0;
    let mut i = 0;
    while i < end {
        if same(i) {
            i += 1;
            continue;
        }
        edits += 1;
        if edits == MOST_EDITS {
            // The last change it can say: the rest of the line, as it is.
            copy(out, i - done, &row[i..end]);
            break;
        }
        let n = run(i);
        if n >= 3 {
            repeat(out, i - done, n, row[i]);
            i += n;
        } else {
            // Copied up to two bytes left as they were, or a run worth repeating.
            let mut j = i + 1;
            while j < end && !(same(j) && j + 1 < end && same(j + 1)) && run(j) < 3 {
                j += 1;
            }
            copy(out, i - done, &row[i..j]);
            i = j;
        }
        done = i;
    }
    // At most 254 changes: always one byte.
    out[count_at] = u8::try_from(edits).unwrap_or(u8::MAX);
}

/// `0 ssss nnn`: skip `s`, then `n + 1` bytes as they are. A field at its
/// most (15 to skip, 7 more bytes) goes on in the bytes after it.
fn copy(out: &mut Vec<u8>, skip: usize, bytes: &[u8]) {
    let n = bytes.len() - 1;
    out.push(u8::try_from((skip.min(15) << 3) | n.min(7)).unwrap_or(0));
    beyond(out, skip, 15);
    beyond(out, n, 7);
    out.extend_from_slice(bytes);
}

/// `1 ss nnnnn`: skip `s`, then the next byte `n + 2` times (3 to skip and
/// 31 at most, the rest after it).
fn repeat(out: &mut Vec<u8>, skip: usize, n: usize, byte: u8) {
    let n = n - 2;
    out.push(0x80 | u8::try_from((skip.min(3) << 5) | n.min(31)).unwrap_or(0));
    beyond(out, skip, 3);
    beyond(out, n, 31);
    out.push(byte);
}

/// What does not fit in a field at its most: in bytes of 255, then the rest
/// (a zero when nothing is left).
fn beyond(out: &mut Vec<u8>, value: usize, most: usize) {
    if value >= most {
        let rest = value - most;
        out.resize(out.len() + rest / 255, 255);
        out.push(u8::try_from(rest % 255).unwrap_or(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the dots back as the printer would: the decoder the tests
    /// check every page against.
    fn decode(job: &[u8], row: usize) -> Vec<Vec<u8>> {
        let start = job.windows(8).position(|w| w == b"\x1b*b1030m").unwrap() + 8;
        let mut at = start;
        let mut lines: Vec<Vec<u8>> = Vec::new();
        let mut blocks = 0;
        while &job[at..at + 5] != b"1030M" {
            let digits = job[at..].iter().position(|&b| b == b'w').unwrap();
            let size: usize = std::str::from_utf8(&job[at..at + digits])
                .unwrap()
                .parse()
                .unwrap();
            assert!(size - 2 < BLOCK, "block of {size} bytes");
            at += digits + 1;
            assert_eq!(job[at], 0);
            let count = usize::from(job[at + 1]);
            assert!(count <= BAND);
            // Never across a band.
            let first = lines.len();
            assert_eq!(first / BAND, (first + count - 1) / BAND);
            let end = at + size;
            at += 2;
            for k in 0..count {
                let mut line = if k == 0 {
                    vec![0xAA; row] // garbage: a block's first line stands alone
                } else {
                    lines.last().cloned().unwrap_or_else(|| vec![0; row])
                };
                at = decode_line(job, at, &mut line);
                lines.push(line);
            }
            assert_eq!(at, end);
            blocks += 1;
        }
        assert!(blocks > 0);
        lines
    }

    fn decode_line(job: &[u8], mut at: usize, line: &mut [u8]) -> usize {
        let more = |at: &mut usize, low: usize, most: usize| {
            let mut v = low;
            if low == most {
                loop {
                    let b = usize::from(job[*at]);
                    *at += 1;
                    v += b;
                    if b != 255 {
                        break;
                    }
                }
            }
            v
        };
        let edits = job[at];
        at += 1;
        if edits == 0xff {
            line.fill(0);
            return at;
        }
        let mut x = 0;
        for _ in 0..edits {
            let head = usize::from(job[at]);
            at += 1;
            if head & 0x80 == 0 {
                x += more(&mut at, (head >> 3) & 15, 15);
                let n = more(&mut at, head & 7, 7) + 1;
                line[x..x + n].copy_from_slice(&job[at..at + n]);
                at += n;
                x += n;
            } else {
                x += more(&mut at, (head >> 5) & 3, 3);
                let n = more(&mut at, head & 31, 31) + 2;
                line[x..x + n].fill(job[at]);
                at += 1;
                x += n;
            }
        }
        at
    }

    fn pbm(width: usize, height: usize, data: &[u8]) -> Vec<u8> {
        let mut out = format!("P4\n# Moli\n{width} {height}\n").into_bytes();
        out.extend_from_slice(data);
        out
    }

    /// The same made-up page every time.
    fn noise(seed: u64, n: usize, black: u64) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                if s % 100 < black {
                    u8::try_from(s >> 56).unwrap()
                } else {
                    0
                }
            })
            .collect()
    }

    const SETTINGS: Settings = Settings {
        name: "Facture (p. 1/2)",
        copies: 2,
        paper: "A4",
        media: "PLAIN",
        toner_save: false,
        density: 0,
    };

    #[test]
    fn a_line_says_only_what_changed() {
        let mut out = Vec::new();
        encode(&mut out, &[0, 0, 0xaa, 0, 0], Some(&[0; 5]));
        assert_eq!(out, [1, 0x10, 0xaa]);
        out.clear();
        encode(&mut out, &[0xff; 40], Some(&[0; 40]));
        // 40 times 0xff: 31 in the head, 7 after it.
        assert_eq!(out, [1, 0x9f, 7, 0xff]);
        out.clear();
        encode(&mut out, &[0; 40], Some(&[1; 40]));
        assert_eq!(out, [0xff]);
        out.clear();
        encode(&mut out, &[1, 2, 3], Some(&[1, 2, 3]));
        assert_eq!(out, [0]);
        out.clear();
        // Alone, all of it: 20 bytes from 0, the count past 7 in a byte after.
        let row: Vec<u8> = (1..=20).collect();
        encode(&mut out, &row, None);
        assert_eq!(&out[..3], [1, 0x07, 12]);
        assert_eq!(&out[3..], &row[..]);
    }

    #[test]
    fn a_skip_past_the_field_goes_on_after_it() {
        let mut out = Vec::new();
        let mut row = vec![0; 600];
        row[300] = 5;
        encode(&mut out, &row, Some(&[0; 600]));
        // 300 to skip: 15 in the head, then 255 and 30.
        assert_eq!(out, [1, 0x78, 255, 30, 5]);
    }

    #[test]
    fn a_line_of_many_changes_ends_in_one() {
        // One byte in three changed: more changes than a line can say.
        let above = vec![0; 900];
        let row: Vec<u8> = (0..900).map(|i| if i % 3 == 0 { 7 } else { 0 }).collect();
        let mut out = Vec::new();
        encode(&mut out, &row, Some(&above));
        assert_eq!(usize::from(out[0]), MOST_EDITS);
        let mut line = above.clone();
        let mut job = out.clone();
        job.push(0);
        decode_line(&job, 0, &mut line);
        assert_eq!(line, row);
    }

    #[test]
    fn every_page_reads_back_dot_for_dot() {
        let (width, height) = dots("A4").unwrap();
        assert_eq!((width, height), (4958, 7016));
        let row = width.div_ceil(8);
        for (seed, black) in [(1, 0), (2, 3), (3, 40), (4, 100)] {
            let mut data = noise(seed, row * height, black);
            // Text-like stretches: a line repeated, a band left blank.
            for y in (100..200).step_by(2) {
                let (a, b) = data.split_at_mut(y * row);
                b[..row].copy_from_slice(&a[(y - 1) * row..]);
            }
            data[500 * row..600 * row].fill(0);
            let bytes = pbm(width, height, &data);
            let page = read_pbm(&bytes).unwrap();
            let job = job(&SETTINGS, &page).unwrap();
            let lines = decode(&job, row);
            assert_eq!(lines.len(), height);
            assert!(lines.iter().flatten().eq(data.iter()), "seed {seed}");
        }
    }

    #[test]
    fn the_settings_go_before_the_page() {
        let (width, height) = dots("A5").unwrap();
        let data = vec![0; width.div_ceil(8) * height];
        let bytes = pbm(width, height, &data);
        let page = read_pbm(&bytes).unwrap();
        let settings = Settings {
            name: "Lettre « été »\"",
            paper: "A5",
            media: "THICK",
            toner_save: true,
            density: -2,
            ..SETTINGS
        };
        let job = job(&settings, &page).unwrap();
        assert!(job[..128].iter().all(|&b| b == 0));
        let text = String::from_utf8_lossy(&job);
        for said in [
            "@PJL JOB NAME=\"Lettre    t    \"",
            "@PJL SET ECONOMODE = ON\n",
            "@PJL SET MEDIATYPE = THICK\n",
            "@PJL SET DENSITY=-2\n",
            "@PJL SET PAPER = A5\n",
            "\x1bE\x1b&l2X\x1b&u600D\x1b*t600R\x1b&l0S\x1b*b1030m",
            "1030M\x0c\x1b%-12345X@PJL\n@PJL EOJ NAME=",
        ] {
            assert!(text.contains(said), "{said:?} missing");
        }
        // A blank page: one byte a line.
        assert_eq!(decode(&job, width.div_ceil(8)).len(), height);
        // The printer's own density when none is asked.
        let plain = self::job(
            &Settings {
                density: 0,
                ..settings
            },
            &page,
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&plain).contains("DENSITY"));
    }

    #[test]
    fn what_the_printer_cannot_take_is_refused() {
        let (width, height) = dots("A4").unwrap();
        let data = vec![0; width.div_ceil(8) * height];
        let bytes = pbm(width, height, &data);
        let page = read_pbm(&bytes).unwrap();
        assert!(
            job(
                &Settings {
                    paper: "A3",
                    ..SETTINGS
                },
                &page
            )
            .is_err()
        );
        assert!(
            job(
                &Settings {
                    media: "TRANS",
                    ..SETTINGS
                },
                &page
            )
            .is_err()
        );
        let small = pbm(width / 2, height / 2, &data);
        assert!(job(&SETTINGS, &read_pbm(&small).unwrap()).is_err());
        assert!(read_pbm(b"P1\n1 1\n1").is_err());
        assert!(read_pbm(b"P4\n8 2\n\x01").is_err());
        assert!(read_pbm(b"P4\n100000 2\n").is_err());
        let tiny = read_pbm(b"P4 9 1 \xff\x80").unwrap();
        assert_eq!((tiny.width, tiny.height, tiny.row), (9, 1, 2));
    }
}
