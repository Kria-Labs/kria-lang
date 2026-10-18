/// Abstract Syntax Tree definitions for Kria language

#[derive(Debug, Clone)]
pub enum Statement {
    Assignment {
        name: String,
        value: Expression,
    },
    Print(Expression),
    If {
        branches: Vec<(Expression, Vec<Statement>)>,
        else_branch: Option<Vec<Statement>>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    FunctionDef {
        name: String,
        params: Vec<String>,
        body: Vec<Statement>,
        exported: bool,
    },
    Return(Option<Expression>),
    IndexAssign {
        object: Box<Expression>,
        index: Box<Expression>,
        value: Box<Expression>,
    },
    PropertyAssign {
        target: Box<Expression>,
        value: Box<Expression>,
    },
    ForIn {
        key_name: String,
        value_name: Option<String>,
        iterable: Box<Expression>,
        body: Vec<Statement>,
    },
    Break,
    Continue,
    Expression(Expression),
    Import {
        alias: String,
        path: String,
    },
}

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    UnaryOp {
        op: UnaryOperator,
        expr: Box<Expression>,
    },
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOperator,
        right: Box<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    FunctionExpr {
        params: Vec<String>,
        body: Vec<Statement>,
    },
    Input {
        types: Vec<InputType>,
        prompt: Box<Expression>,
    },
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    MemberAccess {
        object: Box<Expression>,
        member: String,
    },
    Call {
        callee: Box<Expression>,
        args: Vec<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputType {
    String,
    Int,
    Float,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOperator {
    Not,
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
    Array {
        elements: Vec<Expression>,
        mutable: bool,
    },
    Object {
        fields: Vec<(String, Expression)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    And,
    Or,
}
