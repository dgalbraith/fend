use crate::ast::Expr;
use crate::lexer::{Symbol, Token};

/*

Grammar:

expression = arrow_conversion
arrow_conversion = additive arrow_conversion_cont*
arrow_conversion_cont = '->' additive
additive = compound_fraction [addition_cont subtraction_cont]*
addition_cont = '+' compound_fraction
subtraction_cont = '-' compound_fraction
compound_fraction = A:multiplicative B:multiplicative?
  // if there are two terms, they are matched as follows:
  // <Num>, <Num> / <Num> => A + B # e.g. 8 1/2
  // -<Num>, <Num / <Num> => A - B # e.g. -8 1/2
  // <ApplyMul>, <ApplyMul> => A + B # e.g. 1 foot 3 inches
multiplicative = power [multiplication_cont division_cont]*
multiplication_cont = '*' power
division_cont = '/' power
power = whitespace? [('-' power) ('+' power) (apply power_cont)]
power_cont = ['^' '**'] power
apply = parens_or_literal [parens_or_literal]*
  // every pair of terms is matched as follows
  // <Num>, <Num> => error
  // <ApplyMul>, <Num> => error
  // _, <Num> => ApplyFunctionCall
  // <Num>, _ => ApplyMul
  // <ApplyMul>, _ => ApplyMul
  // _ => Apply
parens_or_literal = [number parens ident]
parens = whitespace? '(' expression ')' whitespace?
ident = whitespace? alphabetic [alphabetic '.']*
number =
    whitespace?
    base_prefix?
    basic_number
basic_number(base) = A:integer
    ('.' B:integer)?
    ('e' '-'? C:integer)?

  // A can have digit separators but no leading zero
  // B can have digit separators and leading zeroes
  // C can have digit separators but no leading zero,
  //   and is only considered if the base <= 10
  // A and B are in the parsed base, or 10 if unspecified.
  // C is always in base 10
  // If C is present, the number is multiplied by base^C

base_prefix = ['0x' '0o' '0b' (A:integer '#')]
  // A is decimal, and may not have leading zeroes or digit separators,
  // and must be between 2 and 36 inclusive.

*/

type ParseResult<'a, T> = Result<(T, &'a [Token]), String>;

fn parse_token(input: &[Token]) -> ParseResult<Token> {
    if input.is_empty() {
        Err("Expected a token".to_string())
    } else {
        Ok((input[0].clone(), &input[1..]))
    }
}

fn parse_fixed_symbol(input: &[Token], symbol: Symbol) -> ParseResult<()> {
    let (token, remaining) = parse_token(input)?;
    match token {
        Token::Symbol(sym) => {
            if sym == symbol {
                Ok(((), remaining))
            } else {
                Err(format!("Found '{}' while expecting '{}'", sym, symbol))
            }
        }
        _ => Err(format!(
            "Found an invalid token while expecting '{}'",
            symbol
        )),
    }
}

fn parse_number(input: &[Token]) -> ParseResult<Expr> {
    match parse_token(input)? {
        (Token::Num(num), remaining) => Ok((Expr::Num(num), remaining)),
        _ => Err("Expected a number".to_string()),
    }
}

fn parse_ident(input: &[Token]) -> ParseResult<Expr> {
    match parse_token(input)? {
        (Token::Ident(ident), remaining) => Ok((Expr::Ident(ident), remaining)),
        _ => Err("Expected an identifier".to_string()),
    }
}

fn parse_parens(input: &[Token]) -> ParseResult<Expr> {
    let (_, input) = parse_fixed_symbol(input, Symbol::OpenParens)?;
    let (inner, input) = parse_expression(input)?;
    let (_, input) = parse_fixed_symbol(input, Symbol::CloseParens)?;
    Ok((Expr::Parens(Box::new(inner)), input))
}

fn parse_parens_or_literal(input: &[Token]) -> ParseResult<Expr> {
    let (token, _) = parse_token(input)?;

    match token {
        Token::Num(_) => parse_number(input),
        Token::Ident(_) => parse_ident(input),
        Token::Symbol(Symbol::OpenParens) => parse_parens(input),
        _ => Err("Expected a number, an identifier or an open parenthesis".to_string()),
    }
}

fn parse_apply(input: &[Token]) -> ParseResult<Expr> {
    let (mut res, mut input) = parse_parens_or_literal(input)?;
    loop {
        if let Ok((term, remaining)) = parse_parens_or_literal(input) {
            res = match (&res, &term) {
                (Expr::Num(_), Expr::Num(_)) => {
                    // this may later be parsed as a compound fraction
                    return Ok((res, input));
                }
                (Expr::ApplyMul(_, _), Expr::Num(_)) => {
                    // this may later become an addition, e.g. 6 feet 1 inch
                    return Ok((res, input));
                }
                (_, Expr::Num(_)) => Expr::ApplyFunctionCall(Box::new(res), Box::new(term)),
                (Expr::Num(_), _) | (Expr::ApplyMul(_, _), _) => {
                    Expr::ApplyMul(Box::new(res), Box::new(term))
                }
                _ => Expr::Apply(Box::new(res), Box::new(term)),
            };
            input = remaining;
        } else {
            break;
        }
    }
    Ok((res, input))
}

