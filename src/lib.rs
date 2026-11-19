//! Evidence-backed interpreter for the documented historical LSE subset.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

const MAX_ARRAY_ELEMENTS: usize = 1_000_000;
const MAX_FORMAT_REPETITIONS: usize = 1_000_000;

pub type ArrayReference = Rc<RefCell<ArrayValue>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicType {
    Form,
    Page,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphicValue {
    pub handle: Option<u64>,
    pub kind: GraphicType,
}

impl GraphicValue {
    pub fn new(handle: u64, kind: GraphicType) -> Self {
        Self {
            handle: Some(handle),
            kind,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphicsArgument {
    Number(f64),
    Text(String),
    Boolean(bool),
    Object(GraphicValue),
}

#[derive(Clone, Debug, PartialEq)]
pub enum GraphicsResult {
    Number(f64),
    Object(GraphicValue),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SystemArgument {
    Identifier(String),
    Number(f64),
    Text(String),
    Boolean(bool),
}

#[derive(Clone, Debug)]
pub struct ArrayValue {
    dimensions: Vec<usize>,
    kind: VariableKind,
    elements: Vec<Value>,
}

impl ArrayValue {
    pub fn dimensions(&self) -> &[usize] {
        &self.dimensions
    }

    pub fn elements(&self) -> Vec<Value> {
        self.elements.clone()
    }

    pub fn element_type(&self) -> &'static str {
        match self.kind {
            VariableKind::Real => "numeric",
            VariableKind::Text => "string",
            VariableKind::Boolean => "Boolean",
            VariableKind::Graphic => "FORME",
        }
    }

    fn offset(&self, indices: &[usize]) -> Result<usize, RuntimeError> {
        if indices.len() != self.dimensions.len() {
            return Err(fail(format!(
                "the table has {} dimensions but {} indices were supplied",
                self.dimensions.len(),
                indices.len()
            )));
        }
        let mut offset = 0_usize;
        for (index, dimension) in indices.iter().zip(&self.dimensions) {
            if *index == 0 || *index > *dimension {
                return Err(fail(format!(
                    "table index {index} is outside 1..={dimension}"
                )));
            }
            offset = offset
                .checked_mul(*dimension)
                .and_then(|value| value.checked_add(*index - 1))
                .ok_or_else(|| fail("dépassement lors du calcul de l'indice du tableau"))?;
        }
        Ok(offset)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}
impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.span.line, self.span.column, self.message
        )
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Real(f64),
    Text(String),
    Boolean(bool),
    Array(ArrayReference),
    Procedure(String),
    Graphic(Option<GraphicValue>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoredElementKind {
    Numeric,
    Text,
    Boolean,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StoredScalar {
    Real(f64),
    Text(String),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StoredValue {
    Real(f64),
    Text(String),
    Boolean(bool),
    Array {
        dimensions: Vec<usize>,
        element_kind: StoredElementKind,
        elements: Vec<StoredScalar>,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Real(a), Self::Real(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => Rc::ptr_eq(a, b),
            (Self::Procedure(a), Self::Procedure(b)) => a.eq_ignore_ascii_case(b),
            (Self::Graphic(a), Self::Graphic(b)) => a == b,
            _ => false,
        }
    }
}
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Real(n) if n.fract() == 0.0 && n.abs() < i64::MAX as f64 => {
                write!(f, "{}", *n as i64)
            }
            Self::Real(n) => write!(f, "{n}"),
            Self::Text(s) => f.write_str(s),
            Self::Boolean(b) => write!(f, "{}", if *b { "VRAI" } else { "FAUX" }),
            Self::Array(array) => {
                let array = array.borrow();
                write!(f, "<table {:?}>", array.dimensions)
            }
            Self::Procedure(name) => write!(f, "<procedure &{name}>"),
            Self::Graphic(Some(value)) => write!(
                f,
                "<graphic {:?} {}>",
                value.kind,
                value.handle.unwrap_or_default()
            ),
            Self::Graphic(None) => f.write_str("<uninitialized graphic>"),
        }
    }
}

#[derive(Clone, Debug)]
enum Expr {
    Real(f64),
    Text(String),
    Boolean(bool),
    Variable(String),
    Index(String, Vec<Expr>),
    ProcedureReference(String),
    ProcedureCall(String, Vec<Expr>),
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>),
    Unary(Unary, Box<Expr>),
    Binary(Box<Expr>, Op, Box<Expr>),
    Call(String, Vec<Expr>),
}
#[derive(Clone, Copy, Debug)]
enum Unary {
    Negate,
    Not,
}
#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Power,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Juxtapose,
    Superpose,
}
#[derive(Clone, Debug, PartialEq)]
enum Tk {
    Name(String),
    Real(f64),
    Text(String),
    Boolean(bool),
    LParen,
    RParen,
    Comma,
    LBracket,
    RBracket,
    Ampersand,
    Plus,
    Minus,
    Star,
    Slash,
    Power,
    Bang,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Colon,
    Percent,
    Eof,
}
#[derive(Clone, Debug)]
struct Token {
    kind: Tk,
    column: usize,
}

fn lex(source: &str, line: usize) -> Result<Vec<Token>, Diagnostic> {
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    let mut tokens = Vec::new();
    while at < chars.len() {
        let c = chars[at];
        let column = at + 1;
        if c.is_whitespace() {
            at += 1;
            continue;
        }
        if c == '.' && at + 5 < chars.len() && chars[at + 5] == '.' {
            let word: String = chars[at + 1..at + 5].iter().collect();
            if word.eq_ignore_ascii_case("VRAI") || word.eq_ignore_ascii_case("FAUX") {
                tokens.push(Token {
                    kind: Tk::Boolean(word.eq_ignore_ascii_case("VRAI")),
                    column,
                });
                at += 6;
                continue;
            }
        }
        let one = match c {
            '(' => Some(Tk::LParen),
            ')' => Some(Tk::RParen),
            ',' => Some(Tk::Comma),
            '[' => Some(Tk::LBracket),
            ']' => Some(Tk::RBracket),
            '&' => Some(Tk::Ampersand),
            '+' => Some(Tk::Plus),
            '-' => Some(Tk::Minus),
            '*' => Some(Tk::Star),
            '/' => Some(Tk::Slash),
            '↑' => Some(Tk::Power),
            '!' => Some(Tk::Bang),
            '=' => Some(Tk::Eq),
            '#' => Some(Tk::Ne),
            '<' => Some(Tk::Lt),
            '>' => Some(Tk::Gt),
            ':' => Some(Tk::Colon),
            '%' => Some(Tk::Percent),
            _ => None,
        };
        if let Some(mut kind) = one {
            at += 1;
            if (c == '<' || c == '>') && chars.get(at) == Some(&'=') {
                at += 1;
                kind = if c == '<' { Tk::Le } else { Tk::Ge };
            }
            tokens.push(Token { kind, column });
            continue;
        }
        if c == '\'' {
            at += 1;
            let mut text = String::new();
            while at < chars.len() {
                if chars[at] == '\'' {
                    if chars.get(at + 1) == Some(&'\'') {
                        text.push('\'');
                        at += 2;
                        continue;
                    }
                    at += 1;
                    break;
                }
                text.push(chars[at]);
                at += 1;
            }
            if at > chars.len() || chars.get(at.saturating_sub(1)) != Some(&'\'') {
                return Err(Diagnostic {
                    span: Span { line, column },
                    message: "chaîne non terminée".into(),
                });
            }
            tokens.push(Token {
                kind: Tk::Text(text),
                column,
            });
            continue;
        }
        if c.is_ascii_digit() || c == '.' && chars.get(at + 1).is_some_and(|x| x.is_ascii_digit()) {
            let start = at;
            at += 1;
            while at < chars.len() && (chars[at].is_ascii_digit() || chars[at] == '.') {
                at += 1;
            }
            let value: String = chars[start..at].iter().collect();
            let n = value.parse().map_err(|_| Diagnostic {
                span: Span { line, column },
                message: "constante numérique incorrecte".into(),
            })?;
            tokens.push(Token {
                kind: Tk::Real(n),
                column,
            });
            continue;
        }
        if c.is_ascii_alphabetic() {
            let start = at;
            at += 1;
            while at < chars.len() && chars[at].is_ascii_alphanumeric() {
                at += 1;
            }
            let name: String = chars[start..at].iter().collect();
            if !valid_identifier(&name) {
                return Err(Diagnostic {
                    span: Span { line, column },
                    message: "un identificateur LSE doit contenir une lettre suivie d'au plus quatre lettres ou chiffres".into(),
                });
            }
            tokens.push(Token {
                kind: Tk::Name(name),
                column,
            });
            continue;
        }
        return Err(Diagnostic {
            span: Span { line, column },
            message: format!("caractère non documenté `{c}`"),
        });
    }
    tokens.push(Token {
        kind: Tk::Eof,
        column: source.chars().count() + 1,
    });
    Ok(tokens)
}

struct ExprParser {
    tokens: Vec<Token>,
    at: usize,
    line: usize,
}
impl ExprParser {
    fn parse(source: &str, line: usize) -> Result<Expr, Diagnostic> {
        let tokens = lex(source, line)?;
        let mut p = Self {
            tokens,
            at: 0,
            line,
        };
        let e = p.expr(0)?;
        if p.current().kind != Tk::Eof {
            return Err(p.error("texte après l'expression"));
        }
        Ok(e)
    }
    fn expr(&mut self, min: u8) -> Result<Expr, Diagnostic> {
        let mut left = self.prefix()?;
        loop {
            if self.current().kind == Tk::LBracket {
                let Expr::Variable(name) = left else {
                    break;
                };
                self.at += 1;
                let mut indices = Vec::new();
                if self.current().kind != Tk::RBracket {
                    loop {
                        indices.push(self.expr(0)?);
                        if self.take(&Tk::Comma) {
                            continue;
                        }
                        break;
                    }
                }
                self.expect(Tk::RBracket, "] attendu")?;
                if indices.is_empty() {
                    return Err(self.error("un indice de tableau est attendu"));
                }
                left = Expr::Index(name, indices);
                continue;
            }
            if self.current().kind == Tk::LParen {
                if let Expr::Variable(name) = left {
                    self.at += 1;
                    let mut args = Vec::new();
                    if self.current().kind != Tk::RParen {
                        loop {
                            args.push(self.expr(0)?);
                            if self.take(&Tk::Comma) {
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(Tk::RParen, ") attendu")?;
                    left = Expr::Call(name, args);
                    continue;
                }
            }
            let Some((op, prec)) = self.operator() else {
                break;
            };
            if prec < min {
                break;
            }
            self.at += 1;
            let right = self.expr(if matches!(op, Op::Power) {
                prec
            } else {
                prec + 1
            })?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }
    fn prefix(&mut self) -> Result<Expr, Diagnostic> {
        let t = self.current().clone();
        match t.kind {
            Tk::Real(n) => {
                self.at += 1;
                Ok(Expr::Real(n))
            }
            Tk::Text(s) => {
                self.at += 1;
                Ok(Expr::Text(s))
            }
            Tk::Boolean(value) => {
                self.at += 1;
                Ok(Expr::Boolean(value))
            }
            Tk::Name(n) if n.eq_ignore_ascii_case("SI") => {
                self.at += 1;
                let condition = self.expr(0)?;
                self.expect_name("ALORS", "ALORS attendu dans l'expression conditionnelle")?;
                let yes = self.expr(0)?;
                self.expect_name("SINON", "SINON attendu dans l'expression conditionnelle")?;
                let no = self.expr(0)?;
                Ok(Expr::Conditional(
                    Box::new(condition),
                    Box::new(yes),
                    Box::new(no),
                ))
            }
            Tk::Name(n) if n.eq_ignore_ascii_case("NON") => {
                self.at += 1;
                Ok(Expr::Unary(Unary::Not, Box::new(self.expr(6)?)))
            }
            Tk::Name(n) => {
                self.at += 1;
                if n.eq_ignore_ascii_case("DAT") && self.current().kind != Tk::LParen {
                    Ok(Expr::Call(n, Vec::new()))
                } else {
                    Ok(Expr::Variable(n))
                }
            }
            Tk::Ampersand => {
                self.at += 1;
                let Tk::Name(name) = self.current().kind.clone() else {
                    return Err(self.error("identificateur attendu après &"));
                };
                self.at += 1;
                if self.current().kind == Tk::LParen {
                    self.at += 1;
                    let mut args = Vec::new();
                    if self.current().kind != Tk::RParen {
                        loop {
                            args.push(self.expr(0)?);
                            if self.take(&Tk::Comma) {
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(Tk::RParen, ") attendu")?;
                    Ok(Expr::ProcedureCall(name, args))
                } else {
                    Ok(Expr::ProcedureReference(name))
                }
            }
            Tk::Minus => {
                self.at += 1;
                Ok(Expr::Unary(Unary::Negate, Box::new(self.expr(6)?)))
            }
            Tk::LParen => {
                self.at += 1;
                let e = self.expr(0)?;
                self.expect(Tk::RParen, ") attendu")?;
                Ok(e)
            }
            _ => Err(Diagnostic {
                span: Span {
                    line: self.line,
                    column: t.column,
                },
                message: "expression attendue".into(),
            }),
        }
    }
    fn operator(&self) -> Option<(Op, u8)> {
        let x = match &self.current().kind {
            Tk::Name(s) if s.eq_ignore_ascii_case("OU") => (Op::Or, 1),
            Tk::Name(s) if s.eq_ignore_ascii_case("ET") => (Op::And, 2),
            Tk::Eq => (Op::Eq, 3),
            Tk::Ne => (Op::Ne, 3),
            Tk::Lt => (Op::Lt, 3),
            Tk::Le => (Op::Le, 3),
            Tk::Gt => (Op::Gt, 3),
            Tk::Ge => (Op::Ge, 3),
            Tk::Colon => (Op::Juxtapose, 3),
            Tk::Percent => (Op::Superpose, 3),
            Tk::Plus => (Op::Add, 4),
            Tk::Minus => (Op::Sub, 4),
            Tk::Bang => (Op::Concat, 4),
            Tk::Star => (Op::Mul, 5),
            Tk::Slash => (Op::Div, 5),
            Tk::Power => (Op::Power, 6),
            _ => return None,
        };
        Some(x)
    }
    fn current(&self) -> &Token {
        &self.tokens[self.at.min(self.tokens.len() - 1)]
    }
    fn take(&mut self, k: &Tk) -> bool {
        if std::mem::discriminant(&self.current().kind) == std::mem::discriminant(k) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, k: Tk, msg: &str) -> Result<(), Diagnostic> {
        if self.take(&k) {
            Ok(())
        } else {
            Err(self.error(msg))
        }
    }
    fn expect_name(&mut self, expected: &str, msg: &str) -> Result<(), Diagnostic> {
        if matches!(&self.current().kind, Tk::Name(name) if name.eq_ignore_ascii_case(expected)) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(msg))
        }
    }
    fn error(&self, msg: &str) -> Diagnostic {
        Diagnostic {
            span: Span {
                line: self.line,
                column: self.current().column,
            },
            message: msg.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    lines: Vec<ProgramLine>,
    string_variables: HashSet<String>,
    boolean_variables: HashSet<String>,
    graphic_variables: HashMap<String, GraphicType>,
    array_variables: HashMap<String, VariableKind>,
    procedures: HashMap<String, Procedure>,
    labels: HashMap<i64, usize>,
}
#[derive(Clone, Debug)]
struct ProgramLine {
    label: i64,
    source_line: usize,
    statements: Vec<Stmt>,
}

#[derive(Clone, Debug)]
struct DisplayFormat {
    repeat: FormatRepeat,
    operation: FormatOperation,
}

#[derive(Clone, Copy, Debug)]
enum FormatRepeat {
    Once,
    Fixed(usize),
    Argument,
}

#[derive(Clone, Debug)]
enum FormatOperation {
    Newline,
    CarriageReturn,
    LineFeed,
    Spaces,
    Literal(String),
    Value(ValueFormat),
}

#[derive(Clone, Copy, Debug)]
enum ValueFormat {
    Universal,
    Fixed {
        digits_before: usize,
        digits_after: usize,
    },
    Scientific {
        digits_before: usize,
        digits_after: usize,
    },
}

#[derive(Clone, Debug)]
enum Stmt {
    Declaration(VariableKind, Vec<String>),
    GraphicDeclaration(GraphicType, Vec<String>),
    Motif(Expr, Expr),
    GraphicCommand(String, Vec<Expr>),
    ArrayDeclaration(Vec<ArrayDeclaration>),
    Assign(String, Expr),
    ArrayAssign(String, Vec<Expr>, Expr),
    ProcedureCall(String, Vec<Expr>),
    Return(Option<Expr>),
    Terminate,
    Jump(i64),
    For {
        body_label: i64,
        variable: String,
        start: Expr,
        step: Expr,
        limit: Option<Expr>,
        condition: Option<Expr>,
    },
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    Block(Vec<Stmt>),
    Read {
        names: Vec<String>,
        prompt: Vec<DisplayFormat>,
    },
    StoreRecord {
        name: String,
        key: Expr,
        file: Expr,
        status: Option<String>,
    },
    LoadRecord {
        name: String,
        key: Expr,
        file: Expr,
        status: Option<String>,
    },
    ExecuteProgram {
        program: Expr,
        argument: Expr,
        status: Option<String>,
    },
    DeleteRecords {
        file: Expr,
        key: Option<Expr>,
    },
    Display {
        formats: Vec<DisplayFormat>,
        expressions: Vec<Expr>,
        implicit_newline: bool,
    },
    Free(Vec<String>),
    Pause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VariableKind {
    Real,
    Text,
    Boolean,
    Graphic,
}

impl VariableKind {
    fn name(self) -> &'static str {
        match self {
            Self::Real => "numeric",
            Self::Text => "string",
            Self::Boolean => "Boolean",
            Self::Graphic => "FORME",
        }
    }
}

fn default_value(kind: VariableKind) -> Value {
    match kind {
        VariableKind::Real => Value::Real(0.0),
        VariableKind::Text => Value::Text(String::new()),
        VariableKind::Boolean => Value::Boolean(false),
        VariableKind::Graphic => Value::Graphic(None),
    }
}

fn value_matches_kind(value: &Value, kind: VariableKind) -> bool {
    matches!(
        (value, kind),
        (Value::Real(_), VariableKind::Real)
            | (Value::Text(_), VariableKind::Text)
            | (Value::Boolean(_), VariableKind::Boolean)
            | (Value::Graphic(_), VariableKind::Graphic)
    )
}

fn parse_input_value(kind: VariableKind, raw: String) -> Result<Value, RuntimeError> {
    match kind {
        VariableKind::Real => Ok(Value::Real(
            raw.trim()
                .parse()
                .map_err(|_| fail("une valeur numérique est attendue"))?,
        )),
        VariableKind::Text => Ok(Value::Text(raw)),
        VariableKind::Boolean => match raw.trim().to_ascii_uppercase().as_str() {
            ".VRAI." => Ok(Value::Boolean(true)),
            ".FAUX." => Ok(Value::Boolean(false)),
            _ => Err(fail("une valeur .VRAI. ou .FAUX. est attendue")),
        },
        VariableKind::Graphic => Err(fail("LIRE ne prend pas en charge les formes graphiques")),
    }
}

fn format_output_value(value: Value, format: ValueFormat) -> Result<String, RuntimeError> {
    match format {
        ValueFormat::Universal => match value {
            Value::Real(number) => {
                let rendered =
                    if number.abs() >= 1_000_000.0 || (number != 0.0 && number.abs() < 0.001) {
                        format!("{number:e}")
                    } else {
                        Value::Real(number).to_string()
                    };
                Ok(format!("{rendered} "))
            }
            Value::Text(text) => Ok(text),
            Value::Boolean(value) => Ok(format!("{} ", if value { ".VRAI." } else { ".FAUX." })),
            Value::Array(_) | Value::Procedure(_) | Value::Graphic(_) => Err(fail(
                "cette valeur ne peut pas être affichée avec le format U",
            )),
        },
        ValueFormat::Fixed {
            digits_before,
            digits_after,
        }
        | ValueFormat::Scientific {
            digits_before,
            digits_after,
        } => {
            let Value::Real(number) = value else {
                return Err(fail("un format numérique attend une valeur numérique"));
            };
            let width = digits_before
                .checked_add(digits_after)
                .and_then(|width| width.checked_add(1))
                .filter(|width| *width <= 2_049)
                .ok_or_else(|| fail("largeur excessive dans le format numérique"))?;
            if matches!(format, ValueFormat::Fixed { .. }) {
                Ok(format!(
                    "{number:>width$.precision$}",
                    width = width,
                    precision = digits_after
                ))
            } else {
                Ok(format!(
                    "{number:>width$.precision$e}",
                    width = width,
                    precision = digits_after
                ))
            }
        }
    }
}

fn stored_kind(kind: VariableKind) -> Result<StoredElementKind, RuntimeError> {
    match kind {
        VariableKind::Real => Ok(StoredElementKind::Numeric),
        VariableKind::Text => Ok(StoredElementKind::Text),
        VariableKind::Boolean => Ok(StoredElementKind::Boolean),
        VariableKind::Graphic => Err(fail(
            "les tableaux de formes ne peuvent pas être enregistrés",
        )),
    }
}

fn variable_kind_from_stored(kind: StoredElementKind) -> VariableKind {
    match kind {
        StoredElementKind::Numeric => VariableKind::Real,
        StoredElementKind::Text => VariableKind::Text,
        StoredElementKind::Boolean => VariableKind::Boolean,
    }
}

fn value_to_stored(value: Value) -> Result<StoredValue, RuntimeError> {
    match value {
        Value::Real(number) => Ok(StoredValue::Real(number)),
        Value::Text(text) => Ok(StoredValue::Text(text)),
        Value::Boolean(value) => Ok(StoredValue::Boolean(value)),
        Value::Array(array) => {
            let array = array.borrow();
            let elements = array
                .elements
                .iter()
                .map(|value| match value {
                    Value::Real(number) => Ok(StoredScalar::Real(*number)),
                    Value::Text(text) => Ok(StoredScalar::Text(text.clone())),
                    Value::Boolean(value) => Ok(StoredScalar::Boolean(*value)),
                    Value::Array(_) | Value::Procedure(_) | Value::Graphic(_) => {
                        Err(fail("un tableau contient une valeur non sérialisable"))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(StoredValue::Array {
                dimensions: array.dimensions.clone(),
                element_kind: stored_kind(array.kind)?,
                elements,
            })
        }
        Value::Procedure(_) => Err(fail("une procédure ne peut pas être enregistrée")),
        Value::Graphic(_) => Err(fail(
            "les formes graphiques ne peuvent pas être enregistrées",
        )),
    }
}

fn value_from_stored(value: StoredValue) -> Result<Value, RuntimeError> {
    match value {
        StoredValue::Real(number) => Ok(Value::Real(number)),
        StoredValue::Text(text) => Ok(Value::Text(text)),
        StoredValue::Boolean(value) => Ok(Value::Boolean(value)),
        StoredValue::Array {
            dimensions,
            element_kind,
            elements,
        } => {
            if dimensions.is_empty() || dimensions.len() > 7 {
                return Err(fail("le tableau du fichier a un rang invalide"));
            }
            let element_count = dimensions.iter().try_fold(1_usize, |product, dimension| {
                if *dimension == 0 {
                    None
                } else {
                    product.checked_mul(*dimension)
                }
            });
            let element_count = element_count
                .filter(|count| *count <= MAX_ARRAY_ELEMENTS)
                .ok_or_else(|| fail("le tableau du fichier dépasse la limite d'allocation"))?;
            if elements.len() != element_count {
                return Err(fail(
                    "le nombre d'éléments du tableau enregistré est incorrect",
                ));
            }
            let kind = variable_kind_from_stored(element_kind);
            let values = elements
                .into_iter()
                .map(|element| match (element, kind) {
                    (StoredScalar::Real(number), VariableKind::Real) => Ok(Value::Real(number)),
                    (StoredScalar::Text(text), VariableKind::Text) => Ok(Value::Text(text)),
                    (StoredScalar::Boolean(value), VariableKind::Boolean) => {
                        Ok(Value::Boolean(value))
                    }
                    _ => Err(fail(
                        "le type d'un élément du tableau enregistré est incorrect",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::Array(Rc::new(RefCell::new(ArrayValue {
                dimensions,
                kind,
                elements: values,
            }))))
        }
    }
}

#[derive(Clone, Debug)]
struct ArrayDeclaration {
    name: String,
    kind: VariableKind,
    dimensions: Vec<Expr>,
}

#[derive(Clone, Debug)]
struct Procedure {
    name: String,
    parameters: Vec<String>,
    by_value: HashSet<String>,
    procedure_parameters: HashSet<String>,
    locals: HashSet<String>,
    string_variables: HashSet<String>,
    boolean_variables: HashSet<String>,
    graphic_variables: HashMap<String, GraphicType>,
    array_variables: HashMap<String, VariableKind>,
    lines: Vec<ProgramLine>,
    labels: HashMap<i64, usize>,
}

#[derive(Clone, Debug)]
struct RawProgramLine {
    label: i64,
    source_line: usize,
    body: String,
}

#[derive(Clone, Debug)]
struct ProcedureHeader {
    name: String,
    parameters: Vec<String>,
    by_value: HashSet<String>,
    procedure_parameters: HashSet<String>,
    locals: HashSet<String>,
}

fn numbered_source_lines(source: &str) -> Result<Vec<RawProgramLine>, Vec<Diagnostic>> {
    let mut lines = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let source_line = index + 1;
        let mut text = raw.trim();
        if text.is_empty() || text.starts_with('*') {
            continue;
        }
        if let Some(star) = text.find('*') {
            if text[..star].trim().parse::<i64>().is_ok() {
                continue;
            }
        }
        if let Some(rest) = text.strip_prefix('?') {
            text = rest.trim();
        }
        let (prefix, body) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(prefix, rest)| (prefix, rest.trim()));
        let label = prefix.parse::<i64>().map_err(|_| {
            vec![Diagnostic {
                span: Span {
                    line: source_line,
                    column: 1,
                },
                message: "un numéro de ligne est requis avant chaque instruction".into(),
            }]
        })?;
        if !(1..=32_767).contains(&label) {
            return Err(vec![Diagnostic {
                span: Span {
                    line: source_line,
                    column: 1,
                },
                message: "le numéro de ligne doit être compris entre 1 et 32767".into(),
            }]);
        }
        if body.starts_with('*') {
            continue;
        }
        lines.push(RawProgramLine {
            label,
            source_line,
            body: body.to_owned(),
        });
    }
    Ok(lines)
}

fn is_procedure_header(source: &str) -> bool {
    let source = source.trim_start();
    source
        .get(..9)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("PROCEDURE"))
        && source
            .get(9..)
            .and_then(|suffix| suffix.chars().next())
            .is_some_and(char::is_whitespace)
}

fn parse_procedure_header(source: &str, line: usize) -> Result<ProcedureHeader, Vec<Diagnostic>> {
    let mut rest = source.trim()["PROCEDURE".len()..].trim_start();
    let Some(rest_after_ampersand) = rest.strip_prefix('&') else {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "un nom de procédure précédé de & est attendu".into(),
        }]);
    };
    rest = rest_after_ampersand;
    let Some(open) = rest.find('(') else {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "une liste de paramètres entre parenthèses est attendue".into(),
        }]);
    };
    let name = rest[..open].trim();
    if !valid_identifier(name) {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: format!("identificateur de procédure invalide `{name}`"),
        }]);
    }
    let Some(close) = rest[open + 1..].find(')').map(|index| open + 1 + index) else {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: ") attendu dans la déclaration de procédure".into(),
        }]);
    };
    let mut parameters = Vec::new();
    let mut procedure_parameters = HashSet::new();
    for parameter in split_commas(&rest[open + 1..close]) {
        let parameter = parameter.trim();
        if parameter.is_empty() {
            continue;
        }
        let is_procedure = parameter.starts_with('&');
        let name = parameter.trim_start_matches('&').trim();
        if !valid_identifier(name) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!("paramètre de procédure invalide `{parameter}`"),
            }]);
        }
        let key = name.to_ascii_lowercase();
        if parameters.contains(&key) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!("paramètre `{name}` déclaré plusieurs fois"),
            }]);
        }
        if is_procedure {
            procedure_parameters.insert(key.clone());
        }
        parameters.push(key);
    }
    let trailing = rest[close + 1..].trim();
    let mut locals = HashSet::new();
    if !trailing.is_empty() {
        let Some(local_list) = trailing
            .get(..5)
            .filter(|prefix| prefix.eq_ignore_ascii_case("LOCAL"))
            .and_then(|_| trailing.get(5..))
            .filter(|suffix| suffix.starts_with(char::is_whitespace))
        else {
            return Err(vec![Diagnostic {
                span: Span {
                    line,
                    column: close + 2,
                },
                message: "seule une liste LOCAL est admise après les paramètres".into(),
            }]);
        };
        for local in split_commas(local_list) {
            let local = local.trim();
            if local.is_empty() {
                continue;
            }
            if !valid_identifier(local) {
                return Err(vec![Diagnostic {
                    span: Span {
                        line,
                        column: close + 2,
                    },
                    message: format!("identificateur local invalide `{local}`"),
                }]);
            }
            locals.insert(local.to_ascii_lowercase());
        }
    }
    let by_value = parameters
        .iter()
        .filter(|parameter| locals.contains(*parameter))
        .cloned()
        .collect();
    locals.extend(parameters.iter().cloned());
    Ok(ProcedureHeader {
        name: name.to_ascii_lowercase(),
        parameters,
        by_value,
        procedure_parameters,
        locals,
    })
}

