//! Small bounded RTF text reader: no external programs, links or embedded objects are opened.
#[derive(Clone, Copy)]
struct State {
    skip: bool,
    uc: usize,
}
pub fn visible_text(source: &str) -> String {
    if !source.trim_start().starts_with("{\\rtf") {
        return source.into();
    }
    let b = source.as_bytes();
    let mut i = 0;
    let mut state = State { skip: false, uc: 1 };
    let mut stack = Vec::new();
    let mut out: Vec<u16> = Vec::new();
    let mut fallback = 0usize;
    let mut encoding = encoding_rs::WINDOWS_1252;
    while i < b.len() {
        match b[i] {
            b'{' => {
                if stack.len() >= 256 {
                    break;
                }
                stack.push(state);
                i += 1;
            }
            b'}' => {
                state = stack.pop().unwrap_or(State { skip: false, uc: 1 });
                i += 1;
            }
            b'\r' | b'\n' => i += 1,
            b'\\' => {
                i += 1;
                if i >= b.len() {
                    break;
                }
                if b[i] == b'*' {
                    state.skip = true;
                    i += 1;
                    continue;
                }
                if b[i] == b'\'' && i + 2 < b.len() {
                    let mut bytes = vec![];
                    while i + 2 < b.len() && b[i] == b'\'' {
                        if let Ok(v) = u8::from_str_radix(
                            std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""),
                            16,
                        ) {
                            bytes.push(v);
                        }
                        i += 3;
                        if i + 1 < b.len() && b[i] == b'\\' && b[i + 1] == b'\'' {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    if fallback > 0 {
                        let n = fallback.min(bytes.len());
                        fallback -= n;
                        bytes.drain(..n);
                    }
                    if !state.skip {
                        out.extend(encoding.decode(&bytes).0.encode_utf16());
                    }
                    continue;
                }
                if !b[i].is_ascii_alphabetic() {
                    let c = match b[i] {
                        b'\\' => Some('\\'),
                        b'{' => Some('{'),
                        b'}' => Some('}'),
                        b'~' => Some(' '),
                        b'_' => Some('-'),
                        _ => None,
                    };
                    if fallback > 0 {
                        fallback -= 1;
                    } else if !state.skip {
                        if let Some(c) = c {
                            out.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
                        }
                    }
                    i += 1;
                    continue;
                }
                let start = i;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &source[start..i];
                let start = i;
                if i < b.len() && b[i] == b'-' {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let n = source[start..i].parse::<i32>().ok();
                if i < b.len() && b[i] == b' ' {
                    i += 1;
                }
                match word {
                    "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                    | "header" | "footer" | "filetbl" | "datastore" | "themedata" | "fldinst" => {
                        state.skip = true
                    }
                    "bin" => {
                        i = i
                            .saturating_add(n.unwrap_or(0).max(0) as usize)
                            .min(b.len())
                    }
                    "ansicpg" => {
                        let label = format!("windows-{}", n.unwrap_or(1252));
                        encoding = encoding_rs::Encoding::for_label(label.as_bytes())
                            .unwrap_or(encoding_rs::WINDOWS_1252);
                    }
                    "uc" => state.uc = n.unwrap_or(1).clamp(0, 16) as usize,
                    "u" => {
                        if !state.skip {
                            out.push(n.unwrap_or(0) as u16);
                        }
                        fallback = state.uc;
                    }
                    "par" | "line" if !state.skip => out.push(b'\n' as u16),
                    "tab" if !state.skip => out.push(b'\t' as u16),
                    "emdash" if !state.skip => out.push(0x2014),
                    "endash" if !state.skip => out.push(0x2013),
                    "bullet" if !state.skip => out.push(0x2022),
                    _ => {}
                }
            }
            _ => {
                let Some(rest) = source.get(i..) else {
                    break;
                };
                let c = rest.chars().next().unwrap();
                i += c.len_utf8();
                if fallback > 0 {
                    fallback -= 1;
                } else if !state.skip {
                    out.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
                }
            }
        }
    }
    String::from_utf16_lossy(&out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_unicode_text_without_tables_or_hidden_payloads() {
        let s = r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}{\*\generator hidden;}\uc1\u21518?\u21049?\u36710? cable\par replace {\b inner wire}{\pict garbage}}";
        assert_eq!(visible_text(s), "后刹车 cable\nreplace inner wire");
    }
    #[test]
    fn decodes_escapes_fields_surrogates_and_binary() {
        assert_eq!(
            visible_text(
                r"{\rtf1 caf\'e9 \{x\} {\field{\*\fldinst HYPERLINK secret}{\fldrslt label}}\u-10179?\u-8704?{\pict\bin3 abc}}"
            ),
            "café {x} label😀"
        );
    }
    #[test]
    fn plain_text_is_untouched_and_truncated_rtf_is_safe() {
        assert_eq!(visible_text("plain {text}"), "plain {text}");
        assert_eq!(visible_text(r"{\rtf1 hello\"), "hello");
    }
}
