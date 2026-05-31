use pest::Parser;
use pest::error::InputLocation;
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::ast::{Argument, Expr, ExprKind, Parameter, Span, Statement, StatementKind};
use crate::{Error, Result};

#[derive(Parser)]
#[grammar = "snare.pest"]
struct SnareParser;

pub fn parse(source: &str) -> Result<Vec<Statement>> {
    let mut pairs = SnareParser::parse(Rule::program, source)
        .map_err(|error| Error::Parse(error.to_string()))?;
    pairs
        .next()
        .expect("program")
        .into_inner()
        .filter(|pair| pair.as_rule() != Rule::EOI)
        .map(parse_statement)
        .collect()
}

pub fn is_incomplete(source: &str) -> bool {
    if has_unclosed_multiline_string(source) {
        return true;
    }
    if parse(&with_trailing_semicolon(source)).is_ok() {
        return false;
    }
    SnareParser::parse(Rule::program, source)
        .err()
        .is_some_and(|error| match error.location {
            InputLocation::Pos(position) => position == source.len(),
            InputLocation::Span((_, end)) => end == source.len(),
        })
}

fn has_unclosed_multiline_string(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut multiline = false;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"\"\"\"") {
            multiline = !multiline;
            index += 3;
        } else if multiline {
            index += 1;
        } else if bytes[index..].starts_with(b"//") {
            index += bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .unwrap_or(bytes.len() - index);
        } else if bytes[index] == b'"' {
            index += 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 2,
                    b'"' => {
                        index += 1;
                        break;
                    }
                    _ => index += 1,
                }
            }
        } else {
            index += 1;
        }
    }
    multiline
}

fn with_trailing_semicolon(source: &str) -> String {
    if source.trim_end().ends_with(';') {
        source.to_owned()
    } else {
        format!("{source};")
    }
}

fn parse_statement(pair: Pair<Rule>) -> Result<Statement> {
    let span = span(&pair);
    match pair.as_rule() {
        Rule::let_statement => {
            let mut inner = pair.into_inner();
            let name = inner.next().expect("let name").as_str().to_owned();
            Ok(Statement {
                kind: StatementKind::Let(name, parse_expr(inner.next().expect("let value"))?),
                span,
            })
        }
        Rule::assignment_statement => {
            let mut inner = pair.into_inner();
            let name = inner.next().expect("assignment name").as_str().to_owned();
            Ok(Statement {
                kind: StatementKind::Assign(
                    name,
                    parse_expr(inner.next().expect("assignment value"))?,
                ),
                span,
            })
        }
        Rule::expression_statement => Ok(Statement {
            kind: StatementKind::Expr(parse_expr(pair.into_inner().next().expect("expression"))?),
            span,
        }),
        _ => unreachable!("unexpected statement: {:?}", pair.as_rule()),
    }
}