fn label_map(lines: &[ProgramLine]) -> Result<HashMap<i64, usize>, Vec<Diagnostic>> {
    let mut labels = HashMap::new();
    for (index, line) in lines.iter().enumerate() {
        if labels.insert(line.label, index).is_some() {
            return Err(vec![Diagnostic {
                span: Span {
                    line: line.source_line,
                    column: 1,
                },
                message: format!("étiquette {} déclarée deux fois", line.label),
            }]);
        }
    }
    Ok(labels)
}

fn parse_procedures(
    raw_lines: &[RawProgramLine],
) -> Result<(HashMap<String, Procedure>, HashSet<usize>), Vec<Diagnostic>> {
    let mut procedures = HashMap::new();
    let mut definition_lines = HashSet::new();
    let mut index = 0;
    while index < raw_lines.len() {
        if !is_procedure_header(&raw_lines[index].body) {
            index += 1;
            continue;
        }
        let header_line = &raw_lines[index];
        let header = parse_procedure_header(&header_line.body, header_line.source_line)?;
        if procedures.contains_key(&header.name) {
            return Err(vec![Diagnostic {
                span: Span {
                    line: header_line.source_line,
                    column: 1,
                },
                message: format!("procédure `&{}` déclarée deux fois", header.name),
            }]);
        }
        definition_lines.insert(header_line.source_line);
        index += 1;
        let mut body = Vec::new();
        while index < raw_lines.len() && !is_procedure_header(&raw_lines[index].body) {
            let line = raw_lines[index].clone();
            definition_lines.insert(line.source_line);
            let ends_procedure = line.body.trim().eq_ignore_ascii_case("RETOUR");
            body.push(line);
            index += 1;
            if ends_procedure {
                break;
            }
        }
        let mut strings = HashSet::new();
        let mut booleans = HashSet::new();
        let mut graphics = HashMap::new();
        let mut arrays = HashMap::new();
        let mut lines = Vec::new();
        for raw in body {
            let mut statements = Vec::new();
            for part in split_statements(&raw.body) {
                if part.trim().is_empty() {
                    continue;
                }
                statements.push(parse_statement(
                    part.trim(),
                    raw.source_line,
                    &mut strings,
                    &mut booleans,
                    &mut graphics,
                    &mut arrays,
                )?);
            }
            lines.push(ProgramLine {
                label: raw.label,
                source_line: raw.source_line,
                statements,
            });
        }
        lines.sort_by_key(|line| line.label);
        let labels = label_map(&lines)?;
        let mut locals = header.locals;
        locals.extend(strings.iter().cloned());
        locals.extend(booleans.iter().cloned());
        locals.extend(graphics.keys().cloned());
        locals.extend(arrays.keys().cloned());
        procedures.insert(
            header.name.clone(),
            Procedure {
                name: header.name,
                parameters: header.parameters,
                by_value: header.by_value,
                procedure_parameters: header.procedure_parameters,
                locals,
                string_variables: strings,
                boolean_variables: booleans,
                graphic_variables: graphics,
                array_variables: arrays,
                lines,
                labels,
            },
        );
    }
    Ok((procedures, definition_lines))
}

