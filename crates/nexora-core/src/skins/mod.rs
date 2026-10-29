//! スキンのライブラリ。ここでは、スキンを画素の内容で見分けるための `pixel_hash` だけを持つ。

pub mod library;
pub mod ops;

use sha2::{Digest, Sha256};

use crate::net::hash::hex;

/// 画素の内容を見るときの上限の寸法（展開した後の大きさを抑える）。
const MAX_SIDE: u32 = 4096;
/// 見分けに使う上の行数。64×32 の旧形式と 64×64 の新形式で、共通の上半分だけを比べる。
const HASH_ROWS: usize = 32;

/// スキンの PNG を、ファイルのバイトではなく画素の内容で見分けるハッシュ（小文字の 16 進）。
///
/// RGBA8 に展開し、上の 32 行だけを取り、透明（alpha が 0）の画素の RGB を 0 にして
/// SHA-256 を取る。圧縮のしかたや、透明な画素に隠れた RGB の違いは影響しない。
/// PNG として読めなければ、生のバイトの SHA-256 にする（落ちない）。
pub fn pixel_hash(png: &[u8]) -> String {
    match decode_top_rows(png) {
        Some(rgba) => hex(&Sha256::digest(rgba)),
        None => hex(&Sha256::digest(png)),
    }
}

/// 上の 32 行を RGBA8 で返す。読めなければ None。
fn decode_top_rows(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let src = &buf[..frame.buffer_size()];
    let (w, rows) = (width as usize, (height as usize).min(HASH_ROWS));
    let mut out = Vec::with_capacity(w * rows * 4);
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    for px in src.chunks_exact(channels).take(w * rows) {
        let [r, g, b, a] = match channels {
            1 => [px[0], px[0], px[0], 255],
            2 => [px[0], px[0], px[0], px[1]],
            3 => [px[0], px[1], px[2], 255],
            _ => [px[0], px[1], px[2], px[3]],
        };
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RGBA の画素から PNG を作る。`level` で圧縮のしかたを変える。
    fn encode(w: u32, h: u32, pixels: &[[u8; 4]], level: png::Compression) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(level);
        let mut writer = enc.write_header().unwrap();
        let data: Vec<u8> = pixels.iter().flatten().copied().collect();
        writer.write_image_data(&data).unwrap();
        writer.finish().unwrap();
        out
    }

    fn pattern(n: usize) -> Vec<[u8; 4]> {
        (0..n)
            .map(|i| {
                [
                    (i % 251) as u8,
                    (i * 7 % 253) as u8,
                    (i * 13 % 255) as u8,
                    255,
                ]
            })
            .collect()
    }

    #[test]
    fn pixel_hash_ignores_reencoding_and_hidden_rgb() {
        let px = pattern(64 * 64);
        let fast = encode(64, 64, &px, png::Compression::Fast);
        let best = encode(64, 64, &px, png::Compression::High);
        assert_ne!(fast, best);
        assert_eq!(pixel_hash(&fast), pixel_hash(&best));

        // 透明な画素の RGB の違いは無視する
        let mut a = px.clone();
        let mut b = px.clone();
        a[5] = [1, 2, 3, 0];
        b[5] = [200, 100, 50, 0];
        assert_eq!(
            pixel_hash(&encode(64, 64, &a, png::Compression::Fast)),
            pixel_hash(&encode(64, 64, &b, png::Compression::Fast))
        );

        // 不透明な画素が 1 つ違えば別物
        let mut c = px.clone();
        c[10][0] ^= 1;
        assert_ne!(
            pixel_hash(&fast),
            pixel_hash(&encode(64, 64, &c, png::Compression::Fast))
        );

        // 64×32 と、同じ画素を上半分に置いた 64×64 は同じ
        let top = pattern(64 * 32);
        let mut tall = top.clone();
        tall.extend(std::iter::repeat_n([0, 0, 0, 0], 64 * 32));
        assert_eq!(
            pixel_hash(&encode(64, 32, &top, png::Compression::Fast)),
            pixel_hash(&encode(64, 64, &tall, png::Compression::Fast))
        );

        // 読めないバイトは生のバイトの SHA-256 になり、落ちない
        let junk = b"not a png at all";
        assert_eq!(pixel_hash(junk), hex(&Sha256::digest(junk)));
        assert_eq!(pixel_hash(b""), hex(&Sha256::digest(b"")));
        let truncated = &fast[..fast.len() / 2];
        assert_eq!(pixel_hash(truncated), hex(&Sha256::digest(truncated)));
    }
}