fn parse_power_cont(mut input: &[Token]) -> ParseResult<Expr> {
    if let Ok((_, remaining)) = parse_fixed_symbol(input, Symbol::Pow) {
        input = remaining;
    } else {
        return Err("Expected ^ or **".to_string());
    }
    let (b, input) = parse_power(input)?;
    Ok((b, input))
}

fn parse_power(input: &[Token]) -> ParseResult<Expr> {
    if let Ok((_, remaining)) = parse_fixed_symbol(input, Symbol::Sub) {
        let (res, remaining) = parse_power(remaining)?;
        return Ok((Expr::UnaryMinus(Box::new(res)), remaining));
    }
    if let Ok((_, remaining)) = parse_fixed_symbol(input, Symbol::Add) {
        let (res, remaining) = parse_power(remaining)?;
        return Ok((Expr::UnaryPlus(Box::new(res)), remaining));
    }
    let (mut res, mut input) = parse_apply(input)?;
    if let Ok((term, remaining)) = parse_power_cont(input) {
        res = Expr::Pow(Box::new(res), Box::new(term));
        input = remaining;
    }
    Ok((res, input))
}

fn parse_multiplication_cont(input: &[Token]) -> ParseResult<Expr> {
    let (_, input) = parse_fixed_symbol(input, Symbol::Mul)?;
    let (b, input) = parse_power(input)?;
    Ok((b, input))
}

fn parse_division_cont(input: &[Token]) -> ParseResult<Expr> {
    let (_, input) = parse_fixed_symbol(input, Symbol::Div)?;
    let (b, input) = parse_power(input)?;
    Ok((b, input))
}

fn parse_multiplicative(input: &[Token]) -> ParseResult<Expr> {
    let (mut res, mut input) = parse_power(input)?;
    loop {
        if let Ok((term, remaining)) = parse_multiplication_cont(input) {
            res = Expr::Mul(Box::new(res), Box::new(term));
            input = remaining;
        } else if let Ok((term, remaining)) = parse_division_cont(input) {
            res = Expr::Div(Box::new(res), Box::new(term));
            input = remaining;
        } else {
            break;
        }
    }
    Ok((res, input))
}

fn parse_compound_fraction(input: &[Token]) -> ParseResult<Expr> {
    let (res, input) = parse_multiplicative(input)?;
    if let Ok((rhs, remaining)) = parse_multiplicative(input) {
        match (&res, &rhs) {
            (Expr::Num(_), Expr::Div(b, c)) => match (&**b, &**c) {
                (Expr::Num(_), Expr::Num(_)) => {
                    return Ok((Expr::Add(Box::new(res), Box::new(rhs)), remaining))
                }
                _ => (),
            },
            (Expr::UnaryMinus(a), Expr::Div(b, c)) => match (&**a, &**b, &**c) {
                (Expr::Num(_), Expr::Num(_), Expr::Num(_)) => {
                    return Ok((
                        // note that res = '-<num>' here because unary minus has a higher precedence
                        Expr::Sub(Box::new(res), Box::new(rhs)),
                        remaining,
                    ));
                }
                _ => (),
            },
            (Expr::ApplyMul(_, _), Expr::ApplyMul(_, _)) => {
                return Ok((Expr::Add(Box::new(res), Box::new(rhs)), remaining))
            }
            _ => (),
        };
    }
    Ok((res, input))
}

fn parse_addition_cont(input: &[Token]) -> ParseResult<Expr> {
    let (_, input) = parse_fixed_symbol(input, Symbol::Add)?;
    let (b, input) = parse_compound_fraction(input)?;
    Ok((b, input))
}

fn parse_subtraction_cont(input: &[Token]) -> ParseResult<Expr> {
    let (_, input) = parse_fixed_symbol(input, Symbol::Sub)?;
    let (b, input) = parse_compound_fraction(input)?;
    Ok((b, input))
}

fn parse_additive(input: &[Token]) -> ParseResult<Expr> {
    let (mut res, mut input) = parse_compound_fraction(input)?;
    loop {
        if let Ok((term, remaining)) = parse_addition_cont(input) {
            res = Expr::Add(Box::new(res), Box::new(term));
            input = remaining;
        } else if let Ok((term, remaining)) = parse_subtraction_cont(input) {
            res = Expr::Sub(Box::new(res), Box::new(term));
            input = remaining;
        } else {
            break;
        }
    }
    Ok((res, input))
}

fn parse_arrow_conversion_cont(input: &[Token]) -> ParseResult<Expr> {
    let (_, input) = parse_fixed_symbol(input, Symbol::ArrowConversion)?;
    let (b, input) = parse_additive(input)?;
    Ok((b, input))
}

fn parse_arrow_conversion(input: &[Token]) -> ParseResult<Expr> {
    let (mut res, mut input) = parse_additive(input)?;
    loop {
        if let Ok((term, remaining)) = parse_arrow_conversion_cont(input) {
            res = Expr::As(Box::new(res), Box::new(term));
            input = remaining;
        } else {
            break;
        }
    }
    Ok((res, input))
}

pub fn parse_expression(input: &[Token]) -> ParseResult<Expr> {
    parse_arrow_conversion(input)
}

pub fn parse_string(input: &str) -> Result<Expr, String> {
    let tokens = crate::lexer::lex(input)?;
    let (res, remaining) = parse_expression(tokens.as_slice())?;
    if !remaining.is_empty() {
        return Err(format!("Unexpected input found: '{}'", input));
    }
    Ok(res)
}