#[derive(Clone, Debug)]
struct ForLoopState {
    body_label: i64,
    variable: String,
    current: f64,
    step: f64,
    step_expression: Expr,
    limit: Option<f64>,
    limit_expression: Option<Expr>,
    condition: Option<Expr>,
}

pub fn parse(source: &str) -> Result<Program, Vec<Diagnostic>> {
    let raw_lines = numbered_source_lines(source)?;
    let (procedures, definition_lines) = parse_procedures(&raw_lines)?;
    let mut lines = Vec::new();
    let mut strings = HashSet::new();
    let mut booleans = HashSet::new();
    let mut graphics = HashMap::new();
    let mut arrays = HashMap::new();
    for raw in raw_lines {
        if definition_lines.contains(&raw.source_line) {
            continue;
        }
        let mut statements = Vec::new();
        for part in split_statements(&raw.body) {
            if part.trim().is_empty() {
                continue;
            }
            let statement = parse_statement(
                part.trim(),
                raw.source_line,
                &mut strings,
                &mut booleans,
                &mut graphics,
                &mut arrays,
            )?;
            statements.push(statement);
        }
        lines.push(ProgramLine {
            label: raw.label,
            source_line: raw.source_line,
            statements,
        });
    }
    lines.sort_by_key(|line| line.label);
    let labels = label_map(&lines)?;
    if lines.is_empty() {
        return Err(vec![Diagnostic {
            span: Span::default(),
            message: "programme LSE vide".into(),
        }]);
    }
    Ok(Program {
        lines,
        string_variables: strings,
        boolean_variables: booleans,
        graphic_variables: graphics,
        array_variables: arrays,
        procedures,
        labels,
    })
}

fn parse_procedure_definitions(
    source: &str,
) -> Result<HashMap<String, Procedure>, Vec<Diagnostic>> {
    let raw_lines = numbered_source_lines(source)?;
    parse_procedures(&raw_lines).map(|(procedures, _)| procedures)
}

fn split_statements(source: &str) -> Vec<&str> {
    let (mut parts, mut start, mut depth, mut blocks, mut quote) =
        (Vec::new(), 0_usize, 0_i32, 0_i32, false);
    let b = source.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c == '\'' {
            if quote && b.get(i + 1) == Some(&(b'\'')) {
                i += 2;
                continue;
            }
            quote = !quote;
        } else if !quote {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ';' if depth == 0 => {
                    if blocks == 0 {
                        parts.push(&source[start..i]);
                        start = i + 1;
                    }
                }
                _ if depth == 0 && c.is_ascii_alphabetic() => {
                    let token_start = i;
                    while i < b.len() && (b[i] as char).is_ascii_alphanumeric() {
                        i += 1;
                    }
                    let token = &source[token_start..i];
                    if token.eq_ignore_ascii_case("DEBUT") {
                        blocks += 1;
                    } else if token.eq_ignore_ascii_case("FIN") {
                        blocks -= 1;
                    }
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    parts.push(&source[start..]);
    parts
}

fn matching_block_end(source: &str, after_open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 1_usize;
    let mut quote = false;
    let mut i = after_open;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if quote && bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            quote = !quote;
            i += 1;
            continue;
        }
        if quote {
            i += 1;
            continue;
        }
        if (bytes[i] as char).is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i] as char).is_ascii_alphanumeric() {
                i += 1;
            }
            let token = &source[start..i];
            if token.eq_ignore_ascii_case("DEBUT") {
                depth += 1;
            } else if token.eq_ignore_ascii_case("FIN") {
                depth -= 1;
                if depth == 0 {
                    return Some(start);
                }
            }
            continue;
        }
        i += 1;
    }
    None
}

fn parse_statement(
    source: &str,
    line: usize,
    strings: &mut HashSet<String>,
    booleans: &mut HashSet<String>,
    graphics: &mut HashMap<String, GraphicType>,
    arrays: &mut HashMap<String, VariableKind>,
) -> Result<Stmt, Vec<Diagnostic>> {
    let upper = source.to_ascii_uppercase();
    if upper == "TERMINER" {
        return Ok(Stmt::Terminate);
    }
    if upper == "RETOUR" {
        return Ok(Stmt::Return(None));
    }
    if upper.starts_with("RESULTAT ") {
        let expression = ExprParser::parse(source["RESULTAT".len()..].trim(), line)
            .map_err(|error| vec![error])?;
        return Ok(Stmt::Return(Some(expression)));
    }
    if upper.starts_with("TABLEAU ") {
        return parse_array_declaration(source, line, strings, booleans, graphics, arrays);
    }
    let starts_block = upper.get(..5).is_some_and(|word| word == "DEBUT")
        && upper
            .as_bytes()
            .get(5)
            .is_none_or(|byte| byte.is_ascii_whitespace() || *byte == b';');
    if starts_block {
        let inner_start = "DEBUT".len();
        let Some(end) = matching_block_end(source, inner_start) else {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "FIN attendu pour fermer DEBUT".into(),
            }]);
        };
        if !source[end + "FIN".len()..].trim().is_empty() {
            return Err(vec![Diagnostic {
                span: Span {
                    line,
                    column: end + "FIN".len() + 1,
                },
                message: "texte inattendu après FIN".into(),
            }]);
        }
        let mut statements = Vec::new();
        for part in split_statements(&source[inner_start..end]) {
            if part.trim().is_empty() {
                continue;
            }
            statements.push(parse_statement(
                part.trim(),
                line,
                strings,
                booleans,
                graphics,
                arrays,
            )?);
        }
        return Ok(Stmt::Block(statements));
    }
    if upper.starts_with("CHAINE ") {
        let mut declared = Vec::new();
        for name in split_commas(&source[7..]) {
            let name = name.trim();
            if !valid_identifier(name) {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 8 },
                    message: "identificateur chaîne invalide".into(),
                }]);
            }
            let key = name.to_ascii_lowercase();
            if arrays.contains_key(&key)
                || booleans.contains(&key)
                || graphics.contains_key(&key)
                || !strings.insert(key)
            {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 8 },
                    message: format!("`{name}` est déjà déclaré"),
                }]);
            }
            declared.push(name.to_ascii_lowercase());
        }
        return Ok(Stmt::Declaration(VariableKind::Text, declared));
    }
    if upper.starts_with("BOOLEEN ") {
        let mut declared = Vec::new();
        for name in split_commas(&source[8..]) {
            let name = name.trim();
            if !valid_identifier(name) {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 9 },
                    message: "identificateur booléen invalide".into(),
                }]);
            }
            let key = name.to_ascii_lowercase();
            if arrays.contains_key(&key)
                || strings.contains(&key)
                || graphics.contains_key(&key)
                || !booleans.insert(key)
            {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 9 },
                    message: format!("`{name}` est déjà déclaré"),
                }]);
            }
            declared.push(name.to_ascii_lowercase());
        }
        return Ok(Stmt::Declaration(VariableKind::Boolean, declared));
    }
    if upper.starts_with("FORME ") || upper.starts_with("PAGE ") {
        let kind = if upper.starts_with("FORME ") {
            GraphicType::Form
        } else {
            GraphicType::Page
        };
        let length = if kind == GraphicType::Form {
            "FORME".len()
        } else {
            "PAGE".len()
        };
        let mut declared = Vec::new();
        for name in split_commas(source[length..].trim()) {
            let name = name.trim();
            let key = name.to_ascii_lowercase();
            if !valid_identifier(name)
                || strings.contains(&key)
                || booleans.contains(&key)
                || arrays.contains_key(&key)
                || graphics.contains_key(&key)
            {
                return Err(vec![Diagnostic {
                    span: Span {
                        line,
                        column: length + 2,
                    },
                    message: format!("identificateur graphique invalide ou déjà déclaré `{name}`"),
                }]);
            }
            graphics.insert(key.clone(), kind);
            declared.push(key);
        }
        return Ok(Stmt::GraphicDeclaration(kind, declared));
    }
    if upper.starts_with("MOTIF ") {
        let rest = source["MOTIF".len()..].trim();
        let Some((code, form)) = rest.split_once('=') else {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "MOTIF attend `code = forme`".into(),
            }]);
        };
        let code = ExprParser::parse(code.trim(), line).map_err(|error| vec![error])?;
        let form = ExprParser::parse(form.trim(), line).map_err(|error| vec![error])?;
        return Ok(Stmt::Motif(code, form));
    }
    if let Some((name, arguments)) = parse_graphic_command(source, line)? {
        return Ok(Stmt::GraphicCommand(name, arguments));
    }
    if upper.starts_with("SI ") {
        let Some(pos) = upper.find(" ALORS ") else {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "ALORS attendu dans SI".into(),
            }]);
        };
        let condition = strip_outer_parentheses(source[3..pos].trim());
        let tail = &source[pos + 7..];
        let (yes, no) = if let Some(split) = find_outer_sinon(tail) {
            (&tail[..split], &tail[split + 5..])
        } else {
            (tail, "")
        };
        let cond = ExprParser::parse(condition, line).map_err(|e| vec![e])?;
        let yes_stmt = parse_statement(yes.trim(), line, strings, booleans, graphics, arrays)?;
        let no_stmt = if no.trim().is_empty() {
            Vec::new()
        } else {
            vec![parse_statement(
                no.trim(),
                line,
                strings,
                booleans,
                graphics,
                arrays,
            )?]
        };
        return Ok(Stmt::If(cond, vec![yes_stmt], no_stmt));
    }
    if upper.starts_with("ALLER EN ") {
        let label = source[9..].trim().parse().map_err(|_| {
            vec![Diagnostic {
                span: Span { line, column: 10 },
                message: "étiquette numérique attendue après ALLER EN".into(),
            }]
        })?;
        if !(1..=32_767).contains(&label) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 10 },
                message: "le numéro de ligne doit être compris entre 1 et 32767".into(),
            }]);
        }
        return Ok(Stmt::Jump(label));
    }
    if upper.starts_with("FAIRE ") {
        return parse_for_statement(source, line);
    }
    if upper == "PAUSE" {
        return Ok(Stmt::Pause);
    }
    if upper.starts_with("LIBERER ") {
        let names = split_commas(&source[8..])
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>();
        if names.is_empty() || names.iter().any(|name| !valid_identifier(name)) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 9 },
                message: "LIBERER attend un ou plusieurs identificateurs LSE".into(),
            }]);
        }
        return Ok(Stmt::Free(names.into_iter().map(str::to_owned).collect()));
    }
    if upper.starts_with("GARER ") || upper.starts_with("CHARGER ") {
        let is_store = upper.starts_with("GARER ");
        let keyword_length = if is_store {
            "GARER".len()
        } else {
            "CHARGER".len()
        };
        let arguments = split_commas(source[keyword_length..].trim());
        if !(3..=4).contains(&arguments.len()) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!(
                    "{} attend un objet, une clé, un fichier et un statut numérique optionnel",
                    if is_store { "GARER" } else { "CHARGER" }
                ),
            }]);
        }
        let name = arguments[0].trim();
        if !valid_identifier(name) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!("identificateur d'objet invalide `{name}`"),
            }]);
        }
        let key = ExprParser::parse(arguments[1].trim(), line).map_err(|error| vec![error])?;
        let file = ExprParser::parse(arguments[2].trim(), line).map_err(|error| vec![error])?;
        let status = if let Some(argument) = arguments.get(3) {
            let name = argument.trim();
            if !valid_identifier(name) {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "identificateur de statut invalide".into(),
                }]);
            }
            Some(name.to_owned())
        } else {
            None
        };
        return Ok(if is_store {
            Stmt::StoreRecord {
                name: name.to_owned(),
                key,
                file,
                status,
            }
        } else {
            Stmt::LoadRecord {
                name: name.to_owned(),
                key,
                file,
                status,
            }
        });
    }
    if upper.starts_with("SUPPRIMER ") {
        let arguments = split_commas(source["SUPPRIMER".len()..].trim());
        if !(1..=2).contains(&arguments.len()) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "SUPPRIMER attend un fichier et une clé d'enregistrement optionnelle"
                    .into(),
            }]);
        }
        let file = ExprParser::parse(arguments[0].trim(), line).map_err(|error| vec![error])?;
        let key = arguments
            .get(1)
            .map(|argument| ExprParser::parse(argument.trim(), line))
            .transpose()
            .map_err(|error| vec![error])?;
        return Ok(Stmt::DeleteRecords { file, key });
    }
    if upper.starts_with("EXECUTER ") {
        let arguments = split_commas(source["EXECUTER".len()..].trim());
        if !(2..=3).contains(&arguments.len()) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "EXECUTER attend un nom de programme, un argument numérique et un statut numérique optionnel".into(),
            }]);
        }
        let program = ExprParser::parse(arguments[0].trim(), line).map_err(|error| vec![error])?;
        let argument = ExprParser::parse(arguments[1].trim(), line).map_err(|error| vec![error])?;
        let status = if let Some(argument) = arguments.get(2) {
            let name = argument.trim();
            if !valid_identifier(name) {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "identificateur de statut invalide".into(),
                }]);
            }
            Some(name.to_owned())
        } else {
            None
        };
        return Ok(Stmt::ExecuteProgram {
            program,
            argument,
            status,
        });
    }
    if upper.starts_with("LIRE ") {
        let mut rest = source[5..].trim();
        let prompt = if rest.starts_with('[') {
            let Some(end) = find_format_end(rest) else {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "] attendu pour fermer le format LIRE".into(),
                }]);
            };
            let formats = parse_display_formats(&rest[1..end], line)?;
            if formats.iter().any(|format| {
                matches!(format.repeat, FormatRepeat::Argument)
                    || matches!(format.operation, FormatOperation::Value(_))
            }) {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "le format LIRE attend des commandes de sortie fixes".into(),
                }]);
            }
            rest = rest[end + 1..].trim();
            formats
        } else {
            Vec::new()
        };
        let names = split_commas(rest)
            .into_iter()
            .map(str::trim)
            .collect::<Vec<_>>();
        if names.is_empty() || names.iter().any(|name| !valid_identifier(name)) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 6 },
                message: "LIRE attend un ou plusieurs identificateurs LSE".into(),
            }]);
        }
        return Ok(Stmt::Read {
            names: names.into_iter().map(str::to_owned).collect(),
            prompt,
        });
    }
    if upper.starts_with("AFFICHER") {
        return parse_display(source, line);
    }
    if let Some((pos, operator_len)) = assignment_position(source) {
        let name = source[..pos].trim();
        if name.contains('[') {
            let target = ExprParser::parse(name, line).map_err(|error| vec![error])?;
            let Expr::Index(array, indices) = target else {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "left side of assignment must be a variable or table element".into(),
                }]);
            };
            let expr = ExprParser::parse(source[pos + operator_len..].trim(), line)
                .map_err(|error| vec![error])?;
            return Ok(Stmt::ArrayAssign(array, indices, expr));
        }
        if !valid_identifier(name) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "identificateur invalide à gauche de l'affectation".into(),
            }]);
        }
        if arrays.contains_key(&name.to_ascii_lowercase()) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!("`{name}` est un tableau; un indice est requis"),
            }]);
        }
        let expr =
            ExprParser::parse(source[pos + operator_len..].trim(), line).map_err(|e| vec![e])?;
        return Ok(Stmt::Assign(name.to_owned(), expr));
    }
    if source.trim_start().starts_with('&') {
        let expression = ExprParser::parse(source, line).map_err(|error| vec![error])?;
        if let Expr::ProcedureCall(name, arguments) = expression {
            return Ok(Stmt::ProcedureCall(name, arguments));
        }
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "un appel de procédure est attendu".into(),
        }]);
    }
    Err(vec![Diagnostic {
        span: Span { line, column: 1 },
        message: "instruction LSE non reconnue ou non couverte par les sources".into(),
    }])
}

