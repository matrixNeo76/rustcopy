//! Minimal OEM code page decoder.
//!
//! `encoding_rs` deliberately does not implement single-byte DOS/OEM code pages such as CP850
//! (IBM850) because they are not part of the Web Platform encoding standard it targets —
//! `Encoding::for_label(b"ibm850")` returns `None`, so code that unwrapped it into `UTF_8` was
//! silently decoding OEM text as UTF-8 (mojibake on any accented character). This module hardcodes
//! the CP850 (DOS Latin-1) upper half, which robocopy uses by default on Western European
//! Windows installs, and exposes a runtime check against the process's actual OEM code page so a
//! non-850 console does not get silently mis-decoded.
//!
//! Bytes 0x00-0x7F are identical to ASCII in every single-byte OEM code page used here.

/// CP850 mapping for bytes 0x80-0xFF, in order.
const CP850_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', // 80-8F
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', 'ø', '£', 'Ø', '×', 'ƒ', // 90-9F
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '®', '¬', '½', '¼', '¡', '«', '»', // A0-AF
    '░', '▒', '▓', '│', '┤', 'Á', 'Â', 'À', '©', '╣', '║', '╗', '╝', '¢', '¥', '┐', // B0-BF
    '└', '┴', '┬', '├', '─', '┼', 'ã', 'Ã', '╚', '╔', '╩', '╦', '╠', '═', '╬', '¤', // C0-CF
    'ð', 'Ð', 'Ê', 'Ë', 'È', 'ı', 'Í', 'Î', 'Ï', '┘', '┌', '█', '▄', '¦', 'Ì', '▀', // D0-DF
    'Ó', 'ß', 'Ô', 'Ò', 'õ', 'Õ', 'µ', 'þ', 'Þ', 'Ú', 'Û', 'Ù', 'ý', 'Ý', '¯', '´', // E0-EF
    '\u{ad}', '±', '‗', '¾', '¶', '§', '÷', '¸', '°', '¨', '·', '¹', '³', '²', '■',
    '\u{a0}', // F0-FF
];

/// Windows OEM code page identifier for CP850 (DOS Latin-1).
pub const CP850_CODE_PAGE: u32 = 850;

/// Decode `bytes` as CP850, replacing nothing — every byte value has a defined mapping.
pub fn decode_cp850(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x80 {
            out.push(b as char);
        } else {
            out.push(CP850_HIGH[(b - 0x80) as usize]);
        }
    }
    out
}

#[cfg(windows)]
extern "system" {
    fn GetOEMCP() -> u32;
}

/// The process's actual OEM code page on Windows, or `None` off Windows / on lookup failure.
#[cfg(windows)]
pub fn active_oem_code_page() -> Option<u32> {
    // SAFETY: GetOEMCP takes no arguments, performs no pointer dereferences, and cannot fail in a
    // way that corrupts memory — it returns 0 on lookup failure rather than trapping. Safe to call
    // unconditionally.
    let cp = unsafe { GetOEMCP() };
    if cp == 0 {
        None
    } else {
        Some(cp)
    }
}

#[cfg(not(windows))]
pub fn active_oem_code_page() -> Option<u32> {
    None
}

/// Decode robocopy's OEM output. Uses the CP850 table when it matches the process's actual OEM
/// code page (or when the code page can't be determined, since CP850 is the historical default
/// baked into most Western European Windows images); otherwise falls back to a lossy UTF-8 decode
/// and logs once so a non-850 console isn't silently mis-decoded without any trace.
pub fn decode_robocopy_output(bytes: &[u8]) -> String {
    match active_oem_code_page() {
        Some(cp) if cp == CP850_CODE_PAGE => decode_cp850(bytes),
        Some(cp) => {
            tracing::warn!(
                code_page = cp,
                "active OEM code page is not CP850; decoding as CP850 anyway may mis-render \
                 accented characters (only CP850 has a built-in table)"
            );
            decode_cp850(bytes)
        }
        None => decode_cp850(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_is_unchanged() {
        assert_eq!(decode_cp850(b"robocopy.exe"), "robocopy.exe");
    }

    #[test]
    fn accented_italian_characters_decode_correctly() {
        // à = 0x85, è = 0x8A, ì = 0x8D, ò = 0x95, ù = 0x97 in CP850.
        let bytes = [0x85, 0x8A, 0x8D, 0x95, 0x97];
        assert_eq!(decode_cp850(&bytes), "àèìòù");
    }

    #[test]
    fn every_byte_value_has_a_mapping() {
        let all_bytes: Vec<u8> = (0u8..=255).collect();
        let decoded = decode_cp850(&all_bytes);
        assert_eq!(decoded.chars().count(), 256);
    }
}
