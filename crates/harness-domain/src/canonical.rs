//! Compatibility codecs. Hash, view and recovery-key encodings are deliberately distinct.
use serde_json::Value;
use std::fmt::Write;

pub fn encoded(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    render(value, 0, true, false, true, &mut out);
    out.push('\n');
    out.into_bytes()
}
pub fn ascii_pretty(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    render(value, 0, true, true, false, &mut out);
    out.push('\n');
    out.into_bytes()
}
pub fn ascii_compact_sorted(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    render(value, 0, false, true, true, &mut out);
    out.into_bytes()
}
fn quoted(text: &str, ascii: bool) -> String {
    let escaped = serde_json::to_string(text).expect("A UTF-8 string always serializes");
    if !ascii {
        return escaped;
    }
    let mut out = String::new();
    for c in escaped.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                write!(&mut out, "\\u{unit:04x}").expect("writing to string");
            }
        }
    }
    out
}
fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth * 2 {
        out.push(' ');
    }
}
fn render(value: &Value, depth: usize, pretty: bool, ascii: bool, sort: bool, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::String(v) => out.push_str(&quoted(v, ascii)),
        Value::Number(v) => out.push_str(&if v.is_f64() {
            python_float(&v.to_string())
        } else {
            v.to_string()
        }),
        Value::Array(values) => {
            out.push('[');
            if !values.is_empty() {
                if pretty {
                    out.push('\n');
                }
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        out.push(if pretty { '\n' } else { ' ' });
                    }
                    if pretty {
                        indent(out, depth + 1);
                    }
                    render(value, depth + 1, pretty, ascii, sort, out);
                }
                if pretty {
                    out.push('\n');
                    indent(out, depth);
                }
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            if !values.is_empty() {
                let mut keys: Vec<_> = values.keys().collect();
                if sort {
                    keys.sort();
                }
                if pretty {
                    out.push('\n');
                }
                for (i, key) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                        out.push(if pretty { '\n' } else { ' ' });
                    }
                    if pretty {
                        indent(out, depth + 1);
                    }
                    out.push_str(&quoted(key, ascii));
                    out.push_str(": ");
                    render(&values[*key], depth + 1, pretty, ascii, sort, out);
                }
                if pretty {
                    out.push('\n');
                    indent(out, depth);
                }
            }
            out.push('}');
        }
    }
}
/// CPython repr uses a scientific exponent below -4 or at least 16, a two-digit
/// signed exponent, and keeps .0 for integral floats in fixed notation.
fn python_float(raw: &str) -> String {
    let negative = raw.starts_with('-');
    let raw = raw.trim_start_matches('-');
    let (mantissa, exp) = raw
        .split_once(['e', 'E'])
        .map(|(m, e)| (m, e.parse::<i32>().unwrap_or(0)))
        .unwrap_or((raw, 0));
    let decimal = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let all = mantissa.replace('.', "");
    let leading = all.bytes().take_while(|b| *b == b'0').count();
    let sign = if negative { "-" } else { "" };
    if leading == all.len() {
        return format!("{sign}0.0");
    }
    let exponent = exp + decimal - leading as i32 - 1;
    let digits = all[leading..].trim_end_matches('0');
    if !(-4..16).contains(&exponent) {
        let tail = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        return format!(
            "{sign}{}{tail}e{}{:02}",
            &digits[..1],
            if exponent >= 0 { "+" } else { "-" },
            exponent.abs()
        );
    }
    let point = exponent + 1;
    let fixed = if point <= 0 {
        format!("0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{}{}.0", digits, "0".repeat(point as usize - digits.len()))
    } else {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    };
    format!("{sign}{fixed}")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn python_generated_golden_codecs_are_byte_identical() {
        let vectors: Value =
            serde_json::from_str(include_str!("../tests/canonical-vectors.json")).unwrap();
        for (i, row) in vectors.as_array().unwrap().iter().enumerate() {
            let value = &row["value"];
            assert_eq!(
                String::from_utf8(encoded(value)).unwrap(),
                row["encoded"].as_str().unwrap(),
                "UTF-8 vector {i}: {value}"
            );
            assert_eq!(
                String::from_utf8(ascii_pretty(value)).unwrap(),
                row["ascii_pretty"].as_str().unwrap(),
                "ASCII vector {i}: {value}"
            );
            assert_eq!(
                String::from_utf8(ascii_compact_sorted(value)).unwrap(),
                row["ascii_compact_sorted"].as_str().unwrap(),
                "recovery vector {i}: {value}"
            );
        }
    }
}