fn parse_array_declaration(
    source: &str,
    line: usize,
    strings: &HashSet<String>,
    booleans: &HashSet<String>,
    graphics: &HashMap<String, GraphicType>,
    arrays: &mut HashMap<String, VariableKind>,
) -> Result<Stmt, Vec<Diagnostic>> {
    let mut rest = source["TABLEAU".len()..].trim_start();
    let upper = rest.to_ascii_uppercase();
    let (kind, length) = if upper.starts_with("CHAINE")
        && upper["CHAINE".len()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    {
        (VariableKind::Text, "CHAINE".len())
    } else if upper.starts_with("BOOLEEN")
        && upper["BOOLEEN".len()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    {
        (VariableKind::Boolean, "BOOLEEN".len())
    } else if upper.starts_with("FORME")
        && upper["FORME".len()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    {
        (VariableKind::Graphic, "FORME".len())
    } else {
        (VariableKind::Real, 0)
    };
    rest = rest[length..].trim();
    let mut declarations = Vec::new();
    for part in split_commas(rest) {
        let part = part.trim();
        let Some(open) = part.find('[') else {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "each TABLEAU name requires one or more dimensions".into(),
            }]);
        };
        if !part.ends_with(']') || part[open + 1..part.len() - 1].contains('[') {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "malformed TABLEAU dimensions".into(),
            }]);
        }
        let name = part[..open].trim();
        if !valid_identifier(name) {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!("identificateur de tableau invalide `{name}`"),
            }]);
        }
        let key = name.to_ascii_lowercase();
        if strings.contains(&key)
            || booleans.contains(&key)
            || graphics.contains_key(&key)
            || arrays.contains_key(&key)
        {
            return Err(vec![Diagnostic {
                span: Span { line, column: 1 },
                message: format!("`{name}` est déjà déclaré"),
            }]);
        }
        let dimension_sources = split_commas(&part[open + 1..part.len() - 1]);
        if dimension_sources.is_empty() || dimension_sources.len() > 7 {
            return Err(vec![Diagnostic {
                span: Span {
                    line,
                    column: open + 2,
                },
                message: "a LSE table must have between one and seven dimensions".into(),
            }]);
        }
        let mut dimensions = Vec::new();
        for dimension in dimension_sources {
            dimensions
                .push(ExprParser::parse(dimension.trim(), line).map_err(|error| vec![error])?);
        }
        arrays.insert(key, kind);
        declarations.push(ArrayDeclaration {
            name: name.to_owned(),
            kind,
            dimensions,
        });
    }
    if declarations.is_empty() {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "TABLEAU requires at least one declared name".into(),
        }]);
    }
    Ok(Stmt::ArrayDeclaration(declarations))
}

fn parse_graphic_command(
    source: &str,
    line: usize,
) -> Result<Option<(String, Vec<Expr>)>, Vec<Diagnostic>> {
    let Some((name, rest)) = source.trim().split_once(char::is_whitespace) else {
        return Ok(None);
    };
    let expected = match name.to_ascii_uppercase().as_str() {
        "TRACE" => 1,
        "INSERER" => 2,
        "CADRER" => 5,
        "TRANSFERER" => 2,
        "MARGER" => 5,
        "ALLUMER" | "ETEINDRE" | "NETTOYER" => 1,
        "CIBLE" => 2,
        _ => return Ok(None),
    };
    let parts = split_commas(rest.trim());
    if parts.len() != expected {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: format!("{name} attend {expected} paramètre(s)"),
        }]);
    }
    if name.eq_ignore_ascii_case("CIBLE") {
        let identifiers = parts
            .into_iter()
            .map(str::trim)
            .map(|identifier| {
                if valid_identifier(identifier) {
                    Ok(Expr::Text(identifier.to_ascii_lowercase()))
                } else {
                    Err(Diagnostic {
                        span: Span { line, column: 1 },
                        message: format!("identificateur attendu dans CIBLE: `{identifier}`"),
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|diagnostic| vec![diagnostic])?;
        return Ok(Some((name.to_ascii_uppercase(), identifiers)));
    }
    let arguments = parts
        .into_iter()
        .map(|part| ExprParser::parse(part.trim(), line))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|diagnostic| vec![diagnostic])?;
    Ok(Some((name.to_ascii_uppercase(), arguments)))
}

fn parse_for_statement(source: &str, line: usize) -> Result<Stmt, Vec<Diagnostic>> {
    let rest = source["FAIRE ".len()..].trim();
    let Some((target, header)) = rest.split_once(char::is_whitespace) else {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "étiquette attendue après FAIRE".into(),
        }]);
    };
    let body_label = target.trim().parse().map_err(|_| {
        vec![Diagnostic {
            span: Span { line, column: 7 },
            message: "étiquette numérique attendue après FAIRE".into(),
        }]
    })?;
    if !(1..=32_767).contains(&body_label) {
        return Err(vec![Diagnostic {
            span: Span { line, column: 7 },
            message: "le numéro de ligne doit être compris entre 1 et 32767".into(),
        }]);
    }
    let header = header.trim();
    if !header.to_ascii_uppercase().starts_with("POUR ") {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "POUR attendu dans l'instruction FAIRE".into(),
        }]);
    }
    let header = header["POUR ".len()..].trim();
    let Some(arrow) = for_assignment_position(header) else {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "affectation de variable attendue après POUR".into(),
        }]);
    };
    let variable = header[..arrow].trim();
    if !valid_identifier(variable) {
        return Err(vec![Diagnostic {
            span: Span { line, column: 1 },
            message: "variable de boucle invalide".into(),
        }]);
    }
    let after_assignment = if header[arrow..].starts_with('←') {
        &header[arrow + '←'.len_utf8()..]
    } else if header[arrow..].starts_with("<-") {
        &header[arrow + 2..]
    } else {
        &header[arrow + 1..]
    }
    .trim();
    let upper_after = after_assignment.to_ascii_uppercase();
    let pas_at = upper_after.find(" PAS ");
    let (start_text, step_expression, clause_source, clause_is_until) =
        if let Some(step_at) = pas_at {
            let clause = &after_assignment[step_at + " PAS ".len()..];
            let (clause_at, clause_len, is_until) = find_loop_clause(clause).ok_or_else(|| {
                vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "JUSQUA ou TANT QUE attendu après le pas".into(),
                }]
            })?;
            (
                &after_assignment[..step_at],
                ExprParser::parse(clause[..clause_at].trim(), line).map_err(|error| vec![error])?,
                &clause[clause_at + clause_len..],
                is_until,
            )
        } else {
            let (clause_at, clause_len, is_until) =
                find_loop_clause(after_assignment).ok_or_else(|| {
                    vec![Diagnostic {
                        span: Span { line, column: 1 },
                        message: "JUSQUA ou TANT QUE attendu après POUR".into(),
                    }]
                })?;
            (
                &after_assignment[..clause_at],
                Expr::Real(1.0),
                &after_assignment[clause_at + clause_len..],
                is_until,
            )
        };
    let start = ExprParser::parse(start_text.trim(), line).map_err(|error| vec![error])?;
    let (limit, condition) = if clause_is_until {
        (
            Some(ExprParser::parse(clause_source.trim(), line).map_err(|error| vec![error])?),
            None,
        )
    } else {
        (
            None,
            Some(ExprParser::parse(clause_source.trim(), line).map_err(|error| vec![error])?),
        )
    };
    Ok(Stmt::For {
        body_label,
        variable: variable.to_owned(),
        start,
        step: step_expression,
        limit,
        condition,
    })
}

fn find_loop_clause(source: &str) -> Option<(usize, usize, bool)> {
    let upper = source.to_ascii_uppercase();
    let until = upper
        .find(" JUSQUA ")
        .map(|index| (index, " JUSQUA ".len(), true));
    let while_clause = upper
        .find(" TANT QUE ")
        .map(|index| (index, " TANT QUE ".len(), false));
    match (until, while_clause) {
        (Some(until), Some(while_clause)) => Some(if until.0 <= while_clause.0 {
            until
        } else {
            while_clause
        }),
        (Some(clause), None) | (None, Some(clause)) => Some(clause),
        (None, None) => None,
    }
}

fn valid_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && name.len() <= 5
        && characters.all(|character| character.is_ascii_alphanumeric())
}

fn for_assignment_position(source: &str) -> Option<usize> {
    assignment_position(source).map(|(position, _)| position)
}
fn find_outer_sinon(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut depth = 0_i32;
    let mut nested_if = 0_usize;
    let mut quote = false;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if quote && bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            quote = !quote;
            i += 1;
            continue;
        }
        if quote {
            i += 1;
            continue;
        }
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
            let rest = &source[i..];
            if rest.len() >= 2
                && rest[..2].eq_ignore_ascii_case("SI")
                && rest.as_bytes().get(2).is_some_and(u8::is_ascii_whitespace)
            {
                nested_if += 1;
                i += 2;
                continue;
            }
            if rest.len() >= 5
                && rest[..5].eq_ignore_ascii_case("SINON")
                && rest.as_bytes().get(5).is_none_or(u8::is_ascii_whitespace)
            {
                if nested_if == 0 {
                    return Some(i);
                }
                nested_if -= 1;
                i += 5;
                continue;
            }
        }
        i += 1;
    }
    None
}
fn strip_outer_parentheses(source: &str) -> &str {
    let trimmed = source.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return trimmed;
    }
    let mut depth = 0_i32;
    let mut quote = false;
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if quote && bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            quote = !quote;
        } else if !quote {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 && i + 1 != bytes.len() {
                        return trimmed;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    if depth == 0 {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}
fn assignment_position(source: &str) -> Option<(usize, usize)> {
    let (mut depth, mut quote) = (0_i32, false);
    for (i, c) in source.char_indices() {
        if c == '\'' {
            quote = !quote;
        } else if !quote {
            if depth == 0 && c == '←' {
                return Some((i, c.len_utf8()));
            }
            if depth == 0 && c == '<' && source[i..].starts_with("<-") {
                return Some((i, 2));
            }
            if depth == 0 && c == '_' {
                return Some((i, 1));
            }
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                '=' if depth == 0 => {
                    if i > 0 && matches!(source.as_bytes()[i - 1], b'<' | b'>') {
                        continue;
                    }
                    return Some((i, 1));
                }
                _ => {}
            }
        }
    }
    None
}
fn parse_display(source: &str, line: usize) -> Result<Stmt, Vec<Diagnostic>> {
    let body = source.trim()[8..].trim();
    let (formats, implicit_newline, expression_source) = if body.starts_with('[') {
        let end = find_format_end(body).ok_or_else(|| {
            vec![Diagnostic {
                span: Span { line, column: 1 },
                message: "] attendu pour fermer le format AFFICHER".into(),
            }]
        })?;
        (
            parse_display_formats(&body[1..end], line)?,
            false,
            body[end + 1..].trim(),
        )
    } else {
        (Vec::new(), true, body)
    };
    let mut expressions = Vec::new();
    for expression in split_commas(expression_source) {
        if expression.trim().is_empty() {
            continue;
        }
        expressions.push(ExprParser::parse(expression.trim(), line).map_err(|error| vec![error])?);
    }
    Ok(Stmt::Display {
        formats,
        expressions,
        implicit_newline,
    })
}

fn find_format_end(source: &str) -> Option<usize> {
    let mut quoted = false;
    let bytes = source.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == b'\'' {
            if quoted && bytes.get(index + 1) == Some(&b'\'') {
                index += 2;
                continue;
            }
            quoted = !quoted;
        } else if !quoted && bytes[index] == b']' {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn parse_display_formats(source: &str, line: usize) -> Result<Vec<DisplayFormat>, Vec<Diagnostic>> {
    let mut formats = Vec::new();
    for raw in split_commas(source) {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let (repeat, operation_source) = if let Some(rest) = token.strip_prefix('*') {
            (FormatRepeat::Argument, rest.trim())
        } else {
            let digit_count = token.bytes().take_while(u8::is_ascii_digit).count();
            if digit_count > 0 {
                let count = token[..digit_count].parse::<usize>().map_err(|_| {
                    vec![Diagnostic {
                        span: Span { line, column: 1 },
                        message: "facteur de répétition invalide dans AFFICHER".into(),
                    }]
                })?;
                let rest = token[digit_count..].trim();
                if rest.is_empty() {
                    formats.push(DisplayFormat {
                        repeat: FormatRepeat::Fixed(count),
                        operation: FormatOperation::Newline,
                    });
                    continue;
                }
                (FormatRepeat::Fixed(count), rest)
            } else {
                (FormatRepeat::Once, token)
            }
        };
        let upper = operation_source.to_ascii_uppercase();
        let operation = if operation_source == "/" {
            FormatOperation::Newline
        } else if upper == "X" {
            FormatOperation::Spaces
        } else if upper == "C" {
            FormatOperation::CarriageReturn
        } else if upper == "L" {
            FormatOperation::LineFeed
        } else if upper == "U" {
            FormatOperation::Value(ValueFormat::Universal)
        } else if upper.starts_with('F') || upper.starts_with('E') {
            let scientific = upper.starts_with('E');
            let Some((before, after)) = upper[1..].split_once('.') else {
                return Err(vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "le format numérique doit être Fm.n ou Em.n".into(),
                }]);
            };
            let digits_before = before.parse::<usize>().map_err(|_| {
                vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "largeur numérique invalide dans AFFICHER".into(),
                }]
            })?;
            let digits_after = after.parse::<usize>().map_err(|_| {
                vec![Diagnostic {
                    span: Span { line, column: 1 },
                    message: "précision numérique invalide dans AFFICHER".into(),
                }]
            })?;
            FormatOperation::Value(if scientific {
                ValueFormat::Scientific {
                    digits_before,
                    digits_after,
                }
            } else {
                ValueFormat::Fixed {
                    digits_before,
                    digits_after,
                }
            })
        } else {
            match ExprParser::parse(operation_source, line) {
                Ok(Expr::Text(text)) => FormatOperation::Literal(text),
                _ => {
                    return Err(vec![Diagnostic {
                        span: Span { line, column: 1 },
                        message: format!("format AFFICHER non reconnu `{operation_source}`"),
                    }]);
                }
            }
        };
        formats.push(DisplayFormat { repeat, operation });
    }
    Ok(formats)
}
fn split_commas(source: &str) -> Vec<&str> {
    let (mut out, mut start, mut depth, mut quote) = (Vec::new(), 0_usize, 0_i32, false);
    for (i, c) in source.char_indices() {
        if c == '\'' {
            quote = !quote;
        } else if !quote {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(&source[start..i]);
                    start = i + 1
                }
                _ => {}
            }
        }
    }
    out.push(&source[start..]);
    out
}