fn parse_expr(pair: Pair<Rule>) -> Result<Expr> {
    let pair_span = span(&pair);
    match pair.as_rule() {
        Rule::expression => {
            let mut inner = pair.into_inner();
            let first = parse_expr(inner.next().expect("left operand"))?;
            let mut rest = vec![];
            while let Some(operator) = inner.next() {
                let right = parse_expr(inner.next().expect("right operand"))?;
                rest.push((operator.as_str().to_owned(), right));
            }
            Ok(parse_binary(first, &rest, &mut 0, 1))
        }
        Rule::prefix => {
            let mut inner: Vec<_> = pair.into_inner().collect();
            let value = parse_expr(inner.pop().expect("prefix value"))?;
            inner.into_iter().rev().try_fold(value, |value, operator| {
                let span = Span {
                    start: operator.as_span().start(),
                    end: value.span.end,
                };
                Ok(Expr {
                    kind: ExprKind::Unary(operator.as_str().to_owned(), Box::new(value)),
                    span,
                })
            })
        }
        Rule::postfix => {
            let mut inner = pair.into_inner();
            let value = parse_expr(inner.next().expect("postfix value"))?;
            inner.try_fold(value, |value, suffix| match suffix.as_rule() {
                Rule::call => Ok(Expr {
                    kind: ExprKind::Call(Box::new(value), parse_call(suffix)?),
                    span: pair_span,
                }),
                Rule::member => Ok(Expr {
                    kind: ExprKind::Member(
                        Box::new(value),
                        suffix
                            .into_inner()
                            .next()
                            .expect("member name")
                            .as_str()
                            .to_owned(),
                    ),
                    span: pair_span,
                }),
                Rule::index => Ok(Expr {
                    kind: ExprKind::Index(
                        Box::new(value),
                        Box::new(parse_expr(
                            suffix.into_inner().next().expect("index expression"),
                        )?),
                    ),
                    span: pair_span,
                }),
                _ => unreachable!("unexpected postfix"),
            })
        }
        Rule::grouped => parse_expr(pair.into_inner().next().expect("grouped expression")),
        Rule::identifier => Ok(expr(
            ExprKind::Identifier(pair.as_str().to_owned()),
            pair_span,
        )),
        Rule::null => Ok(expr(ExprKind::Null, pair_span)),
        Rule::boolean => Ok(expr(ExprKind::Bool(pair.as_str() == "true"), pair_span)),
        Rule::number => Ok(expr(
            ExprKind::Number(
                pair.as_str()
                    .parse()
                    .map_err(|_| Error::Parse("invalid number".into()))?,
            ),
            pair_span,
        )),
        Rule::string => Ok(expr(
            ExprKind::String(
                serde_json::from_str(pair.as_str())
                    .map_err(|error| Error::Parse(error.to_string()))?,
            ),
            pair_span,
        )),
        Rule::multiline_string => Ok(expr(
            ExprKind::String(pair.as_str()[3..pair.as_str().len() - 3].to_owned()),
            pair_span,
        )),
        Rule::array => pair
            .into_inner()
            .map(parse_expr)
            .collect::<Result<_>>()
            .map(|values| expr(ExprKind::Array(values), pair_span)),
        Rule::object => pair
            .into_inner()
            .map(|entry| {
                let mut inner = entry.into_inner();
                let key = serde_json::from_str(inner.next().expect("object key").as_str())
                    .map_err(|error| Error::Parse(error.to_string()))?;
                Ok((key, parse_expr(inner.next().expect("object value"))?))
            })
            .collect::<Result<_>>()
            .map(|values| expr(ExprKind::Object(values), pair_span)),
        Rule::function => {
            let mut inner: Vec<_> = pair.into_inner().collect();
            let body = parse_expr(inner.pop().expect("function body"))?;
            let parameters = if inner.is_empty() {
                vec![]
            } else {
                parse_parameters(inner.pop().expect("parameters"))?
            };
            Ok(expr(
                ExprKind::Function(parameters, Box::new(body)),
                pair_span,
            ))
        }
        Rule::if_expression => {
            let mut inner = pair.into_inner();
            Ok(expr(
                ExprKind::If(
                    Box::new(parse_expr(inner.next().expect("condition"))?),
                    Box::new(parse_expr(inner.next().expect("then"))?),
                    Box::new(parse_expr(inner.next().expect("else"))?),
                ),
                pair_span,
            ))
        }
        Rule::do_expression => {
            let mut inner: Vec<_> = pair.into_inner().collect();
            let result = parse_expr(inner.pop().expect("do result"))?;
            let statements = inner
                .into_iter()
                .map(parse_statement)
                .collect::<Result<_>>()?;
            Ok(expr(ExprKind::Do(statements, Box::new(result)), pair_span))
        }
        _ => unreachable!("unexpected expression: {:?}", pair.as_rule()),
    }
}

fn parse_call(pair: Pair<Rule>) -> Result<Vec<Argument>> {
    let Some(arguments) = pair.into_inner().next() else {
        return Ok(vec![]);
    };
    arguments
        .into_inner()
        .map(|argument| {
            let inner = argument.into_inner().next().expect("argument");
            if inner.as_rule() == Rule::named_argument {
                let mut named = inner.into_inner();
                Ok(Argument::Named(
                    named.next().expect("argument name").as_str().to_owned(),
                    parse_expr(named.next().expect("argument value"))?,
                ))
            } else {
                Ok(Argument::Positional(parse_expr(inner)?))
            }
        })
        .collect()
}

fn parse_parameters(pair: Pair<Rule>) -> Result<Vec<Parameter>> {
    pair.into_inner()
        .map(|parameter| {
            let mut inner = parameter.into_inner();
            Ok(Parameter {
                name: inner.next().expect("parameter name").as_str().to_owned(),
                default: inner.next().map(parse_expr).transpose()?,
            })
        })
        .collect()
}

fn precedence(operator: &str) -> u8 {
    match operator {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" => 3,
        "<" | "<=" | ">" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        _ => unreachable!("operator"),
    }
}

fn parse_binary(mut left: Expr, rest: &[(String, Expr)], index: &mut usize, minimum: u8) -> Expr {
    while let Some((operator, mut right)) = rest.get(*index).cloned() {
        let current = precedence(&operator);
        if current < minimum {
            break;
        }
        *index += 1;
        while let Some((next_operator, _)) = rest.get(*index) {
            let next = precedence(next_operator);
            if next <= current {
                break;
            }
            right = parse_binary(right, rest, index, next);
        }
        let span = Span {
            start: left.span.start,
            end: right.span.end,
        };
        left = expr(
            ExprKind::Binary(Box::new(left), operator, Box::new(right)),
            span,
        );
    }
    left
}

fn expr(kind: ExprKind, span: Span) -> Expr {
    Expr { kind, span }
}

fn span(pair: &Pair<Rule>) -> Span {
    Span {
        start: pair.as_span().start(),
        end: pair.as_span().end(),
    }
}
