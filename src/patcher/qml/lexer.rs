use super::super::{PatchError, unsupported};
#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) line: usize,
}
pub(super) fn lex(
    source: &str,
    file: &str,
) -> Result<(Vec<Token>, Vec<Option<usize>>, Vec<Option<usize>>), PatchError> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut line = 1;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                line += 1;
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                let mut closed = false;
                while i + 1 < bytes.len() {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        closed = true;
                        break;
                    }
                    i += 1;
                }
                if !closed {
                    return Err(unsupported(file, "unterminated block comment"));
                }
            }
            quote @ (b'\'' | b'"' | b'`') => {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        closed = true;
                        break;
                    } else {
                        if bytes[i] == b'\n' {
                            line += 1;
                        }
                        i += 1;
                    }
                }
                if !closed {
                    return Err(unsupported(file, "unterminated string literal"));
                }
                tokens.push(Token {
                    start,
                    end: i,
                    line,
                });
            }
            b if is_ident_start(b) => {
                let start = i;
                i += 1;
                while i < bytes.len() && is_ident_continue(bytes[i]) {
                    i += 1;
                }
                tokens.push(Token {
                    start,
                    end: i,
                    line,
                });
            }
            b if b.is_ascii_digit() => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
                {
                    i += 1;
                }
                tokens.push(Token {
                    start,
                    end: i,
                    line,
                });
            }
            _ => {
                let start = i;
                i += 1;
                tokens.push(Token {
                    start,
                    end: i,
                    line,
                });
            }
        }
    }

    let mut matching = vec![None; tokens.len()];
    let mut container = vec![None; tokens.len()];
    let mut stack: Vec<(usize, u8)> = Vec::new();
    for index in 0..tokens.len() {
        let text = &source[tokens[index].start..tokens[index].end];
        let byte = text.as_bytes()[0];
        if is_close(byte) {
            let Some((open, open_byte)) = stack.pop() else {
                return Err(unsupported(file, "unbalanced delimiters"));
            };
            if !same_delimiter(open_byte, byte) {
                return Err(unsupported(file, "mismatched delimiters"));
            }
            container[index] = Some(open);
            matching[index] = Some(open);
            matching[open] = Some(index);
        } else {
            container[index] = stack.last().map(|(open, _)| *open);
            if is_open(byte) {
                stack.push((index, byte));
            }
        }
    }
    if !stack.is_empty() {
        return Err(unsupported(file, "unbalanced delimiters"));
    }
    Ok((tokens, matching, container))
}
fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}
fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}
fn is_open(byte: u8) -> bool {
    matches!(byte, b'{' | b'(' | b'[')
}
fn is_close(byte: u8) -> bool {
    matches!(byte, b'}' | b')' | b']')
}
fn same_delimiter(open: u8, close: u8) -> bool {
    matches!((open, close), (b'{', b'}') | (b'(', b')') | (b'[', b']'))
}
pub(super) fn lex_fragment(fragment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let bytes = fragment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                    } else if bytes[i] == quote {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
            b if is_ident_start(b) => {
                i += 1;
                while i < bytes.len() && is_ident_continue(bytes[i]) {
                    i += 1;
                }
            }
            b if b.is_ascii_digit() => {
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
                {
                    i += 1;
                }
            }
            _ => i += 1,
        }
        tokens.push(fragment[start..i].to_owned());
    }
    tokens
}