pub trait Host {
    fn input(&mut self) -> Result<String, String>;
    fn output(&mut self, text: &str) -> Result<(), String>;
    fn newline(&mut self, count: usize) -> Result<(), String>;
    fn pause(&mut self, line: i64) -> Result<(), String>;
    fn date_time(&mut self) -> Result<String, String>;
    fn random_unit(&mut self) -> Result<f64, String>;
    fn store_record(&mut self, _file: &str, _key: f64, _value: StoredValue) -> Result<i64, String> {
        Err("record storage is unavailable in this host".into())
    }
    fn load_record(
        &mut self,
        _file: &str,
        _key: f64,
    ) -> Result<(i64, Option<StoredValue>), String> {
        Err("record loading is unavailable in this host".into())
    }
    fn delete_records(&mut self, _file: &str, _key: Option<f64>) -> Result<i64, String> {
        Err("record deletion is unavailable in this host".into())
    }
    fn execute_program(&mut self, _program: &str, _argument: f64) -> Result<i64, String> {
        Err("external program execution is unavailable in this host".into())
    }
    fn declare_graphic(&mut self, _name: &str, _kind: GraphicType) -> Result<GraphicValue, String> {
        Err("graphics are unavailable in this host".into())
    }
    fn define_motif(&mut self, _code: &str, _form: &GraphicValue) -> Result<(), String> {
        Err("graphics are unavailable in this host".into())
    }
    fn graphics_function(
        &mut self,
        _name: &str,
        _arguments: &[GraphicsArgument],
    ) -> Result<GraphicsResult, String> {
        Err("graphics are unavailable in this host".into())
    }
    fn graphics_operator(
        &mut self,
        _operator: char,
        _left: &GraphicValue,
        _right: &GraphicValue,
    ) -> Result<GraphicValue, String> {
        Err("graphics are unavailable in this host".into())
    }
    fn graphics_command(
        &mut self,
        _name: &str,
        _arguments: &[GraphicsArgument],
    ) -> Result<(), String> {
        Err("graphics are unavailable in this host".into())
    }
    fn system_function(
        &mut self,
        _name: &str,
        _arguments: &[SystemArgument],
    ) -> Result<f64, String> {
        Err("LSE system functions are unavailable in this host".into())
    }
    fn load_procedure_source(&mut self, _name: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Execution {
    pub variables: HashMap<String, Value>,
    pub executed_lines: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub message: String,
}
impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for RuntimeError {}

#[derive(Clone, Debug)]
enum Flow {
    Next,
    Jump(i64),
    Return(Option<Value>),
    Terminate,
}

#[derive(Clone, Debug)]
enum VariableLocation {
    Global(String),
    Frame(usize, String),
    ArrayElement(ArrayReference, usize),
}

#[derive(Clone, Debug)]
struct CallFrame {
    locals: HashSet<String>,
    parameters: HashSet<String>,
    values: HashMap<String, Value>,
    aliases: HashMap<String, VariableLocation>,
    string_variables: HashSet<String>,
    boolean_variables: HashSet<String>,
    graphic_variables: HashMap<String, GraphicType>,
    declared_strings: HashSet<String>,
    declared_booleans: HashSet<String>,
    array_variables: HashMap<String, VariableKind>,
}

const MAX_CALL_DEPTH: usize = 128;

pub struct Interpreter<H: Host> {
    host: H,
    variables: HashMap<String, Value>,
    string_variables: HashSet<String>,
    boolean_variables: HashSet<String>,
    graphic_variables: HashMap<String, GraphicType>,
    declared_strings: HashSet<String>,
    declared_booleans: HashSet<String>,
    array_variables: HashMap<String, VariableKind>,
    procedures: HashMap<String, Procedure>,
    frames: Vec<CallFrame>,
    loop_states: HashMap<(usize, i64), ForLoopState>,
    steps_executed: usize,
    step_limit: usize,
}
impl<H: Host> Interpreter<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            variables: HashMap::new(),
            string_variables: HashSet::new(),
            boolean_variables: HashSet::new(),
            graphic_variables: HashMap::new(),
            declared_strings: HashSet::new(),
            declared_booleans: HashSet::new(),
            array_variables: HashMap::new(),
            procedures: HashMap::new(),
            frames: Vec::new(),
            loop_states: HashMap::new(),
            steps_executed: 0,
            step_limit: 5_000_000,
        }
    }
    pub fn with_step_limit(mut self, limit: usize) -> Self {
        self.step_limit = limit.max(1);
        self
    }
    pub fn host(&self) -> &H {
        &self.host
    }
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
    fn local_frame_index(&self, name: &str) -> Option<usize> {
        let key = name.to_ascii_lowercase();
        self.frames
            .iter()
            .rposition(|frame| frame.locals.contains(&key))
    }
    fn location_for_name(&self, name: &str) -> VariableLocation {
        let key = name.to_ascii_lowercase();
        if let Some(index) = self.local_frame_index(&key) {
            if let Some(location) = self.frames[index].aliases.get(&key) {
                location.clone()
            } else {
                VariableLocation::Frame(index, key)
            }
        } else {
            VariableLocation::Global(key)
        }
    }
    fn read_location(&self, location: &VariableLocation) -> Option<Value> {
        match location {
            VariableLocation::Global(name) => self.variables.get(name).cloned(),
            VariableLocation::Frame(index, name) => {
                self.frames.get(*index)?.values.get(name).cloned()
            }
            VariableLocation::ArrayElement(array, offset) => {
                array.borrow().elements.get(*offset).cloned()
            }
        }
    }
    fn get_variable(&self, name: &str) -> Option<Value> {
        self.read_location(&self.location_for_name(name))
    }
    fn graphic_type_for_location(&self, location: &VariableLocation) -> Option<GraphicType> {
        match location {
            VariableLocation::Global(name) => self.graphic_variables.get(name).copied(),
            VariableLocation::Frame(index, name) => {
                let frame = self.frames.get(*index)?;
                if let Some(alias) = frame.aliases.get(name) {
                    self.graphic_type_for_location(alias)
                } else {
                    frame.graphic_variables.get(name).copied()
                }
            }
            VariableLocation::ArrayElement(array, _) => {
                (array.borrow().kind == VariableKind::Graphic).then_some(GraphicType::Form)
            }
        }
    }
    fn write_location(
        &mut self,
        location: &VariableLocation,
        value: Value,
    ) -> Result<(), RuntimeError> {
        if let Value::Graphic(Some(object)) = &value {
            if self.graphic_type_for_location(location) != Some(object.kind) {
                return Err(fail("le type graphique ne correspond pas à la déclaration"));
            }
        }
        match location {
            VariableLocation::Global(name) => {
                self.variables.insert(name.clone(), value);
            }
            VariableLocation::Frame(index, name) => {
                self.frames
                    .get_mut(*index)
                    .ok_or_else(|| fail("contexte local de procédure invalide"))?
                    .values
                    .insert(name.clone(), value);
            }
            VariableLocation::ArrayElement(array, offset) => {
                let mut array = array.borrow_mut();
                if !value_matches_kind(&value, array.kind) {
                    return Err(fail(format!(
                        "les éléments du tableau doivent être de type {}",
                        array.kind.name()
                    )));
                }
                let cell = array
                    .elements
                    .get_mut(*offset)
                    .ok_or_else(|| fail("indice de tableau hors limites"))?;
                *cell = value;
            }
        }
        Ok(())
    }
    fn set_variable(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        let key = name.to_ascii_lowercase();
        let location = self.location_for_name(&key);
        self.write_location(&location, value)
    }
    fn remove_variable(&mut self, name: &str) {
        let key = name.to_ascii_lowercase();
        let location = self.location_for_name(&key);
        match location {
            VariableLocation::Global(name) => {
                self.variables.remove(&name);
            }
            VariableLocation::Frame(index, name) => {
                if let Some(frame) = self.frames.get_mut(index) {
                    frame.values.remove(&name);
                }
            }
            VariableLocation::ArrayElement(_, _) => {}
        }
    }
    fn kind_of_location(&self, location: &VariableLocation) -> Option<VariableKind> {
        match location {
            VariableLocation::Global(name) => {
                if self.graphic_variables.contains_key(name)
                    || matches!(self.variables.get(name), Some(Value::Graphic(_)))
                {
                    Some(VariableKind::Graphic)
                } else if self.string_variables.contains(name) {
                    Some(VariableKind::Text)
                } else if self.boolean_variables.contains(name) {
                    Some(VariableKind::Boolean)
                } else {
                    Some(VariableKind::Real)
                }
            }
            VariableLocation::Frame(index, name) => {
                let frame = self.frames.get(*index)?;
                if let Some(alias) = frame.aliases.get(name) {
                    self.kind_of_location(alias)
                } else if frame.graphic_variables.contains_key(name)
                    || matches!(frame.values.get(name), Some(Value::Graphic(_)))
                {
                    Some(VariableKind::Graphic)
                } else if frame.string_variables.contains(name)
                    || matches!(frame.values.get(name), Some(Value::Text(_)))
                {
                    Some(VariableKind::Text)
                } else if frame.boolean_variables.contains(name)
                    || matches!(frame.values.get(name), Some(Value::Boolean(_)))
                {
                    Some(VariableKind::Boolean)
                } else if matches!(frame.values.get(name), Some(Value::Procedure(_))) {
                    None
                } else {
                    Some(VariableKind::Real)
                }
            }
            VariableLocation::ArrayElement(array, _) => Some(array.borrow().kind),
        }
    }
    fn variable_kind(&self, name: &str) -> VariableKind {
        let key = name.to_ascii_lowercase();
        self.kind_of_location(&self.location_for_name(&key))
            .unwrap_or(VariableKind::Real)
    }
    fn location_is_declared_as(&self, location: &VariableLocation, kind: VariableKind) -> bool {
        match location {
            VariableLocation::Global(name) => match kind {
                VariableKind::Text => self.declared_strings.contains(name),
                VariableKind::Boolean => self.declared_booleans.contains(name),
                VariableKind::Graphic => self.graphic_variables.contains_key(name),
                VariableKind::Real => true,
            },
            VariableLocation::Frame(index, name) => {
                let Some(frame) = self.frames.get(*index) else {
                    return false;
                };
                match kind {
                    VariableKind::Text if frame.declared_strings.contains(name) => true,
                    VariableKind::Boolean if frame.declared_booleans.contains(name) => true,
                    VariableKind::Graphic if frame.graphic_variables.contains_key(name) => true,
                    VariableKind::Text | VariableKind::Boolean => {
                        if let Some(alias) = frame.aliases.get(name) {
                            self.location_is_declared_as(alias, kind)
                        } else {
                            matches!(
                                (frame.values.get(name), kind),
                                (Some(Value::Text(_)), VariableKind::Text)
                                    | (Some(Value::Boolean(_)), VariableKind::Boolean)
                            )
                        }
                    }
                    VariableKind::Graphic => {
                        if let Some(alias) = frame.aliases.get(name) {
                            self.location_is_declared_as(alias, kind)
                        } else {
                            matches!(frame.values.get(name), Some(Value::Graphic(_)))
                        }
                    }
                    VariableKind::Real => true,
                }
            }
            VariableLocation::ArrayElement(array, _) => array.borrow().kind == kind,
        }
    }
    fn variable_is_declared_as(&self, name: &str, kind: VariableKind) -> bool {
        self.location_is_declared_as(&self.location_for_name(name), kind)
    }
    fn array_is_declared(&self, name: &str) -> bool {
        let key = name.to_ascii_lowercase();
        if let Some(index) = self.local_frame_index(&key) {
            self.frames[index].array_variables.contains_key(&key)
                || matches!(self.get_variable(&key), Some(Value::Array(_)))
        } else {
            self.array_variables.contains_key(&key)
                || matches!(self.variables.get(&key), Some(Value::Array(_)))
        }
    }
    fn array_kind(&self, name: &str) -> Option<VariableKind> {
        let key = name.to_ascii_lowercase();
        if let Some(index) = self.local_frame_index(&key) {
            self.frames[index]
                .array_variables
                .get(&key)
                .copied()
                .or_else(|| match self.get_variable(&key) {
                    Some(Value::Array(array)) => Some(array.borrow().kind),
                    _ => None,
                })
        } else {
            self.array_variables
                .get(&key)
                .copied()
                .or_else(|| match self.variables.get(&key) {
                    Some(Value::Array(array)) => Some(array.borrow().kind),
                    _ => None,
                })
        }
    }
    fn validate_loaded_value(&self, name: &str, value: &Value) -> Result<(), RuntimeError> {
        let compatible = match value {
            Value::Real(_) => self.variable_kind(name) == VariableKind::Real,
            Value::Text(_) => {
                self.variable_kind(name) == VariableKind::Text
                    && self.variable_is_declared_as(name, VariableKind::Text)
            }
            Value::Boolean(_) => {
                self.variable_kind(name) == VariableKind::Boolean
                    && self.variable_is_declared_as(name, VariableKind::Boolean)
            }
            Value::Graphic(value) => {
                self.variable_kind(name) == VariableKind::Graphic
                    && value.as_ref().is_none_or(|object| {
                        self.graphic_type_for_location(&self.location_for_name(name))
                            == Some(object.kind)
                    })
            }
            Value::Array(array) => self.array_kind(name) == Some(array.borrow().kind),
            Value::Procedure(_) => false,
        };
        if compatible {
            Ok(())
        } else {
            Err(fail(format!(
                "le type enregistré ne correspond pas à `{name}`"
            )))
        }
    }
    fn reference_for_argument(
        &mut self,
        expression: &Expr,
    ) -> Result<VariableLocation, RuntimeError> {
        match expression {
            Expr::Variable(name) => Ok(self.location_for_name(name)),
            Expr::Index(name, index_expressions) => {
                let indices = self.eval_array_indices(index_expressions)?;
                let array = self.array_reference(name)?;
                let offset = array.borrow().offset(&indices)?;
                Ok(VariableLocation::ArrayElement(array, offset))
            }
            _ => Err(fail("un paramètre par référence doit être une variable")),
        }
    }
    pub fn run(&mut self, program: &Program) -> Result<Execution, RuntimeError> {
        self.variables.clear();
        self.string_variables = program.string_variables.clone();
        self.boolean_variables = program.boolean_variables.clone();
        self.graphic_variables = program.graphic_variables.clone();
        self.declared_strings.clear();
        self.declared_booleans.clear();
        self.array_variables = program.array_variables.clone();
        self.procedures = program.procedures.clone();
        self.frames.clear();
        self.loop_states.clear();
        self.steps_executed = 0;
        match self.execute_lines(&program.lines, &program.labels)? {
            Flow::Next | Flow::Terminate => {}
            Flow::Return(_) => return Err(fail("RETOUR ou RESULTAT hors d'une procédure")),
            Flow::Jump(label) => return Err(fail(format!("étiquette {label} inconnue"))),
        }
        Ok(Execution {
            variables: self.variables.clone(),
            executed_lines: self.steps_executed,
        })
    }
    fn execute_lines(
        &mut self,
        lines: &[ProgramLine],
        labels: &HashMap<i64, usize>,
    ) -> Result<Flow, RuntimeError> {
        let mut pc = 0;
        while pc < lines.len() {
            self.steps_executed += 1;
            if self.steps_executed > self.step_limit {
                return Err(fail("limite d'exécution dépassée"));
            }
            let line = &lines[pc];
            let mut jumped = false;
            for stmt in &line.statements {
                match self.execute(stmt, line.label)? {
                    Flow::Next => {}
                    Flow::Jump(label) => {
                        pc = *labels
                            .get(&label)
                            .ok_or_else(|| fail(&format!("étiquette {label} inconnue")))?;
                        jumped = true;
                        break;
                    }
                    return_flow @ (Flow::Return(_) | Flow::Terminate) => return Ok(return_flow),
                }
            }
            if !jumped {
                let loop_header = self.loop_states.iter().find_map(|(header, state)| {
                    (header.0 == self.frames.len() && state.body_label == line.label)
                        .then_some(header.1)
                });
                if let Some(header) = loop_header {
                    pc = *labels
                        .get(&header)
                        .ok_or_else(|| fail(&format!("étiquette de boucle {header} inconnue")))?;
                } else {
                    pc += 1;
                }
            }
        }
        Ok(Flow::Next)
    }
    fn execute(&mut self, stmt: &Stmt, line: i64) -> Result<Flow, RuntimeError> {
        match stmt {
            Stmt::Declaration(kind, names) => {
                for name in names {
                    let key = name.to_ascii_lowercase();
                    let local = self.local_frame_index(&key);
                    if let Some(index) = local {
                        let frame = &mut self.frames[index];
                        match kind {
                            VariableKind::Text => {
                                frame.declared_strings.insert(key.clone());
                            }
                            VariableKind::Boolean => {
                                frame.declared_booleans.insert(key.clone());
                            }
                            VariableKind::Graphic => {
                                frame
                                    .graphic_variables
                                    .entry(key.clone())
                                    .or_insert(GraphicType::Form);
                            }
                            VariableKind::Real => {}
                        }
                        if !frame.aliases.contains_key(&key) && !frame.parameters.contains(&key) {
                            frame.values.insert(key, default_value(*kind));
                        }
                    } else {
                        match kind {
                            VariableKind::Text => {
                                self.declared_strings.insert(key.clone());
                            }
                            VariableKind::Boolean => {
                                self.declared_booleans.insert(key.clone());
                            }
                            VariableKind::Graphic => {
                                self.graphic_variables
                                    .entry(key.clone())
                                    .or_insert(GraphicType::Form);
                            }
                            VariableKind::Real => {}
                        }
                        self.variables.insert(key, default_value(*kind));
                    }
                }
                Ok(Flow::Next)
            }
            Stmt::GraphicDeclaration(kind, names) => {
                for name in names {
                    let object = self.host.declare_graphic(name, *kind).map_err(fail)?;
                    if object.handle.is_none() || object.kind != *kind {
                        return Err(fail(format!(
                            "le service graphique a renvoyé une déclaration invalide pour `{name}`"
                        )));
                    }
                    self.set_variable(name, Value::Graphic(Some(object)))?;
                }
                Ok(Flow::Next)
            }
            Stmt::Motif(code, expression) => {
                let code = as_text(self.eval(code)?)?;
                let Value::Graphic(Some(form)) = self.eval(expression)? else {
                    return Err(fail("MOTIF attend une forme graphique initialisée"));
                };
                if form.kind != GraphicType::Form {
                    return Err(fail("MOTIF attend une forme graphique de type FORME"));
                }
                self.host.define_motif(&code, &form).map_err(fail)?;
                Ok(Flow::Next)
            }
            Stmt::GraphicCommand(name, expressions) => {
                let arguments = self.graphics_arguments(expressions)?;
                match name.as_str() {
                    "TRACE" => {
                        require_graphic_text(&arguments, 0, name)?;
                    }
                    "INSERER" => {
                        require_graphic_object(&arguments, 0, GraphicType::Page, name)?;
                        require_graphic_object(&arguments, 1, GraphicType::Form, name)?;
                    }
                    "CADRER" => {
                        require_graphic_object(&arguments, 0, GraphicType::Page, name)?;
                        for index in 1..5 {
                            require_graphic_number(&arguments, index, name)?;
                        }
                    }
                    "TRANSFERER" => {
                        require_graphic_object(&arguments, 0, GraphicType::Page, name)?;
                        require_graphic_number(&arguments, 1, name)?;
                    }
                    "MARGER" => {
                        for index in 0..5 {
                            require_graphic_number(&arguments, index, name)?;
                        }
                    }
                    "ALLUMER" | "ETEINDRE" | "NETTOYER" => {
                        require_graphic_number(&arguments, 0, name)?;
                    }
                    "CIBLE" => {}
                    _ => return Err(fail(format!("commande graphique `{name}` inconnue"))),
                }
                self.host.graphics_command(name, &arguments).map_err(fail)?;
                Ok(Flow::Next)
            }
            Stmt::ArrayDeclaration(declarations) => {
                for declaration in declarations {
                    let mut dimensions = Vec::with_capacity(declaration.dimensions.len());
                    for expression in &declaration.dimensions {
                        let dimension = as_integer(self.eval(expression)?)?;
                        if dimension <= 0 {
                            return Err(fail(
                                "les dimensions du tableau doivent être des entiers positifs",
                            ));
                        }
                        dimensions.push(
                            usize::try_from(dimension)
                                .map_err(|_| fail("dimension de tableau trop grande"))?,
                        );
                    }
                    let element_count =
                        dimensions.iter().try_fold(1_usize, |product, dimension| {
                            product.checked_mul(*dimension)
                        });
                    let element_count = element_count
                        .filter(|count| *count <= MAX_ARRAY_ELEMENTS)
                        .ok_or_else(|| {
                            fail(format!(
                                "l'allocation du tableau dépasse la limite de {MAX_ARRAY_ELEMENTS} éléments"
                            ))
                        })?;
                    let default = default_value(declaration.kind);
                    let mut elements = Vec::new();
                    elements
                        .try_reserve_exact(element_count)
                        .map_err(|_| fail("mémoire insuffisante pour allouer le tableau"))?;
                    elements.resize_with(element_count, || default.clone());
                    self.set_variable(
                        &declaration.name,
                        Value::Array(Rc::new(RefCell::new(ArrayValue {
                            dimensions,
                            kind: declaration.kind,
                            elements,
                        }))),
                    )?;
                }
                Ok(Flow::Next)
            }
            Stmt::Assign(name, expr) => {
                let value = self.eval(expr)?;
                if (matches!(value, Value::Text(_))
                    && !self.variable_is_declared_as(name, VariableKind::Text))
                    || (matches!(value, Value::Boolean(_))
                        && !self.variable_is_declared_as(name, VariableKind::Boolean))
                    || (matches!(value, Value::Graphic(_))
                        && !self.variable_is_declared_as(name, VariableKind::Graphic))
                {
                    return Err(fail(format!(
                        "la variable `{name}` doit être déclarée avant son utilisation"
                    )));
                }
                if !value_matches_kind(&value, self.variable_kind(name))
                    || matches!(value, Value::Array(_) | Value::Procedure(_))
                {
                    return Err(fail(&format!(
                        "le type de `{name}` doit correspondre à sa déclaration"
                    )));
                }
                self.set_variable(name, value)?;
                Ok(Flow::Next)
            }
            Stmt::ArrayAssign(name, index_expressions, expression) => {
                let indices = self.eval_array_indices(index_expressions)?;
                let array = self.array_reference(name)?;
                let offset = array.borrow().offset(&indices)?;
                let value = self.eval(expression)?;
                let mut array = array.borrow_mut();
                if !value_matches_kind(&value, array.kind) {
                    return Err(fail(format!(
                        "les éléments du tableau `{name}` doivent être de type {}",
                        array.kind.name()
                    )));
                }
                array.elements[offset] = value;
                Ok(Flow::Next)
            }
            Stmt::Jump(label) => Ok(Flow::Jump(*label)),
            Stmt::For {
                body_label,
                variable,
                start,
                step,
                limit,
                condition,
            } => {
                let loop_key = (self.frames.len(), line);
                let (mut state, advance) = if let Some(state) = self.loop_states.remove(&loop_key) {
                    (state, true)
                } else {
                    let current = as_real(self.eval(start)?)?;
                    let increment = as_real(self.eval(step)?)?;
                    let current_limit = limit
                        .as_ref()
                        .map(|expression| self.eval(expression).and_then(as_real))
                        .transpose()?;
                    if !increment.is_finite() {
                        return Err(fail("le pas de boucle doit être fini"));
                    }
                    (
                        ForLoopState {
                            body_label: *body_label,
                            variable: variable.to_ascii_lowercase(),
                            current,
                            step: increment,
                            step_expression: step.clone(),
                            limit: current_limit,
                            limit_expression: limit.clone(),
                            condition: condition.clone(),
                        },
                        false,
                    )
                };
                if advance {
                    let increment = as_real(self.eval(&state.step_expression)?)?;
                    if !increment.is_finite() {
                        return Err(fail("le pas de boucle doit être fini"));
                    }
                    state.step = increment;
                    if let Some(value) = self.get_variable(&state.variable) {
                        state.current = as_real(value)?;
                    }
                    state.current += increment;
                    state.limit = state
                        .limit_expression
                        .as_ref()
                        .map(|expression| self.eval(expression).and_then(as_real))
                        .transpose()?;
                }
                if !state.current.is_finite() {
                    return Err(fail("la variable de boucle n'est pas finie"));
                }
                self.set_variable(&state.variable, Value::Real(state.current))?;
                let continues = if let Some(condition) = &state.condition {
                    as_bool(self.eval(condition)?)?
                } else {
                    let limit = state.limit.ok_or_else(|| fail("limite JUSQUA manquante"))?;
                    if state.step > 0.0 {
                        state.current <= limit
                    } else {
                        state.current >= limit
                    }
                };
                if continues {
                    let target = state.body_label;
                    self.loop_states.insert(loop_key, state);
                    Ok(Flow::Jump(target))
                } else {
                    Ok(Flow::Next)
                }
            }
            Stmt::If(condition, yes, no) => {
                if as_bool(self.eval(condition)?)? {
                    self.execute_list(yes, line)
                } else {
                    self.execute_list(no, line)
                }
            }
            Stmt::Block(statements) => self.execute_list(statements, line),
            Stmt::ProcedureCall(name, arguments) => match self.call_procedure(name, arguments)? {
                Flow::Terminate => Ok(Flow::Terminate),
                Flow::Jump(label) => Ok(Flow::Jump(label)),
                Flow::Return(_) | Flow::Next => Ok(Flow::Next),
            },
            Stmt::Return(expression) => {
                let value = expression
                    .as_ref()
                    .map(|expression| self.eval(expression))
                    .transpose()?;
                Ok(Flow::Return(value))
            }
            Stmt::Terminate => Ok(Flow::Terminate),
            Stmt::Read { names, prompt } => {
                if !prompt.is_empty() {
                    self.display(prompt, &[], false)?;
                }
                for name in names {
                    let key = name.to_ascii_lowercase();
                    if let Some(Value::Array(array)) = self.get_variable(&key) {
                        let (kind, element_count) = {
                            let array = array.borrow();
                            (array.kind, array.elements.len())
                        };
                        for index in 0..element_count {
                            let raw = self.host.input().map_err(fail)?;
                            let value = parse_input_value(kind, raw)?;
                            array.borrow_mut().elements[index] = value;
                        }
                        continue;
                    }
                    if self.array_is_declared(&key) {
                        return Err(fail(format!("le tableau `{name}` n'a pas été alloué")));
                    }
                    let kind = self.variable_kind(&key);
                    if kind != VariableKind::Real && !self.variable_is_declared_as(&key, kind) {
                        return Err(fail(format!(
                            "la variable `{name}` doit être déclarée avant son utilisation"
                        )));
                    }
                    let raw = self.host.input().map_err(fail)?;
                    let value = parse_input_value(kind, raw)?;
                    self.set_variable(&key, value)?;
                }
                Ok(Flow::Next)
            }
            Stmt::StoreRecord {
                name,
                key,
                file,
                status,
            } => {
                let key = as_real(self.eval(key)?)?;
                let file = as_text(self.eval(file)?)?;
                let value = self
                    .get_variable(name)
                    .ok_or_else(|| fail(format!("variable `{name}` non définie")))?;
                let stored = value_to_stored(value)?;
                let result = self.host.store_record(&file, key, stored).map_err(fail)?;
                if let Some(status) = status {
                    self.assign_numeric_output(status, result as f64)?;
                }
                Ok(Flow::Next)
            }
            Stmt::LoadRecord {
                name,
                key,
                file,
                status,
            } => {
                let key = as_real(self.eval(key)?)?;
                let file = as_text(self.eval(file)?)?;
                let (result, loaded) = self.host.load_record(&file, key).map_err(fail)?;
                if let Some(stored) = loaded {
                    let value = value_from_stored(stored)?;
                    self.validate_loaded_value(name, &value)?;
                    self.set_variable(name, value)?;
                }
                if let Some(status) = status {
                    self.assign_numeric_output(status, result as f64)?;
                }
                Ok(Flow::Next)
            }
            Stmt::ExecuteProgram {
                program,
                argument,
                status,
            } => {
                let program = as_text(self.eval(program)?)?;
                let argument = as_real(self.eval(argument)?)?;
                let result = self
                    .host
                    .execute_program(&program, argument)
                    .map_err(fail)?;
                if let Some(status) = status {
                    self.assign_numeric_output(status, result as f64)?;
                }
                Ok(Flow::Next)
            }
            Stmt::DeleteRecords { file, key } => {
                let file = as_text(self.eval(file)?)?;
                let key = key
                    .as_ref()
                    .map(|expression| self.eval(expression).and_then(as_real))
                    .transpose()?;
                self.host.delete_records(&file, key).map_err(fail)?;
                Ok(Flow::Next)
            }
            Stmt::Display {
                formats,
                expressions,
                implicit_newline,
            } => {
                self.display(formats, expressions, *implicit_newline)?;
                Ok(Flow::Next)
            }
            Stmt::Free(names) => {
                for name in names {
                    let key = name.to_ascii_lowercase();
                    self.remove_variable(&key);
                }
                Ok(Flow::Next)
            }
            Stmt::Pause => {
                self.host.pause(line).map_err(fail)?;
                Ok(Flow::Next)
            }
        }
    }
    fn display(
        &mut self,
        formats: &[DisplayFormat],
        expressions: &[Expr],
        implicit_newline: bool,
    ) -> Result<(), RuntimeError> {
        let mut expression_index = 0;
        if formats.is_empty() {
            for expression in expressions {
                let value = self.eval(expression)?;
                self.output_value(value, ValueFormat::Universal)?;
            }
        } else {
            for format in formats {
                let repetitions = match format.repeat {
                    FormatRepeat::Once => 1,
                    FormatRepeat::Fixed(count) => count,
                    FormatRepeat::Argument => {
                        let expression = expressions.get(expression_index).ok_or_else(|| {
                            fail("une expression de répétition manque dans AFFICHER")
                        })?;
                        expression_index += 1;
                        let count = as_integer(self.eval(expression)?)?;
                        usize::try_from(count)
                            .ok()
                            .filter(|count| *count <= MAX_FORMAT_REPETITIONS)
                            .ok_or_else(|| {
                                fail(format!(
                                    "le facteur de répétition doit être compris entre 0 et {MAX_FORMAT_REPETITIONS}"
                                ))
                            })?
                    }
                };
                if repetitions > MAX_FORMAT_REPETITIONS {
                    return Err(fail(format!(
                        "le facteur de répétition dépasse la limite de {MAX_FORMAT_REPETITIONS}"
                    )));
                }
                match &format.operation {
                    FormatOperation::Newline => self.host.newline(repetitions).map_err(fail)?,
                    FormatOperation::CarriageReturn => {
                        self.host.output(&"\r".repeat(repetitions)).map_err(fail)?
                    }
                    FormatOperation::LineFeed => {
                        self.host.output(&"\n".repeat(repetitions)).map_err(fail)?
                    }
                    FormatOperation::Spaces => {
                        self.host.output(&" ".repeat(repetitions)).map_err(fail)?
                    }
                    FormatOperation::Literal(text) => {
                        for _ in 0..repetitions {
                            self.host.output(text).map_err(fail)?;
                        }
                    }
                    FormatOperation::Value(value_format) => {
                        for _ in 0..repetitions {
                            let expression =
                                expressions.get(expression_index).ok_or_else(|| {
                                    fail("une expression manque dans la liste AFFICHER")
                                })?;
                            expression_index += 1;
                            let value = self.eval(expression)?;
                            self.output_value(value, *value_format)?;
                        }
                    }
                }
            }
            for expression in &expressions[expression_index..] {
                let value = self.eval(expression)?;
                self.output_value(value, ValueFormat::Universal)?;
            }
        }
        if implicit_newline {
            self.host.newline(1).map_err(fail)?;
        }
        Ok(())
    }
    fn output_value(&mut self, value: Value, format: ValueFormat) -> Result<(), RuntimeError> {
        self.host
            .output(&format_output_value(value, format)?)
            .map_err(fail)
    }
    fn execute_list(&mut self, statements: &[Stmt], line: i64) -> Result<Flow, RuntimeError> {
        for stmt in statements {
            match self.execute(stmt, line)? {
                Flow::Next => {}
                flow @ (Flow::Jump(_) | Flow::Return(_) | Flow::Terminate) => return Ok(flow),
            }
        }
        Ok(Flow::Next)
    }
    fn eval(&mut self, expr: &Expr) -> Result<Value, RuntimeError> {
        match expr {
            Expr::Real(n) => Ok(Value::Real(*n)),
            Expr::Text(s) => Ok(Value::Text(s.clone())),
            Expr::Boolean(value) => Ok(Value::Boolean(*value)),
            Expr::Variable(name) => match self.get_variable(name) {
                Some(Value::Array(_)) => Err(fail(format!(
                    "le tableau `{name}` doit être utilisé avec des indices"
                ))),
                Some(value) => Ok(value),
                None => Err(fail(&format!("variable `{name}` non définie"))),
            },
            Expr::ProcedureReference(name) => {
                if let Some(Value::Procedure(procedure)) = self.get_variable(name) {
                    Ok(Value::Procedure(procedure))
                } else if self.procedures.contains_key(&name.to_ascii_lowercase()) {
                    Ok(Value::Procedure(name.to_ascii_lowercase()))
                } else {
                    Err(fail(format!("procédure `&{name}` non définie")))
                }
            }
            Expr::ProcedureCall(name, arguments) => match self.call_procedure(name, arguments)? {
                Flow::Return(Some(value)) => Ok(value),
                Flow::Terminate => Err(fail("le programme s'est terminé dans une procédure")),
                Flow::Jump(label) => Err(fail(format!("étiquette {label} inconnue"))),
                Flow::Return(None) | Flow::Next => {
                    Err(fail(format!("`&{name}` ne retourne aucune valeur")))
                }
            },
            Expr::Conditional(condition, yes, no) => {
                if as_bool(self.eval(condition)?)? {
                    self.eval(yes)
                } else {
                    self.eval(no)
                }
            }
            Expr::Index(name, index_expressions) => {
                let indices = self.eval_array_indices(index_expressions)?;
                let array = self.array_reference(name)?;
                let array = array.borrow();
                let offset = array.offset(&indices)?;
                array
                    .elements
                    .get(offset)
                    .cloned()
                    .ok_or_else(|| fail("indice de tableau hors limites"))
            }
            Expr::Unary(op, e) => {
                let v = self.eval(e)?;
                match op {
                    Unary::Negate => Ok(Value::Real(-as_real(v)?)),
                    Unary::Not => Ok(Value::Boolean(!as_bool(v)?)),
                }
            }
            Expr::Binary(a, op, b) => {
                let left = self.eval(a)?;
                if matches!(op, Op::And) && !as_bool(left.clone())? {
                    return Ok(Value::Boolean(false));
                }
                if matches!(op, Op::Or) && as_bool(left.clone())? {
                    return Ok(Value::Boolean(true));
                }
                let right = self.eval(b)?;
                self.binary(left, *op, right)
            }
            Expr::Call(name, args) => self.call(name, args),
        }
    }
    fn array_reference(&self, name: &str) -> Result<ArrayReference, RuntimeError> {
        match self.get_variable(name) {
            Some(Value::Array(array)) => Ok(array.clone()),
            Some(_) => Err(fail(format!("`{name}` n'est pas un tableau"))),
            None if self.array_is_declared(name) => {
                Err(fail(format!("le tableau `{name}` n'a pas été alloué")))
            }
            None => Err(fail(format!("tableau `{name}` non déclaré"))),
        }
    }
    fn eval_array_indices(&mut self, expressions: &[Expr]) -> Result<Vec<usize>, RuntimeError> {
        expressions
            .iter()
            .map(|expression| {
                let index = as_integer(self.eval(expression)?)?;
                usize::try_from(index)
                    .ok()
                    .filter(|index| *index > 0)
                    .ok_or_else(|| fail("les indices de tableau commencent à 1"))
            })
            .collect()
    }
    fn call_procedure(&mut self, name: &str, arguments: &[Expr]) -> Result<Flow, RuntimeError> {
        let procedure_name = match self.get_variable(name) {
            Some(Value::Procedure(procedure)) => procedure,
            _ => name.to_ascii_lowercase(),
        };
        if !self.procedures.contains_key(&procedure_name) {
            if let Some(source) = self
                .host
                .load_procedure_source(&procedure_name)
                .map_err(fail)?
            {
                let loaded = parse_procedure_definitions(&source).map_err(|diagnostics| {
                    fail(
                        diagnostics
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                })?;
                self.procedures.extend(loaded);
            }
        }
        let procedure = self
            .procedures
            .get(&procedure_name)
            .cloned()
            .ok_or_else(|| fail(format!("procédure `&{procedure_name}` non définie")))?;
        if arguments.len() != procedure.parameters.len() {
            return Err(fail(format!(
                "`&{}` attend {} paramètre(s), reçu {}",
                procedure.name,
                procedure.parameters.len(),
                arguments.len()
            )));
        }
        if self.frames.len() >= MAX_CALL_DEPTH {
            return Err(fail(format!(
                "limite de récursion dépassée ({MAX_CALL_DEPTH} appels imbriqués)"
            )));
        }

        let parameter_names: HashSet<String> = procedure.parameters.iter().cloned().collect();
        let mut frame = CallFrame {
            locals: procedure.locals.clone(),
            parameters: parameter_names,
            values: HashMap::new(),
            aliases: HashMap::new(),
            string_variables: procedure.string_variables.clone(),
            boolean_variables: procedure.boolean_variables.clone(),
            graphic_variables: procedure.graphic_variables.clone(),
            declared_strings: HashSet::new(),
            declared_booleans: HashSet::new(),
            array_variables: procedure.array_variables.clone(),
        };
        for (parameter, argument) in procedure.parameters.iter().zip(arguments) {
            if procedure.procedure_parameters.contains(parameter) {
                let value = self.eval(argument)?;
                let Value::Procedure(procedure_name) = value else {
                    return Err(fail(format!(
                        "le paramètre `&{parameter}` doit recevoir une procédure"
                    )));
                };
                frame
                    .values
                    .insert(parameter.clone(), Value::Procedure(procedure_name));
            } else if procedure.by_value.contains(parameter) {
                let value = self.eval(argument)?;
                if matches!(value, Value::Array(_)) {
                    return Err(fail(format!(
                        "le paramètre `{parameter}` est passé par valeur et ne peut recevoir un tableau"
                    )));
                }
                frame.values.insert(parameter.clone(), value);
            } else {
                let location = self.reference_for_argument(argument)?;
                frame.aliases.insert(parameter.clone(), location);
            }
        }

        self.frames.push(frame);
        let depth = self.frames.len();
        let result = self.execute_lines(&procedure.lines, &procedure.labels);
        self.frames.pop();
        self.loop_states
            .retain(|(frame_depth, _), _| *frame_depth < depth);
        result
    }
    fn graphics_arguments(
        &mut self,
        expressions: &[Expr],
    ) -> Result<Vec<GraphicsArgument>, RuntimeError> {
        expressions
            .iter()
            .map(|expression| match self.eval(expression)? {
                Value::Real(number) => Ok(GraphicsArgument::Number(number)),
                Value::Text(text) => Ok(GraphicsArgument::Text(text)),
                Value::Boolean(value) => Ok(GraphicsArgument::Boolean(value)),
                Value::Graphic(Some(object)) => Ok(GraphicsArgument::Object(object)),
                Value::Graphic(None) => {
                    Err(fail("une forme graphique non initialisée est utilisée"))
                }
                Value::Array(_) | Value::Procedure(_) => Err(fail(
                    "cet argument n'est pas pris en charge par le service graphique",
                )),
            })
            .collect()
    }
    fn graphics_function(&mut self, name: &str, args: &[Expr]) -> Result<Value, RuntimeError> {
        let (arity, returns_number) = match name {
            "VEC" => (3, false),
            "VEQ" | "VEX" | "VEY" => (1, true),
            "MTF" | "REF" => (1, false),
            "ROT" | "HOM" => (2, false),
            "TRN" | "MAT" => (if name == "TRN" { 3 } else { 5 }, false),
            "SMX" | "SMY" | "SMO" => (1, false),
            "AFX" | "AFY" => (2, false),
            _ => return Err(fail(format!("fonction graphique `{name}` non reconnue"))),
        };
        self.arity(name, args, arity)?;
        let arguments = self.graphics_arguments(args)?;
        match name {
            "VEC" => {
                let Some(GraphicsArgument::Text(code)) = arguments.first() else {
                    return Err(fail("VEC attend un code de forme A, C, E ou G"));
                };
                if code.len() != 1 || !"ACEG".contains(code.as_str()) {
                    return Err(fail("VEC attend un code de forme A, C, E ou G"));
                }
                require_graphic_number(&arguments, 1, name)?;
                require_graphic_number(&arguments, 2, name)?;
            }
            "MTF" => {
                require_graphic_text(&arguments, 0, name)?;
            }
            "VEQ" | "VEX" | "VEY" | "ROT" | "TRN" | "SMX" | "SMY" | "SMO" | "AFX" | "AFY"
            | "MAT" | "HOM" => {
                require_graphic_object(&arguments, 0, GraphicType::Form, name)?;
                match name {
                    "ROT" | "HOM" => {
                        require_graphic_number(&arguments, 1, name)?;
                    }
                    "TRN" => {
                        require_graphic_number(&arguments, 1, name)?;
                        require_graphic_number(&arguments, 2, name)?;
                    }
                    "AFX" | "AFY" => {
                        require_graphic_number(&arguments, 1, name)?;
                    }
                    "MAT" => {
                        for index in 1..5 {
                            require_graphic_number(&arguments, index, name)?;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        let result = self
            .host
            .graphics_function(name, &arguments)
            .map_err(fail)?;
        match (returns_number, result) {
            (true, GraphicsResult::Number(number)) if number.is_finite() => Ok(Value::Real(number)),
            (true, GraphicsResult::Number(_)) => {
                Err(fail("la fonction graphique a renvoyé un nombre non fini"))
            }
            (true, GraphicsResult::Object(_)) => {
                Err(fail("la fonction graphique doit renvoyer un nombre"))
            }
            (false, GraphicsResult::Object(object))
                if object.handle.is_some() && object.kind == GraphicType::Form =>
            {
                Ok(Value::Graphic(Some(object)))
            }
            (false, GraphicsResult::Object(_)) => Err(fail(
                "la fonction graphique a renvoyé une forme non initialisée",
            )),
            (false, GraphicsResult::Number(_)) => {
                Err(fail("la fonction graphique doit renvoyer une forme"))
            }
        }
    }
    fn call(&mut self, name: &str, args: &[Expr]) -> Result<Value, RuntimeError> {
        let key = name.to_ascii_uppercase();
        if matches!(key.as_str(), "TEM" | "TZL" | "NIV" | "TYP" | "IND") {
            let expected = match key.as_str() {
                "TEM" | "TZL" | "NIV" => &[0_usize][..],
                "TYP" => &[1][..],
                "IND" => &[1, 2][..],
                _ => unreachable!(),
            };
            if !expected.contains(&args.len()) {
                return Err(fail(format!(
                    "{key} n'accepte pas {} paramètre(s)",
                    args.len()
                )));
            }
            if key == "TYP" && !matches!(args.first(), Some(Expr::Variable(_))) {
                return Err(fail("TYP attend un identificateur"));
            }
            if key == "IND" && !matches!(args.first(), Some(Expr::Variable(_))) {
                return Err(fail("IND attend un identificateur en premier paramètre"));
            }
            let mut arguments = Vec::with_capacity(args.len());
            for (index, expression) in args.iter().enumerate() {
                if index == 0 && matches!(key.as_str(), "TYP" | "IND") {
                    if let Expr::Variable(identifier) = expression {
                        arguments.push(SystemArgument::Identifier(identifier.clone()));
                        continue;
                    }
                }
                arguments.push(match self.eval(expression)? {
                    Value::Real(number) => SystemArgument::Number(number),
                    Value::Text(text) => SystemArgument::Text(text),
                    Value::Boolean(value) => SystemArgument::Boolean(value),
                    Value::Array(_) | Value::Procedure(_) | Value::Graphic(_) => {
                        return Err(fail(format!("argument invalide pour {key}")));
                    }
                });
            }
            let value = self.host.system_function(&key, &arguments).map_err(fail)?;
            if !value.is_finite() {
                return Err(fail(format!("{key} a renvoyé une valeur non finie")));
            }
            return Ok(Value::Real(value));
        }
        match key.as_str() {
            "ALE" => {
                if args.len() != 1 || as_real(self.eval(&args[0])?)? != 1.0 {
                    return Err(fail("ALE(1) est la seule forme documentée"));
                }
                let n = self.host.random_unit().map_err(fail)?;
                if !(0.0 < n && n < 1.0) {
                    return Err(fail(
                        "le service aléatoire doit renvoyer une valeur strictement entre 0 et 1",
                    ));
                }
                Ok(Value::Real(n))
            }
            "VEC" | "VEQ" | "VEX" | "VEY" | "MTF" | "REF" | "ROT" | "TRN" | "SMX" | "SMY"
            | "SMO" | "AFX" | "AFY" | "MAT" | "HOM" => self.graphics_function(&key, args),
            "DAT" => {
                if !args.is_empty() {
                    return Err(fail("DAT n'accepte aucun argument"));
                }
                Ok(Value::Text(self.host.date_time().map_err(fail)?))
            }
            "ICH" => {
                self.arity(name, args, 1)?;
                let text = as_text(self.eval(&args[0])?)?;
                Ok(Value::Text(text.chars().rev().collect()))
            }
            "TMA" | "TMI" => {
                self.arity(name, args, 1)?;
                let text = as_text(self.eval(&args[0])?)?;
                let converted = if key == "TMA" {
                    text.to_uppercase()
                } else {
                    text.to_lowercase()
                };
                Ok(Value::Text(converted))
            }
            "REP" => {
                self.arity(name, args, 2)?;
                let text = as_text(self.eval(&args[0])?)?;
                let count = as_integer(self.eval(&args[1])?)?;
                let count = usize::try_from(count)
                    .map_err(|_| fail("REP attend une répétition entière non négative"))?;
                let capacity = text
                    .len()
                    .checked_mul(count)
                    .filter(|size| *size <= 16 * 1024 * 1024)
                    .ok_or_else(|| fail("le résultat de REP dépasse la limite de chaîne"))?;
                let mut result = String::new();
                result
                    .try_reserve_exact(capacity)
                    .map_err(|_| fail("mémoire insuffisante pour le résultat de REP"))?;
                for _ in 0..count {
                    result.push_str(&text);
                }
                Ok(Value::Text(result))
            }
            "RAC" | "ABS" | "ENT" | "SIN" | "COS" | "TAN" | "EXP" | "LGN" | "ATG" => {
                self.arity(name, args, 1)?;
                let input = as_real(self.eval(&args[0])?)?;
                if !input.is_finite() {
                    return Err(fail("l'argument numérique doit être fini"));
                }
                let value = match key.as_str() {
                    "RAC" if input >= 0.0 => input.sqrt(),
                    "RAC" => return Err(fail("RAC attend un argument positif ou nul")),
                    "ABS" => input.abs(),
                    "ENT" => input.floor(),
                    "SIN" => input.sin(),
                    "COS" => input.cos(),
                    "TAN" => input.tan(),
                    "EXP" => input.exp(),
                    "LGN" if input > 0.0 => input.ln(),
                    "LGN" => return Err(fail("LGN attend un argument strictement positif")),
                    "ATG" => input.atan(),
                    _ => unreachable!(),
                };
                if !value.is_finite() {
                    return Err(fail("le résultat de la fonction numérique n'est pas fini"));
                }
                Ok(Value::Real(value))
            }
            "CCA" => {
                self.arity(name, args, 1)?;
                Ok(Value::Text(as_real(self.eval(&args[0])?)?.to_string()))
            }
            "EQC" => {
                self.arity(name, args, 1)?;
                let n = as_integer(self.eval(&args[0])?)?;
                if !(0..128).contains(&n) {
                    return Err(fail("EQC attend un code ASCII entre 0 et 127"));
                }
                Ok(Value::Text(char::from_u32(n as u32).unwrap().to_string()))
            }
            "LGR" => {
                self.arity(name, args, 1)?;
                Ok(Value::Real(
                    as_text(self.eval(&args[0])?)?.chars().count() as f64
                ))
            }
            "POS" => {
                self.arity(name, args, 3)?;
                let text = as_text(self.eval(&args[0])?)?;
                let start = as_index(self.eval(&args[1])?)?;
                let needle = as_text(self.eval(&args[2])?)?;
                let chars: Vec<char> = text.chars().collect();
                if start > chars.len() + 1 {
                    return Ok(Value::Real(0.0));
                }
                if needle.is_empty() {
                    return Ok(Value::Real(start as f64));
                }
                let pattern: Vec<char> = needle.chars().collect();
                let found = chars[start - 1..]
                    .windows(pattern.len())
                    .position(|w| w == pattern.as_slice());
                Ok(Value::Real(
                    found.map(|p| (start + p) as f64).unwrap_or(0.0),
                ))
            }
            "RCC" => self.compare_text(args),
            "SCH" => self.substring_function(args),
            "MCH" => self.modify_text(args),
            "PTR" => self.pointer_function(args),
            "GRL" => self.letters_function(args),
            "CNB" => self.numeric_conversion(args),
            "EQN" => {
                if !(1..=2).contains(&args.len()) {
                    return Err(fail("EQN attend une chaîne et une position optionnelle"));
                }
                let text = as_text(self.eval(&args[0])?)?;
                let pos = if let Some(position) = args.get(1) {
                    as_index(self.eval(position)?)?
                } else {
                    1
                };
                let c = text
                    .chars()
                    .nth(pos - 1)
                    .ok_or_else(|| fail("position EQN hors limites"))?;
                Ok(Value::Real(c as u32 as f64))
            }
            "SKP" => self.skip_function(args),
            _ => Err(fail(&format!(
                "fonction LSE `{name}` sans règle documentée"
            ))),
        }
    }
    fn substring_function(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        if !(3..=4).contains(&args.len()) {
            return Err(fail("SCH/PTR attendent trois paramètres, avec δ optionnel"));
        }
        let source = as_text(self.eval(&args[0])?)?;
        let start = as_index(self.eval(&args[1])?)?;
        let criterion = self.eval(&args[2])?;
        let chars: Vec<char> = source.chars().collect();
        let (begin, end) = substring_bounds(&chars, start, criterion)?;
        if args.len() == 4 {
            let Expr::Variable(name) = &args[3] else {
                return Err(fail("δ doit être un identificateur numérique"));
            };
            self.assign_numeric_output(name, (end + 1) as f64)?;
        }
        Ok(Value::Text(chars[begin..end].iter().collect()))
    }
    fn compare_text(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        if !(2..=3).contains(&args.len()) {
            return Err(fail(
                "RCC attend deux chaînes et un ordre optionnel de 256 caractères",
            ));
        }
        let left = as_text(self.eval(&args[0])?)?;
        let right = as_text(self.eval(&args[1])?)?;
        let order = if let Some(expression) = args.get(2) {
            let order = as_text(self.eval(expression)?)?;
            let characters = order.chars().collect::<Vec<_>>();
            if characters.len() != 256 {
                return Err(fail(
                    "l'ordre fourni à RCC doit contenir exactement 256 caractères",
                ));
            }
            let mut positions = HashMap::with_capacity(characters.len());
            for (position, character) in characters.into_iter().enumerate() {
                if positions.insert(character, position).is_some() {
                    return Err(fail(
                        "l'ordre fourni à RCC doit contenir des caractères uniques",
                    ));
                }
            }
            Some(positions)
        } else {
            None
        };
        let comparison = if let Some(positions) = order {
            let rank = |text: &str| -> Result<Vec<usize>, RuntimeError> {
                text.chars()
                    .map(|character| {
                        positions.get(&character).copied().ok_or_else(|| {
                            fail(&format!(
                                "le caractère `{character}` n'existe pas dans l'ordre fourni à RCC"
                            ))
                        })
                    })
                    .collect()
            };
            rank(&left)?.cmp(&rank(&right)?)
        } else {
            left.chars().cmp(right.chars())
        };
        let result = match comparison {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        };
        Ok(Value::Real(result))
    }
    fn pointer_function(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        if !(2..=3).contains(&args.len()) {
            return Err(fail("PTR attend deux paramètres et un critère optionnel"));
        }
        let source = as_text(self.eval(&args[0])?)?;
        let start = as_index(self.eval(&args[1])?)?;
        let chars: Vec<char> = source.chars().collect();
        let end = if let Some(criterion) = args.get(2) {
            let criterion = self.eval(criterion)?;
            substring_bounds(&chars, start, criterion)?.1
        } else {
            let begin = start.saturating_sub(1).min(chars.len());
            (begin..chars.len())
                .find(|index| !chars[*index].is_alphabetic())
                .unwrap_or(chars.len())
        };
        Ok(Value::Real((end + 1) as f64))
    }
    fn modify_text(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        self.arity("MCH", args, 4)?;
        let source = as_text(self.eval(&args[0])?)?;
        let start = as_index(self.eval(&args[1])?)?;
        let criterion = self.eval(&args[2])?;
        let replacement = as_text(self.eval(&args[3])?)?;
        let mut chars: Vec<char> = source.chars().collect();
        let (begin, end) = substring_bounds(&chars, start, criterion)?;
        chars.splice(begin..end, replacement.chars());
        Ok(Value::Text(chars.into_iter().collect()))
    }
    fn skip_function(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        if !(2..=3).contains(&args.len()) {
            return Err(fail("SKP attend deux paramètres et une chaîne optionnelle"));
        }
        let text = as_text(self.eval(&args[0])?)?;
        let start = as_index(self.eval(&args[1])?)?;
        let chars: Vec<char> = text.chars().collect();
        let position = if let Some(skipped) = args.get(2) {
            let skipped = as_text(self.eval(skipped)?)?;
            let set: HashSet<char> = skipped.chars().collect();
            chars
                .iter()
                .enumerate()
                .skip(start.saturating_sub(1))
                .find(|(_, character)| !set.contains(character))
                .map(|(index, _)| index + 1)
        } else {
            chars
                .iter()
                .enumerate()
                .skip(start.saturating_sub(1))
                .find(|(_, character)| !character.is_alphabetic())
                .map(|(index, _)| index + 1)
        }
        .unwrap_or(chars.len() + 1);
        Ok(Value::Real(position as f64))
    }
    fn letters_function(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        if !(2..=3).contains(&args.len()) {
            return Err(fail("GRL attend deux paramètres et δ optionnel"));
        }
        let source = as_text(self.eval(&args[0])?)?;
        let start = as_index(self.eval(&args[1])?)?;
        let chars: Vec<char> = source.chars().collect();
        let mut begin = (start - 1).min(chars.len());
        while begin < chars.len() && !chars[begin].is_alphabetic() {
            begin += 1;
        }
        let mut end = begin;
        while end < chars.len() && chars[end].is_alphabetic() {
            end += 1;
        }
        if args.len() == 3 {
            let Expr::Variable(name) = &args[2] else {
                return Err(fail("δ doit être un identificateur numérique"));
            };
            self.assign_numeric_output(name, (end + 1) as f64)?;
        }
        Ok(Value::Text(chars[begin..end].iter().collect()))
    }
    fn numeric_conversion(&mut self, args: &[Expr]) -> Result<Value, RuntimeError> {
        if !(2..=3).contains(&args.len()) {
            return Err(fail("CNB attend deux paramètres et un résultat optionnel"));
        }
        let source = as_text(self.eval(&args[0])?)?;
        let start = as_index(self.eval(&args[1])?)?;
        let chars: Vec<char> = source.chars().collect();
        let mut end = (start - 1).min(chars.len());
        if end < chars.len() && matches!(chars[end], '+' | '-') {
            end += 1;
        }
        let mut digits = 0;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
            digits += 1;
        }
        if end < chars.len() && chars[end] == '.' {
            end += 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(fail("CNB n'a trouvé aucun nombre"));
        }
        let number: String = chars[start.saturating_sub(1)..end].iter().collect();
        let value: f64 = number
            .parse()
            .map_err(|_| fail("nombre incorrect dans CNB"))?;
        if let Some(cursor) = args.get(2) {
            let Expr::Variable(name) = cursor else {
                return Err(fail("δ doit être un identificateur numérique"));
            };
            self.assign_numeric_output(name, (end + 1) as f64)?;
        }
        Ok(Value::Real(value))
    }
    fn assign_numeric_output(&mut self, name: &str, value: f64) -> Result<(), RuntimeError> {
        if self.variable_kind(name) != VariableKind::Real {
            return Err(fail(&format!("`{name}` doit être une variable numérique")));
        }
        self.set_variable(name, Value::Real(value))
    }
    fn binary(&mut self, left: Value, op: Op, right: Value) -> Result<Value, RuntimeError> {
        use Op::*;
        match op {
            Add => Ok(Value::Real(as_real(left)? + as_real(right)?)),
            Sub => Ok(Value::Real(as_real(left)? - as_real(right)?)),
            Mul => Ok(Value::Real(as_real(left)? * as_real(right)?)),
            Power => {
                let base = as_real(left)?;
                let exponent = as_real(right)?;
                let value = base.powf(exponent);
                if value.is_finite() {
                    Ok(Value::Real(value))
                } else {
                    Err(fail("résultat d'exponentiation hors du domaine numérique"))
                }
            }
            Div => {
                let d = as_real(right)?;
                if d == 0.0 {
                    Err(fail("division par zéro"))
                } else {
                    Ok(Value::Real(as_real(left)? / d))
                }
            }
            Concat => {
                let (Value::Text(mut a), Value::Text(b)) = (left, right) else {
                    return Err(fail("! concatène deux chaînes"));
                };
                a.push_str(&b);
                Ok(Value::Text(a))
            }
            Eq | Ne => {
                let equal = match (&left, &right) {
                    (Value::Real(a), Value::Real(b)) => a == b,
                    (Value::Text(a), Value::Text(b)) => a == b,
                    (Value::Boolean(a), Value::Boolean(b)) => a == b,
                    _ => return Err(fail("comparaison de types incompatibles")),
                };
                Ok(Value::Boolean(if matches!(op, Eq) {
                    equal
                } else {
                    !equal
                }))
            }
            Lt | Le | Gt | Ge => {
                let ordering = match (&left, &right) {
                    (Value::Real(a), Value::Real(b)) => a.partial_cmp(b),
                    (Value::Text(a), Value::Text(b)) => Some(a.cmp(b)),
                    _ => None,
                }
                .ok_or_else(|| fail("comparaison de types incompatibles"))?;
                Ok(Value::Boolean(match op {
                    Lt => ordering.is_lt(),
                    Le => !ordering.is_gt(),
                    Gt => ordering.is_gt(),
                    _ => !ordering.is_lt(),
                }))
            }
            And => Ok(Value::Boolean(as_bool(left)? && as_bool(right)?)),
            Or => Ok(Value::Boolean(as_bool(left)? || as_bool(right)?)),
            Juxtapose | Superpose => {
                let (Value::Graphic(Some(left)), Value::Graphic(Some(right))) = (left, right)
                else {
                    return Err(fail(
                        "les opérateurs graphiques attendent deux formes initialisées",
                    ));
                };
                if left.kind != GraphicType::Form || right.kind != GraphicType::Form {
                    return Err(fail(
                        "les opérateurs graphiques attendent deux valeurs FORME",
                    ));
                }
                let operator = if matches!(op, Juxtapose) { ':' } else { '%' };
                let result = self
                    .host
                    .graphics_operator(operator, &left, &right)
                    .map_err(fail)?;
                if result.handle.is_none() || result.kind != GraphicType::Form {
                    return Err(fail(
                        "l'opération graphique doit renvoyer une valeur FORME initialisée",
                    ));
                }
                Ok(Value::Graphic(Some(result)))
            }
        }
    }
    fn arity(&self, name: &str, args: &[Expr], expected: usize) -> Result<(), RuntimeError> {
        if args.len() == expected {
            Ok(())
        } else {
            Err(fail(&format!(
                "{name} attend {expected} argument(s), reçu {}",
                args.len()
            )))
        }
    }
}

fn require_graphic_number(
    arguments: &[GraphicsArgument],
    index: usize,
    operation: &str,
) -> Result<f64, RuntimeError> {
    match arguments.get(index) {
        Some(GraphicsArgument::Number(number)) => Ok(*number),
        _ => Err(fail(format!(
            "{operation} attend une valeur numérique au paramètre {}",
            index + 1
        ))),
    }
}

fn require_graphic_text<'a>(
    arguments: &'a [GraphicsArgument],
    index: usize,
    operation: &str,
) -> Result<&'a str, RuntimeError> {
    match arguments.get(index) {
        Some(GraphicsArgument::Text(text)) => Ok(text),
        _ => Err(fail(format!(
            "{operation} attend une chaîne au paramètre {}",
            index + 1
        ))),
    }
}

fn require_graphic_object<'a>(
    arguments: &'a [GraphicsArgument],
    index: usize,
    kind: GraphicType,
    operation: &str,
) -> Result<&'a GraphicValue, RuntimeError> {
    match arguments.get(index) {
        Some(GraphicsArgument::Object(object)) if object.kind == kind => Ok(object),
        _ => Err(fail(format!(
            "{operation} attend une valeur graphique de type {kind:?} au paramètre {}",
            index + 1
        ))),
    }
}

