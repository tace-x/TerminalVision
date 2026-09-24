//! Lightweight deterministic syntax token classification.

use crate::preview::language::Language;

/// Token classification for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    StringLiteral,
    Comment,
    Number,
    Boolean,
    TypeOrFunction,
    Punctuation,
    Plain,
}

/// A classified slice of a text line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan<'a> {
    pub text: &'a str,
    pub kind: TokenKind,
}

/// Tokenizes a single sanitized line into a sequence of [`StyledSpan`] tokens.
pub fn tokenize_line<'a>(line: &'a str, language: Language) -> Vec<StyledSpan<'a>> {
    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut cursor = 0;

    while cursor < len {
        // 1. Comments: check for line comment prefix
        if let Some(prefix) = language.line_comment_prefix()
            && line[cursor..].starts_with(prefix)
        {
            spans.push(StyledSpan {
                text: &line[cursor..],
                kind: TokenKind::Comment,
            });
            break;
        }

        // HTML/XML comments <!-- ... -->
        if language == Language::HtmlXml && line[cursor..].starts_with("<!--") {
            if let Some(end) = line[cursor..].find("-->") {
                let end_pos = cursor + end + 3;
                spans.push(StyledSpan {
                    text: &line[cursor..end_pos],
                    kind: TokenKind::Comment,
                });
                cursor = end_pos;
                continue;
            } else {
                spans.push(StyledSpan {
                    text: &line[cursor..],
                    kind: TokenKind::Comment,
                });
                break;
            }
        }

        // C/Rust/JS block comments on single line /* ... */
        if line[cursor..].starts_with("/*") {
            if let Some(end) = line[cursor..].find("*/") {
                let end_pos = cursor + end + 2;
                spans.push(StyledSpan {
                    text: &line[cursor..end_pos],
                    kind: TokenKind::Comment,
                });
                cursor = end_pos;
                continue;
            } else {
                spans.push(StyledSpan {
                    text: &line[cursor..],
                    kind: TokenKind::Comment,
                });
                break;
            }
        }

        let ch = bytes[cursor];

        // 2. Whitespace
        if ch.is_ascii_whitespace() {
            let start = cursor;
            while cursor < len && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            spans.push(StyledSpan {
                text: &line[start..cursor],
                kind: TokenKind::Plain,
            });
            continue;
        }

        // 3. String literals ("...", '...', `...`)
        if ch == b'"'
            || ch == b'\''
            || (ch == b'`'
                && matches!(
                    language,
                    Language::JavaScript | Language::TypeScript | Language::Markdown | Language::Go
                ))
        {
            let quote = ch;
            let start = cursor;
            cursor += 1; // skip opening quote
            while cursor < len {
                if bytes[cursor] == b'\\' && cursor + 1 < len {
                    cursor += 2; // skip escaped char
                } else if bytes[cursor] == quote {
                    cursor += 1; // include closing quote
                    break;
                } else {
                    cursor += 1;
                }
            }
            spans.push(StyledSpan {
                text: &line[start..cursor],
                kind: TokenKind::StringLiteral,
            });
            continue;
        }

        // 4. Numbers (e.g. 123, 0xFF, 3.14)
        if ch.is_ascii_digit()
            || (ch == b'-' && cursor + 1 < len && bytes[cursor + 1].is_ascii_digit())
        {
            let start = cursor;
            if ch == b'-' {
                cursor += 1;
            }
            while cursor < len
                && (bytes[cursor].is_ascii_alphanumeric()
                    || bytes[cursor] == b'.'
                    || bytes[cursor] == b'_')
            {
                cursor += 1;
            }
            spans.push(StyledSpan {
                text: &line[start..cursor],
                kind: TokenKind::Number,
            });
            continue;
        }

        // 5. Word identifiers (keywords, booleans, types, functions)
        if is_ident_start(ch) {
            let start = cursor;
            while cursor < len && is_ident_continue(bytes[cursor]) {
                cursor += 1;
            }
            let word = &line[start..cursor];

            // Lookahead for function call '('
            let is_fn = cursor < len && {
                let rest = &line[cursor..];
                rest.trim_start().starts_with('(')
            };

            let kind = if language.is_keyword(word) {
                TokenKind::Keyword
            } else if language.is_boolean_or_null(word) {
                TokenKind::Boolean
            } else if is_fn || is_type_name(word) {
                TokenKind::TypeOrFunction
            } else {
                TokenKind::Plain
            };

            spans.push(StyledSpan { text: word, kind });
            continue;
        }

        // 6. Punctuation / other symbols
        let start = cursor;
        let char_len = line[cursor..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        cursor += char_len;
        spans.push(StyledSpan {
            text: &line[start..cursor],
            kind: TokenKind::Punctuation,
        });
    }

    spans
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn is_type_name(word: &str) -> bool {
    if let Some(first) = word.chars().next() {
        first.is_ascii_uppercase()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::language::Language;

    #[test]
    fn tokenize_rust_function() {
        let line = "pub fn main() -> Result<(), Error> {";
        let tokens = tokenize_line(line, Language::Rust);

        assert_eq!(
            tokens,
            vec![
                StyledSpan {
                    text: "pub",
                    kind: TokenKind::Keyword
                },
                StyledSpan {
                    text: " ",
                    kind: TokenKind::Plain
                },
                StyledSpan {
                    text: "fn",
                    kind: TokenKind::Keyword
                },
                StyledSpan {
                    text: " ",
                    kind: TokenKind::Plain
                },
                StyledSpan {
                    text: "main",
                    kind: TokenKind::TypeOrFunction
                },
                StyledSpan {
                    text: "(",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: ")",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: " ",
                    kind: TokenKind::Plain
                },
                StyledSpan {
                    text: "-",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: ">",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: " ",
                    kind: TokenKind::Plain
                },
                StyledSpan {
                    text: "Result",
                    kind: TokenKind::TypeOrFunction
                },
                StyledSpan {
                    text: "<",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: "(",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: ")",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: ",",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: " ",
                    kind: TokenKind::Plain
                },
                StyledSpan {
                    text: "Error",
                    kind: TokenKind::TypeOrFunction
                },
                StyledSpan {
                    text: ">",
                    kind: TokenKind::Punctuation
                },
                StyledSpan {
                    text: " ",
                    kind: TokenKind::Plain
                },
                StyledSpan {
                    text: "{",
                    kind: TokenKind::Punctuation
                },
            ]
        );
    }

    #[test]
    fn tokenize_comments_and_strings() {
        let line = "let msg = \"Hello\"; // Greeting";
        let tokens = tokenize_line(line, Language::Rust);

        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::StringLiteral && t.text == "\"Hello\"")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Comment && t.text == "// Greeting")
        );
    }

    #[test]
    fn tokenize_numbers_and_booleans() {
        let line = "let active = true; let count = 42;";
        let tokens = tokenize_line(line, Language::Rust);

        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Boolean && t.text == "true")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Number && t.text == "42")
        );
    }

    #[test]
    fn tokenize_python_syntax() {
        let line = "def calculate(x, y=10): # compute";
        let tokens = tokenize_line(line, Language::Python);

        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "def")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::TypeOrFunction && t.text == "calculate")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Number && t.text == "10")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Comment && t.text == "# compute")
        );
    }

    #[test]
    fn tokenize_go_php_ruby_csharp_syntax() {
        // Go
        let go_line = "func Process(items []string) error { `raw` // go";
        let go_tokens = tokenize_line(go_line, Language::Go);
        assert!(
            go_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "func")
        );
        assert!(
            go_tokens
                .iter()
                .any(|t| t.kind == TokenKind::StringLiteral && t.text == "`raw`")
        );
        assert!(
            go_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Comment && t.text == "// go")
        );

        // PHP
        let php_line = "function test($val = null) { // php";
        let php_tokens = tokenize_line(php_line, Language::Php);
        assert!(
            php_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "function")
        );
        assert!(
            php_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Boolean && t.text == "null")
        );

        // Ruby
        let rb_line = "def process(items) # ruby";
        let rb_tokens = tokenize_line(rb_line, Language::Ruby);
        assert!(
            rb_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "def")
        );
        assert!(
            rb_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Comment && t.text == "# ruby")
        );

        // C#
        let cs_line = "public class Item { bool active = true; }";
        let cs_tokens = tokenize_line(cs_line, Language::CSharp);
        assert!(
            cs_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "public")
        );
        assert!(
            cs_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword && t.text == "class")
        );
        assert!(
            cs_tokens
                .iter()
                .any(|t| t.kind == TokenKind::Boolean && t.text == "true")
        );
    }
}
