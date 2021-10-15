use crate::artefact::tokens::{Delimiter, Operator, Side, Token, Tokens};
use std::{fmt::Display, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorPosition {
    file_path: Option<String>,
    code_row: String,
    row: usize,
    column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnexpectedEOF(ErrorPosition, &'static str),
    InvalidIdentifier(ErrorPosition, &'static str),
    InvalidCharacter(ErrorPosition, char),

    InvalidLitFloat(ErrorPosition, <f64 as FromStr>::Err),
    InvalidLitInt(ErrorPosition, <isize as FromStr>::Err),
    InvalidLitChar(ErrorPosition, &'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Display for ErrorPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let file_path = match self.file_path.as_ref() {
            Some(file_path) => &file_path,
            None => "<stdin>",
        };
        write!(
            f,
            "  at {}:{}:{}\n\n  {}\n  {}^ ",
            file_path,
            self.row,
            self.column,
            self.code_row,
            " ".repeat(self.column)
        )
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEOF(pos, err) => write!(f, "Unexpected end of file\n{}{}", pos, err),
            Self::InvalidIdentifier(pos, err) => write!(f, "Invalid identifier\n{}{}", pos, err),
            Self::InvalidCharacter(pos, c) => write!(f, "Invalid character {}\n{}", c, pos),
            Self::InvalidLitFloat(pos, err) => write!(f, "Invalid float literal: {}\n{}", err, pos),
            Self::InvalidLitInt(pos, err) => write!(f, "Invalid int literal: {}\n{}", err, pos),
            Self::InvalidLitChar(pos, err) => write!(f, "Invalid char literal: {}\n{}", err, pos),
        }
    }
}

struct LexerPosition {
    index: usize,

    row: usize,
    row_index: usize,
    column: usize,
}

impl LexerPosition {
    fn new() -> Self {
        Self {
            index: 0,
            row: 0,
            row_index: 0,
            column: 0,
        }
    }

    fn new_line(&mut self) {
        self.index += 1;
        self.row += 1;
        self.row_index = self.index;
        self.column = 0;
    }

    fn advance(&mut self, n: usize) {
        self.index += n;
        self.column += n;
    }
}

struct Lexer {
    position: LexerPosition,
    code: Vec<char>,
    tokens: Tokens,
}

impl Lexer {
    fn new(code: &str) -> Self {
        Self {
            position: LexerPosition::new(),
            code: code.chars().collect(),
            tokens: Tokens::new(),
        }
    }

    fn run(mut self) -> Result<Tokens> {
        while self.advance()? {}
        Ok(self.tokens)
    }

    fn advance(&mut self) -> Result<bool> {
        self.process()?;
        Ok((0..self.code.len()).contains(&self.position.index))
    }

    fn get_prev(&self) -> Option<char> {
        if self.position.index == 0 {
            None
        } else {
            Some(self.code[self.position.index - 1])
        }
    }

    fn get_this(&self) -> char {
        self.code[self.position.index]
    }

    fn get_next(&self) -> Option<char> {
        self.code.get(self.position.index + 1).map(|c| *c)
    }

    fn get_chars(&self) -> (Option<char>, char, Option<char>) {
        (self.get_prev(), self.get_this(), self.get_next())
    }

    fn make_error_pos(&self) -> ErrorPosition {
        let code_row = self.code[self.position.row_index..]
            .into_iter()
            .take_while(|&&c| c != '\n')
            .collect();

        ErrorPosition {
            code_row,
            row: self.position.row,
            column: self.position.column,
            file_path: None, // TODO:
        }
    }

    #[rustfmt::skip]
    fn process(&mut self) -> Result<()> {
        let chars = self.get_chars();

        let advance = match chars {
            // skip all whitespaces
            (_, '\n', _) =>         { self.position.new_line(); 0 }
            (_, c, _) if c.is_whitespace()
                =>                  { 1 },
            (_, '.', _) =>          { self.tokens.push(Token::Dot); 1 },
            (_, ',', _) =>          { self.tokens.push(Token::Comma); 1 },
            (_, '+', _) =>          { self.tokens.push(Token::Operator(Operator::Add)); 1 },
            (_, '-', Some('>')) =>  { self.tokens.push(Token::Arrow); 2 },
            (_, '-', _) =>          { self.tokens.push(Token::Operator(Operator::Sub)); 1 },
            (_, '*', _) =>          { self.tokens.push(Token::Operator(Operator::Mul)); 1 },
            (_, '/', Some('/')) =>  { self.inline_comment()? },
            (_, '/', Some('*')) =>  { self.block_comment()? },
            (_, '/', _) =>          { self.tokens.push(Token::Operator(Operator::Div)); 1 },
            (_, '(', _) =>          { self.tokens.push(Token::Group(Delimiter::Parentheses, Side::Left)); 1 },
            (_, ')', _) =>          { self.tokens.push(Token::Group(Delimiter::Parentheses, Side::Right)); 1 },
            (_, '{', _) =>          { self.tokens.push(Token::Group(Delimiter::Braces, Side::Left)); 1 },
            (_, '}', _) =>          { self.tokens.push(Token::Group(Delimiter::Braces, Side::Right)); 1 },
            (_, '[', _) =>          { self.tokens.push(Token::Group(Delimiter::Brackets, Side::Left)); 1 },
            (_, ']', _) =>          { self.tokens.push(Token::Group(Delimiter::Brackets, Side::Right)); 1 },
            (_, '\"', _) =>         { self.lit_str()? },
            (_, '\'', _) =>         { self.lit_char()? },
            (_, '0'..='9', _) =>    { self.lit_num()? },
            other => return Err(Error::InvalidCharacter(self.make_error_pos(), other.1)),
        };
        self.position.advance(advance);

        Ok(())
    }

    fn block_comment(&self) -> Result<usize> {
        let mut last = '\0';
        let last = match self.find_next(self.position.index, |&(_, &c)| {
            let result = last == '*' && c == '/';
            last = c;
            result
        }) {
            Some(last) => last,
            None => {
                return Err(Error::UnexpectedEOF(
                    self.make_error_pos(),
                    "while waiting for the tailing */",
                ))
            }
        };

        Ok(last + 1)
    }

    fn inline_comment(&self) -> Result<usize> {
        let mut last = self.position.index;
        let last = match self.find_next(last, |&(i, &c)| {
            last = i;
            c == '\n'
        }) {
            Some(last) => last,
            None => last + 1,
        };

        println!("{}", last);

        Ok(last)
    }

    fn find_next<P>(&self, after: usize, pred: P) -> Option<usize>
    where
        P: FnMut(&(usize, &char)) -> bool,
    {
        self.code[after..]
            .iter()
            .enumerate()
            .find(pred)
            .map(|(i, _)| i)
    }

    fn lit_str(&mut self) -> Result<usize> {
        let first = self.position.index;

        let last = match self.find_next(first + 1, |&(_, &c)| c == '\"') {
            Some(last) => last,
            None => {
                return Err(Error::UnexpectedEOF(
                    self.make_error_pos(),
                    "while waiting for the tailing \"",
                ))
            }
        };

        // TODO: escapes

        self.tokens.push(Token::LitStr(
            self.code[first + 1..first + 1 + last].into_iter().collect(),
        ));

        Ok(last + 3)
    }

    fn lit_char(&mut self) -> Result<usize> {
        let first = self.position.index;
        let last = match self.code[first + 1..]
            .iter()
            .enumerate()
            .find(|&(_, &c)| c == '\'')
        {
            Some((last, _)) => last,
            None => {
                return Err(Error::UnexpectedEOF(
                    self.make_error_pos(),
                    "while waiting for the tailing '",
                ))
            }
        };

        if last != 1 {
            return Err(Error::InvalidLitChar(
                self.make_error_pos(),
                "a char has to have exactly one codepoint",
            ));
        }

        // TODO: escapes

        self.tokens.push(Token::LitChar(self.code[first + 1]));

        Ok(last + 3)
    }

    fn parse_radix(&mut self) -> (u32, usize) {
        match self.get_chars() {
            (_, '0', Some('x')) => (16, 2),
            (_, '0', Some('o')) => (8, 2),
            (_, '0', Some('b')) => (2, 2),
            _ => (10, 0),
        }
    }

    fn lit_num(&mut self) -> Result<usize> {
        let (radix, mut offset) = self.parse_radix();
        let mut dot = false;

        loop {
            let c = self.code[self.position.index + offset];
            let is_dot = c == '.';
            let is_digit = c.is_digit(radix);

            // digit ends when there are no more numbers or dots
            // and if the char after dot is not a digit
            if !is_dot && !is_digit {
                break;
            }

            // first dot means that it is a float
            // second dot ends the digit
            if dot && is_dot {
                break;
            } else if is_dot {
                dot = true;
            }

            offset += 1;
        }

        let digit_str = self.code[self.position.index..self.position.index + offset]
            .into_iter()
            .collect::<String>();

        self.tokens.push(if dot {
            Token::LitFloat(
                digit_str
                    .parse()
                    .or_else(|err| Err(Error::InvalidLitFloat(self.make_error_pos(), err)))?,
            )
        } else {
            Token::LitInt(
                digit_str
                    .parse()
                    .or_else(|err| Err(Error::InvalidLitInt(self.make_error_pos(), err)))?,
            )
        });

        Ok(offset)
    }
}

pub fn run_lexer(code: &str) -> Result<Tokens> {
    Lexer::new(code).run()
}