fn substring_bounds(
    chars: &[char],
    start: usize,
    criterion: Value,
) -> Result<(usize, usize), RuntimeError> {
    let begin = start.saturating_sub(1).min(chars.len());
    let end = match criterion {
        Value::Real(length) => {
            let length = as_integer(Value::Real(length))?;
            if length < 0 {
                return Err(fail("longueur de sous-chaîne négative"));
            }
            begin.saturating_add(length as usize).min(chars.len())
        }
        Value::Text(delimiters) => {
            let set: HashSet<char> = delimiters.chars().collect();
            (begin..chars.len())
                .find(|index| set.contains(&chars[*index]))
                .unwrap_or(chars.len())
        }
        _ => {
            return Err(fail(
                "le critère de sous-chaîne doit être un nombre ou une chaîne",
            ));
        }
    };
    Ok((begin, end))
}

fn as_real(value: Value) -> Result<f64, RuntimeError> {
    if let Value::Real(n) = value {
        Ok(n)
    } else {
        Err(fail("une valeur numérique est attendue"))
    }
}
fn as_integer(value: Value) -> Result<i64, RuntimeError> {
    let n = as_real(value)?;
    if !n.is_finite() || n.fract() != 0.0 || n < i64::MIN as f64 || n >= 9_223_372_036_854_775_808.0
    {
        Err(fail("un entier est attendu"))
    } else {
        Ok(n as i64)
    }
}
fn as_index(value: Value) -> Result<usize, RuntimeError> {
    let n = as_integer(value)?;
    if n < 1 {
        Err(fail("les positions de chaîne commencent à 1"))
    } else {
        Ok(n as usize)
    }
}
fn as_text(value: Value) -> Result<String, RuntimeError> {
    if let Value::Text(s) = value {
        Ok(s)
    } else {
        Err(fail("une chaîne est attendue"))
    }
}
fn as_bool(value: Value) -> Result<bool, RuntimeError> {
    if let Value::Boolean(b) = value {
        Ok(b)
    } else {
        Err(fail("une condition booléenne est attendue"))
    }
}
fn fail(message: impl ToString) -> RuntimeError {
    RuntimeError {
        message: message.to_string(),
    }
}
