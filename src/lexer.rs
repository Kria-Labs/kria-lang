use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Set,
    If,
    Else,
    ElseIf,
    While,
    For,
    In,
    Fn,
    Return,
    True,
    False,
    Null,
    Print,
    Break,
    Continue,
    Import,
    Export,
    From,

    // Identifiers and Literals
    Identifier(String),
    Number(i64),
    Float(f64),
    InvalidNumber(String),
    String(String),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Equal,
    EqualEqual,
    NotEqual,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    And,
    Or,
    Not,
    Pipe,

    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Hash,
    Dot,
    Comma,
    Colon,
    Newline,

    // Special
    Eof,
}

pub struct Lexer<'a> {
    input: Peekable<Chars<'a>>,
    current: Option<char>,
    peeked: Option<char>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut chars = input.chars().peekable();
        let current = chars.next();
        let peeked = chars.peek().copied();
        Lexer {
            input: chars,
            current,
            peeked,
        }
    }

    fn current_char(&self) -> Option<char> {
        self.current
    }

    fn peek_char(&self) -> Option<char> {
        self.peeked
    }

    fn advance(&mut self) {
        self.current = self.input.next();
        self.peeked = self.input.peek().copied();
    }

    fn skip_spaces(&mut self) {
        while let Some(ch) = self.current_char() {
            if ch.is_whitespace() && ch != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn push_escape_char(&mut self, result: &mut String) {
        self.advance();
        if let Some(escaped) = self.current_char() {
            match escaped {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                _ => {
                    result.push('\\');
                    result.push(escaped);
                }
            }
            self.advance();
        }
    }

    fn read_string(&mut self) -> String {
        self.advance(); // Skip opening quote
        let mut result = String::new();

        while let Some(ch) = self.current_char() {
            if ch == '"' {
                self.advance();
                break;
            } else if ch == '\\' {
                self.push_escape_char(&mut result);
            } else {
                result.push(ch);
                self.advance();
            }
        }

        result
    }

    fn read_triple_string(&mut self) -> String {
        let mut result = String::new();

        while self.current_char().is_some() {
            if self.current_char() == Some('"') && self.peek_char() == Some('"') {
                self.advance();
                self.advance();
                if self.current_char() == Some('"') {
                    self.advance();
                    break;
                }
                result.push('"');
                result.push('"');
                continue;
            }

            if self.current_char() == Some('\\') {
                self.push_escape_char(&mut result);
                continue;
            }

            let ch = self.current_char().unwrap();
            self.advance();
            result.push(ch);
        }

        result
    }

    fn skip_block_comment(&mut self) {
        self.advance(); // /
        self.advance(); // *
        while let Some(ch) = self.current_char() {
            if ch == '*' && self.peek_char() == Some('/') {
                self.advance();
                self.advance();
                break;
            }
            self.advance();
        }
    }

    fn read_number(&mut self) -> Token {
        let mut result = String::new();
        let mut is_float = false;

        while let Some(ch) = self.current_char() {
            if ch.is_ascii_digit() {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        if self.current_char() == Some('.')
            && self.peek_char().map_or(true, |ch| {
                (!ch.is_alphabetic() && ch != '_') || matches!(ch, 'e' | 'E')
            })
        {
            is_float = true;
            result.push('.');
            self.advance();
            while let Some(ch) = self.current_char() {
                if ch.is_ascii_digit() {
                    result.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        is_float |= self.read_exponent_part(&mut result);

        if is_float {
            match result.parse::<f64>() {
                Ok(value) if value.is_finite() => Token::Float(value),
                _ => Token::InvalidNumber(result),
            }
        } else {
            match result.parse::<i64>() {
                Ok(value) => Token::Number(value),
                Err(_) => Token::InvalidNumber(result),
            }
        }
    }

    fn read_exponent_part(&mut self, result: &mut String) -> bool {
        if !matches!(self.current_char(), Some('e' | 'E')) {
            return false;
        }

        let mut lookahead = self.input.clone();
        let next = lookahead.next();
        let exponent_has_digits = match next {
            Some('+' | '-') => lookahead.next().map_or(false, |ch| ch.is_ascii_digit()),
            Some(ch) => ch.is_ascii_digit(),
            None => false,
        };
        if !exponent_has_digits {
            return false;
        }

        result.push(self.current_char().unwrap());
        self.advance();
        if matches!(self.current_char(), Some('+' | '-')) {
            result.push(self.current_char().unwrap());
            self.advance();
        }
        while let Some(ch) = self.current_char() {
            if ch.is_ascii_digit() {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        true
    }

    fn read_leading_decimal(&mut self) -> Token {
        let mut result = String::from("0.");
        self.advance();
        while let Some(ch) = self.current_char() {
            if ch.is_ascii_digit() {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        self.read_exponent_part(&mut result);
        match result.parse::<f64>() {
            Ok(value) if value.is_finite() => Token::Float(value),
            _ => Token::InvalidNumber(result),
        }
    }

    fn read_identifier(&mut self) -> String {
        let mut result = String::new();

        while let Some(ch) = self.current_char() {
            if ch.is_alphanumeric() || ch == '_' {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        result
    }

    pub fn next_token(&mut self) -> Token {
        loop {
            self.skip_spaces();

            match self.current_char() {
                None => return Token::Eof,
                Some(ch) => match ch {
                    '+' => {
                        self.advance();
                        return Token::Plus;
                    }
                    '-' => {
                        self.advance();
                        return Token::Minus;
                    }
                    '*' => {
                        self.advance();
                        return Token::Star;
                    }
                    '/' => {
                        if self.peek_char() == Some('/') {
                            self.advance();
                            self.advance();
                            while let Some(ch) = self.current_char() {
                                if ch == '\n' {
                                    break;
                                }
                                self.advance();
                            }
                            continue;
                        } else if self.peek_char() == Some('*') {
                            self.skip_block_comment();
                            continue;
                        } else {
                            self.advance();
                            return Token::Slash;
                        }
                    }
                    '(' => {
                        self.advance();
                        return Token::LParen;
                    }
                    ')' => {
                        self.advance();
                        return Token::RParen;
                    }
                    '=' => {
                        if self.peek_char() == Some('=') {
                            self.advance();
                            self.advance();
                            return Token::EqualEqual;
                        } else {
                            self.advance();
                            return Token::Equal;
                        }
                    }
                    '!' => {
                        self.advance();
                        if self.current_char() == Some('=') {
                            self.advance();
                            return Token::NotEqual;
                        } else {
                            return Token::Not;
                        }
                    }
                    '>' => {
                        self.advance();
                        if self.current_char() == Some('=') {
                            self.advance();
                            return Token::GreaterEqual;
                        } else {
                            return Token::Greater;
                        }
                    }
                    '<' => {
                        self.advance();
                        if self.current_char() == Some('=') {
                            self.advance();
                            return Token::LessEqual;
                        } else {
                            return Token::Less;
                        }
                    }
                    '{' => {
                        self.advance();
                        return Token::LBrace;
                    }
                    '}' => {
                        self.advance();
                        return Token::RBrace;
                    }
                    '[' => {
                        self.advance();
                        return Token::LBracket;
                    }
                    ']' => {
                        self.advance();
                        return Token::RBracket;
                    }
                    '#' => {
                        self.advance();
                        return Token::Hash;
                    }
                    '.' => {
                        if self.peek_char().map_or(false, |next| next.is_ascii_digit()) {
                            return self.read_leading_decimal();
                        }
                        self.advance();
                        return Token::Dot;
                    }
                    ',' => {
                        self.advance();
                        return Token::Comma;
                    }
                    ':' => {
                        self.advance();
                        return Token::Colon;
                    }
                    '|' => {
                        self.advance();
                        return Token::Pipe;
                    }
                    '\n' => {
                        self.advance();
                        return Token::Newline;
                    }
                    '"' => {
                        if self.peek_char() == Some('"') {
                            self.advance();
                            if self.current_char() == Some('"') {
                                self.advance();
                                if self.current_char() == Some('"') {
                                    self.advance();
                                    return Token::String(self.read_triple_string());
                                }
                                return Token::String(String::new());
                            }
                        }
                        return Token::String(self.read_string());
                    }
                    _ if ch.is_ascii_digit() => return self.read_number(),
                    _ if ch.is_alphabetic() || ch == '_' => {
                        let ident = self.read_identifier();
                        return match ident.as_str() {
                            "set" => Token::Set,
                            "if" => Token::If,
                            "else" => Token::Else,
                            "elseif" => Token::ElseIf,
                            "while" => Token::While,
                            "for" => Token::For,
                            "in" => Token::In,
                            "fn" => Token::Fn,
                            "return" => Token::Return,
                            "true" => Token::True,
                            "false" => Token::False,
                            "null" => Token::Null,
                            "print" => Token::Print,
                            "and" => Token::And,
                            "or" => Token::Or,
                            "not" => Token::Not,
                            "break" => Token::Break,
                            "continue" => Token::Continue,
                            "import" => Token::Import,
                            "export" => Token::Export,
                            "from" => Token::From,
                            _ => Token::Identifier(ident),
                        };
                    }
                    _ => {
                        self.advance();
                        continue;
                    }
                },
            }
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token();
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }

        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_comment_is_skipped() {
        let mut lexer = Lexer::new("set x = 1 /* hidden */ + 2");
        let tokens = lexer.tokenize();
        assert!(tokens.iter().any(|t| matches!(t, Token::Number(2))));
        assert!(!format!("{:?}", tokens).contains("hidden"));
    }

    fn first_string_token(tokens: &[Token]) -> &str {
        match tokens.iter().find(|t| matches!(t, Token::String(_))) {
            Some(Token::String(s)) => s,
            _ => panic!("no string token in {:?}", tokens),
        }
    }

    #[test]
    fn triple_quoted_string_preserves_newlines() {
        let mut lexer = Lexer::new(r#"print("""a\nhello""")"#);
        let tokens = lexer.tokenize();
        assert_eq!(first_string_token(&tokens), "a\nhello");
    }

    #[test]
    fn multiline_triple_string_literal() {
        let source = "print(\"\"\"a\nhello\nworld\"\"\")";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize();
        assert_eq!(first_string_token(&tokens), "a\nhello\nworld");
    }

    #[test]
    fn decimal_and_scientific_literals_are_tokenized() {
        let mut lexer = Lexer::new("1.25 .5 2e3 4.5E-2 .5e2 1. 1.e2 6");
        let tokens = lexer.tokenize();
        assert!(tokens.contains(&Token::Float(1.25)));
        assert!(tokens.contains(&Token::Float(0.5)));
        assert!(tokens.contains(&Token::Float(2000.0)));
        assert!(tokens.contains(&Token::Float(0.045)));
        assert!(tokens.contains(&Token::Float(50.0)));
        assert!(tokens.contains(&Token::Float(1.0)));
        assert!(tokens.contains(&Token::Float(100.0)));
        assert!(tokens.contains(&Token::Number(6)));
    }

    #[test]
    fn out_of_range_numeric_literals_are_not_silently_changed() {
        let mut lexer = Lexer::new("9223372036854775808 1e999");
        let tokens = lexer.tokenize();
        assert!(tokens.contains(&Token::InvalidNumber("9223372036854775808".to_string())));
        assert!(tokens.contains(&Token::InvalidNumber("1e999".to_string())));
    }
}
