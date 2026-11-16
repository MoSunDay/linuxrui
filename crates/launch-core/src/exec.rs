//! Exec-line tokenizer: whitespace splitting with double quoting and
//! backslash escaping. Field codes are captured as [`Piece::Code`] for the
//! expansion stage. This never invokes a shell.

/// One fragment of a token: literal text or a field code (`%c` etc.).
/// `%%` is folded into a literal `%` here, so codes are always single chars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Literal(String),
    Code(char),
}

/// A token is a sequence of pieces; expansions decide how many argv entries
/// it finally produces (e.g. standalone `%i` -> `--icon <name>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecToken(pub Vec<Piece>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecTokenizeError {
    UnterminatedQuote,
    DanglingEscape,
}

impl std::fmt::Display for ExecTokenizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecTokenizeError::UnterminatedQuote => write!(f, "unterminated double quote"),
            ExecTokenizeError::DanglingEscape => write!(f, "dangling backslash"),
        }
    }
}

impl std::error::Error for ExecTokenizeError {}

/// Tokenize an Exec value per the Desktop Entry Specification:
/// - arguments are separated by unquoted space/tab/newline;
/// - `"` toggles quoting (removed) and may wrap whole or partial arguments;
/// - `\` escapes the next character (both inside and outside quotes);
/// - `%x` becomes a code piece, `%%` a literal percent.
pub fn tokenize(exec: &str) -> Result<Vec<ExecToken>, ExecTokenizeError> {
    let mut tokens: Vec<ExecToken> = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut literal = String::new();
    let mut in_quotes = false;
    let mut chars = exec.chars().peekable();

    fn end_token(tokens: &mut Vec<ExecToken>, pieces: &mut Vec<Piece>, literal: &mut String) {
        if !literal.is_empty() {
            pieces.push(Piece::Literal(std::mem::take(literal)));
        }
        if !pieces.is_empty() {
            tokens.push(ExecToken(std::mem::take(pieces)));
        }
    }

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
            }
            '\\' => match chars.next() {
                Some(escaped) => literal.push(escaped),
                None => return Err(ExecTokenizeError::DanglingEscape),
            },
            ' ' | '\t' | '\n' if !in_quotes => {
                end_token(&mut tokens, &mut pieces, &mut literal);
            }
            '%' => match chars.peek().copied() {
                Some('%') => {
                    chars.next();
                    literal.push('%');
                }
                Some(code) => {
                    chars.next();
                    if !literal.is_empty() {
                        pieces.push(Piece::Literal(std::mem::take(&mut literal)));
                    }
                    pieces.push(Piece::Code(code));
                }
                // A trailing lone `%` is emitted as a code so the expansion
                // stage reports it as unknown instead of silently dropping it.
                None => pieces.push(Piece::Code('%')),
            },
            other => literal.push(other),
        }
    }
    if in_quotes {
        return Err(ExecTokenizeError::UnterminatedQuote);
    }
    end_token(&mut tokens, &mut pieces, &mut literal);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal_tokens(exec: &str) -> Vec<String> {
        // Helper: plain argv for inputs without field codes.
        tokenize(exec)
            .expect("tokenize")
            .into_iter()
            .map(|ExecToken(pieces)| {
                pieces
                    .into_iter()
                    .map(|p| match p {
                        Piece::Literal(s) => s,
                        Piece::Code(c) => panic!("unexpected code {c}"),
                    })
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(literal_tokens("foo bar\tbaz"), ["foo", "bar", "baz"]);
    }

    #[test]
    fn double_quotes_group_and_can_be_partial() {
        assert_eq!(literal_tokens("e \"a b\" c"), ["e", "a b", "c"]);
        assert_eq!(literal_tokens("e --o=\"v v\""), ["e", "--o=v v"]);
    }

    #[test]
    fn backslash_escapes_any_char() {
        assert_eq!(
            literal_tokens("e a\\ b \\\"x\\\" \\\\"),
            ["e", "a b", "\"x\"", "\\"]
        );
    }

    #[test]
    fn percent_percent_is_literal_percent() {
        assert_eq!(literal_tokens("e 100%% done"), ["e", "100%", "done"]);
    }

    #[test]
    fn field_codes_become_pieces() {
        let tokens = tokenize("e %f %i %c").expect("tokenize");
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[1].0, [Piece::Code('f')]);
        assert_eq!(tokens[2].0, [Piece::Code('i')]);
        assert_eq!(tokens[3].0, [Piece::Code('c')]);
        assert_eq!(tokenize("e --file=%f").expect("tokenize")[1].0.len(), 2);
    }

    #[test]
    fn unterminated_quote_and_dangling_escape_are_errors() {
        assert_eq!(
            tokenize("e \"oops").unwrap_err(),
            ExecTokenizeError::UnterminatedQuote
        );
        assert_eq!(
            tokenize("e bad\\").unwrap_err(),
            ExecTokenizeError::DanglingEscape
        );
    }
}
