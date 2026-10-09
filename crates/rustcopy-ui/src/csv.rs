//! A CSV the history can be exported to (CATALOGO_COMPORTAMENTI_GUI.md L24). Same rules as the Tauri
//! console's `csv.js`: RFC 4180 quoting with CRLF line ends, a UTF-8 byte-order mark so a spreadsheet
//! reads accented text, and a leading apostrophe on a field that starts like a formula.

/// Characters that make a spreadsheet read a field as a formula.
const FORMULA_TRIGGERS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

fn field(value: &str) -> String {
    let neutral = if value.starts_with(FORMULA_TRIGGERS) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if neutral.contains(['"', ',', '\r', '\n']) {
        format!("\"{}\"", neutral.replace('"', "\"\""))
    } else {
        neutral
    }
}

/// `headers` and `rows` as CSV text, with the byte-order mark in front.
pub fn to_csv(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(rows.len() + 1);
    lines.push(
        headers
            .iter()
            .map(|h| field(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    for row in rows {
        lines.push(row.iter().map(|v| field(v)).collect::<Vec<_>>().join(","));
    }
    format!("\u{feff}{}", lines.join("\r\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_with_commas_quotes_or_newlines_are_quoted() {
        let csv = to_csv(&["a", "b"], &[vec!["x,y".into(), "say \"hi\"".into()]]);
        assert_eq!(csv, "\u{feff}a,b\r\n\"x,y\",\"say \"\"hi\"\"\"");
    }

    #[test]
    fn a_field_that_starts_like_a_formula_is_neutralised() {
        let csv = to_csv(
            &["h"],
            &[vec!["=HYPERLINK(\"x\")".into()], vec!["+1".into()]],
        );
        assert!(csv.contains("\"'=HYPERLINK(\"\"x\"\")\""), "{csv}");
        assert!(csv.contains("\r\n'+1"), "{csv}");
    }

    #[test]
    fn plain_text_and_accents_are_left_alone() {
        let csv = to_csv(&["Esito"], &[vec!["Riuscita è bene".into()]]);
        assert_eq!(csv, "\u{feff}Esito\r\nRiuscita è bene");
    }
}
