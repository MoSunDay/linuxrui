//! Pure software rendering for the m0-panel bar.
//!
//! Everything here is side-effect free: `paint` builds the pixel buffer for a
//! panel frame from (width, height, label, clock), and the 5x7 bitmap font is
//! a plain lookup table. The Wayland-facing code only feeds in measurements
//! and uploads the returned bytes.

/// Panel background, matching the m0-prototype shell chrome (#16181d).
pub const BG: [u8; 3] = [22, 24, 29];
/// Xfce-blue accent line (#36689e), the same accent used by the shell UI.
pub const ACCENT: [u8; 3] = [54, 104, 158];
/// Foreground text color (#e8eaf0).
pub const TEXT: [u8; 3] = [232, 234, 240];
/// Dim text color for the trailing launcher note (#8c919b).
pub const DIM: [u8; 3] = [140, 145, 155];

/// Thickness of the accent strip along the panel bottom edge.
pub const ACCENT_ROWS: u32 = 3;
/// Horizontal padding around panel text, in pixels.
pub const PAD: u32 = 12;
/// Glyph render scale (bitmap font is 5x7).
pub const SCALE: u32 = 2;

/// Classic 5x7 bitmap font subset: digits, uppercase letters, and the few
/// punctuation marks the panel labels need. Rows are 5-bit values, MSB left.
const fn glyph(ch: char) -> Option<&'static [u8; 7]> {
    Some(match ch {
        '0' => &[0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => &[0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => &[0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => &[0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => &[0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => &[0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => &[0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => &[0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => &[0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => &[0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => &[0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => &[0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => &[0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => &[0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => &[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => &[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => &[0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => &[0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => &[0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => &[0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => &[0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => &[0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => &[0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => &[0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => &[0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => &[0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => &[0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => &[0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => &[0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => &[0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => &[0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => &[0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => &[0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => &[0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => &[0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' => &[0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        ':' => &[0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00],
        '.' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        '-' => &[0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00],
        '/' => &[0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        ' ' => &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => return None,
    })
}

/// Rendered width of `text` in pixels (glyphs + 1px tracking), or `None` when
/// the string contains characters the subset font cannot draw.
pub fn text_width(text: &str, scale: u32) -> Option<u32> {
    if text.is_empty() {
        return Some(0);
    }
    let mut w = 0u32;
    for ch in text.chars() {
        glyph(ch)?;
        w += (5 + 1) * scale;
    }
    // No tracking after the final glyph.
    Some(w - scale)
}

/// Paint one scaled glyph into the buffer. Returns without drawing when any
/// coordinate falls outside the buffer.
#[allow(clippy::too_many_arguments)]
fn draw_glyph(
    buf: &mut [u8],
    width: u32,
    height: u32,
    ch: char,
    x0: u32,
    y0: u32,
    scale: u32,
    rgb: [u8; 3],
) {
    let rows = match glyph(ch) {
        Some(rows) => rows,
        None => return,
    };
    for (ry, row) in rows.iter().enumerate() {
        for rx in 0..5u32 {
            if row & (1 << (4 - rx)) == 0 {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let x = x0 + rx * scale + dx;
                    let y = y0 + ry as u32 * scale + dy;
                    if x >= width || y >= height {
                        continue;
                    }
                    let off = ((y * width + x) * 4) as usize;
                    if off + 2 < buf.len() {
                        buf[off] = rgb[2];
                        buf[off + 1] = rgb[1];
                        buf[off + 2] = rgb[0];
                    }
                }
            }
        }
    }
}

/// Paint `text` left-aligned at (x0, y0). Silently skips unsupported glyphs.
#[allow(clippy::too_many_arguments)]
fn draw_text(
    buf: &mut [u8],
    width: u32,
    height: u32,
    text: &str,
    x0: u32,
    y0: u32,
    scale: u32,
    rgb: [u8; 3],
) {
    let mut x = x0;
    for ch in text.chars() {
        draw_glyph(buf, width, height, ch, x, y0, scale, rgb);
        x += (5 + 1) * scale;
    }
}

/// Text layout inside the panel: label on the left, clock on the right.
/// Returns `None` when the panel is too narrow for both.
pub fn layout(width: u32, label: &str, clock: &str) -> Option<(u32, u32, u32)> {
    let label_w = text_width(label, SCALE)?;
    let clock_w = text_width(clock, SCALE)?;
    let total = label_w + clock_w + 3 * PAD;
    if total > width {
        return None;
    }
    Some((PAD, width - PAD - clock_w, total))
}

/// Vertical text position: 5x7 glyph box centered in `height`.
pub fn text_top(height: u32) -> u32 {
    height.saturating_sub(7 * SCALE) / 2
}

/// Render a full panel frame as XRGB8888 (little-endian) bytes.
pub fn paint(width: u32, height: u32, label: &str, clock: &str) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 4) as usize];
    // Background fill.
    for px in buf.chunks_exact_mut(4) {
        px[0] = BG[2];
        px[1] = BG[1];
        px[2] = BG[0];
    }
    // Accent strip along the bottom edge.
    for y in height.saturating_sub(ACCENT_ROWS)..height {
        for x in 0..width {
            let off = ((y * width + x) * 4) as usize;
            buf[off] = ACCENT[2];
            buf[off + 1] = ACCENT[1];
            buf[off + 2] = ACCENT[0];
        }
    }
    if let Some((label_x, clock_x, _)) = layout(width, label, clock) {
        let y = text_top(height);
        draw_text(&mut buf, width, height, label, label_x, y, SCALE, TEXT);
        draw_text(&mut buf, width, height, clock, clock_x, y, SCALE, DIM);
    }
    buf
}

/// Current UTC wall clock as "HH:MM:SS UTC" (the host runs UTC; see the
/// m0-prototype clock for the same convention).
pub fn clock_text(unix_secs: u64) -> String {
    let rem = unix_secs % 86_400;
    let (h, m, s) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    format!("{h:02}:{m:02}:{s:02} UTC")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
        let off = ((y * width + x) * 4) as usize;
        [buf[off + 2], buf[off + 1], buf[off]] // back to RGB
    }

    #[test]
    fn clock_text_formats_around_the_day() {
        assert_eq!(clock_text(0), "00:00:00 UTC");
        assert_eq!(clock_text(3661), "01:01:01 UTC");
        assert_eq!(clock_text(86_399), "23:59:59 UTC");
    }

    #[test]
    fn layout_places_label_left_and_clock_right() {
        let (label_x, clock_x, total) = layout(1280, "APPLICATIONS", "12:00:00 UTC").unwrap();
        assert_eq!(label_x, PAD);
        let clock_w = text_width("12:00:00 UTC", SCALE).unwrap();
        assert_eq!(clock_x, 1280 - PAD - clock_w);
        assert!(total <= 1280);
    }

    #[test]
    fn layout_rejects_panels_that_cannot_fit_both_texts() {
        assert!(layout(120, "APPLICATIONS", "12:00:00 UTC").is_none());
    }

    #[test]
    fn paint_fills_background_accent_and_text() {
        let (w, h) = (1280u32, 42u32);
        let buf = paint(w, h, "M0 PANEL", "03:04:05 UTC");
        // Background in an untouched region.
        assert_eq!(px(&buf, w, 640, 0), BG);
        // Accent strip on the bottom row, background just above it.
        assert_eq!(px(&buf, w, 640, h - 1), ACCENT);
        assert_eq!(px(&buf, w, 640, h - ACCENT_ROWS - 1), BG);
        // A '0' glyph at scale 2 has ink at the left edge of the clock box.
        let (_, clock_x, _) = layout(w, "M0 PANEL", "03:04:05 UTC").unwrap();
        let found_text = (0..w).any(|x| px(&buf, w, x, text_top(h)) == TEXT);
        assert!(found_text, "text ink expected somewhere on the panel");
        // The clock is drawn in DIM; its first glyph pixel sits at clock_x+2.
        assert_eq!(px(&buf, w, clock_x + 2, text_top(h)), DIM);
    }

    #[test]
    fn paint_on_narrow_panels_still_draws_chrome() {
        let (w, h) = (40u32, 42u32);
        let buf = paint(w, h, "APPLICATIONS", "12:00:00 UTC");
        assert_eq!(px(&buf, w, 20, 5), BG);
        assert_eq!(px(&buf, w, 20, h - 1), ACCENT);
    }

    #[test]
    fn text_width_counts_tracking_between_glyphs() {
        assert_eq!(text_width("", SCALE), Some(0));
        assert_eq!(text_width("I", SCALE), Some(5 * SCALE));
        assert_eq!(text_width("II", SCALE), Some(11 * SCALE));
        assert_eq!(
            text_width("\u{4e2d}", SCALE),
            None,
            "non-Latin glyph rejected"
        );
    }
}
