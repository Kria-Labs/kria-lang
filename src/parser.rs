use crate::ast::{BinaryOperator, Expression, Literal, Statement, UnaryOperator};
use crate::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            position: 0,
        }
    }

    fn current_token(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        if std::mem::discriminant(self.current_token()) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected {:?}, found {:?}",
                expected,
                self.current_token()
            ))
        }
    }

    fn expect_statement_end(&mut self) -> Result<(), String> {
        match self.current_token() {
            Token::Newline => {
                self.advance();
                Ok(())
            }
            Token::Eof | Token::RBrace => Ok(()),
            _ => Err(format!(
                "Expected newline or end of file, found {:?}",
                self.current_token()
            )),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Statement>, String> {
        let mut statements = Vec::new();

        while self.current_token() != &Token::Eof {
            while self.current_token() == &Token::Newline {
                self.advance();
            }
            if self.current_token() == &Token::Eof {
                break;
            }
            statements.push(self.parse_statement()?);
        }

        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        match self.current_token() {
            Token::Set => self.parse_assignment(),
            Token::For => self.parse_for_in(),
            Token::Print => self.parse_print(),
            Token::If => self.parse_if_statement(),
            Token::While => self.parse_while_statement(),
            Token::Import => self.parse_import(),
            Token::Export => {
                self.advance();
                if self.current_token() != &Token::Fn {
                    return Err(
                        "export only supports functions (use: export fn name(...))".to_string()
                    );
                }
                self.parse_function_def(true)
            }
            Token::Fn => self.parse_function_def(false),
            Token::Return => self.parse_return(),
            Token::Break => {
                self.advance();
                self.expect_statement_end()?;
                Ok(Statement::Break)
            }
            Token::Continue => {
                self.advance();
                self.expect_statement_end()?;
                Ok(Statement::Continue)
            }
            Token::Identifier(_)
            | Token::Number(_)
            | Token::String(_)
            | Token::True
            | Token::False
            | Token::Null
            | Token::LParen
            | Token::LBracket
            | Token::Hash
            | Token::LBrace => self.parse_expression_or_target_assign(),
            _ => Err(format!("Unexpected token: {:?}", self.current_token())),
        }
    }

    fn parse_expression_or_target_assign(&mut self) -> Result<Statement, String> {
        let expr = self.parse_expression()?;
        if self.current_token() == &Token::Equal {
            match &expr {
                Expression::Index { object, index } => {
                    self.advance();
                    let value = self.parse_expression()?;
                    self.expect_statement_end()?;
                    Ok(Statement::IndexAssign {
                        object: object.clone(),
                        index: index.clone(),
                        value: Box::new(value),
                    })
                }
                Expression::MemberAccess { .. } => {
                    self.advance();
                    let value = self.parse_expression()?;
                    self.expect_statement_end()?;
                    Ok(Statement::PropertyAssign {
                        target: Box::new(expr),
                        value: Box::new(value),
                    })
                }
                _ => Err(
                    "Only property or index expressions can be assigned without 'set'".to_string(),
                ),
            }
        } else {
            self.expect_statement_end()?;
            Ok(Statement::Expression(expr))
        }
    }

    fn parse_for_in(&mut self) -> Result<Statement, String> {
        self.expect(Token::For)?;
        let key_name = match self.current_token() {
            Token::Identifier(n) => {
                let n_clone = n.clone();
                self.advance();
                n_clone
            }
            _ => return Err("Expected variable name after 'for'".to_string()),
        };

        let value_name = if self.current_token() == &Token::Comma {
            self.advance();
            match self.current_token() {
                Token::Identifier(n) => {
                    let v = n.clone();
                    self.advance();
                    Some(v)
                }
                _ => return Err("Expected value variable name after ',' in for-in".to_string()),
            }
        } else {
            None
        };

        self.expect(Token::In)?;
        let iterable = self.parse_expression()?;
        while self.current_token() == &Token::Newline {
            self.advance();
        }
        let body = self.parse_block()?;
        self.expect_statement_end()?;
        Ok(Statement::ForIn {
            key_name,
            value_name,
            iterable: Box::new(iterable),
            body,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement, String> {
        self.expect(Token::While)?;
        let condition = self.parse_expression()?;
        while self.current_token() == &Token::Newline {
            self.advance();
        }
        let body = self.parse_block()?;

        Ok(Statement::While { condition, body })
    }

    fn parse_if_statement(&mut self) -> Result<Statement, String> {
        self.expect(Token::If)?;
        let condition = self.parse_expression()?;
        while self.current_token() == &Token::Newline {
            self.advance();
        }
        let true_branch = self.parse_block()?;

        while self.current_token() == &Token::Newline {
            self.advance();
        }

        let mut branches = vec![(condition, true_branch)];
        let mut else_branch = None;

        while let Token::ElseIf = self.current_token() {
            self.advance();
            let condition = self.parse_expression()?;
            while self.current_token() == &Token::Newline {
                self.advance();
            }
            let branch = self.parse_block()?;
            while self.current_token() == &Token::Newline {
                self.advance();
            }
            branches.push((condition, branch));
        }

        if let Token::Else = self.current_token() {
            self.advance();
            while self.current_token() == &Token::Newline {
                self.advance();
            }
            else_branch = Some(self.parse_block()?);
        }

        Ok(Statement::If {
            branches,
            else_branch,
        })
    }

    fn parse_block(&mut self) -> Result<Vec<Statement>, String> {
        self.expect(Token::LBrace)?;

        let mut statements = Vec::new();
        while self.current_token() != &Token::RBrace {
            if self.current_token() == &Token::Newline {
                self.advance();
                continue;
            }
            statements.push(self.parse_statement()?);
            if self.current_token() == &Token::Newline {
                self.advance();
            }
        }

        self.expect(Token::RBrace)?;
        Ok(statements)
    }

    fn parse_assignment(&mut self) -> Result<Statement, String> {
        self.expect(Token::Set)?;

        let name = match self.current_token() {
            Token::Identifier(n) => {
                let n_clone = n.clone();
                self.advance();
                n_clone
            }
            _ => return Err("Expected identifier after 'set'".to_string()),
        };

        while self.current_token() == &Token::Newline {
            self.advance();
        }

        self.expect(Token::Equal)?;
        let value = self.parse_expression()?;
        self.expect_statement_end()?;

        Ok(Statement::Assignment { name, value })
    }

    fn parse_print(&mut self) -> Result<Statement, String> {
        self.expect(Token::Print)?;
        self.expect(Token::LParen)?;
        let expr = self.parse_expression()?;
        self.expect(Token::RParen)?;
        self.expect_statement_end()?;

        Ok(Statement::Print(expr))
    }

    fn parse_import(&mut self) -> Result<Statement, String> {
        self.expect(Token::Import)?;
        let alias = match self.current_token() {
            Token::Identifier(n) => {
                let a = n.clone();
                self.advance();
                a
            }
            _ => return Err("Expected module alias after 'import'".to_string()),
        };
        self.expect(Token::From)?;
        let path = match self.current_token() {
            Token::String(p) => {
                let p = p.clone();
                self.advance();
                p
            }
            _ => return Err("Expected string path after 'from'".to_string()),
        };
        self.expect_statement_end()?;
        Ok(Statement::Import { alias, path })
    }

    fn parse_function_def(&mut self, exported: bool) -> Result<Statement, String> {
        self.expect(Token::Fn)?;

        let name = match self.current_token() {
            Token::Identifier(n) => {
                let n_clone = n.clone();
                self.advance();
                n_clone
            }
            _ => return Err("Expected function name after 'fn'".to_string()),
        };

        self.expect(Token::LParen)?;
        let params = self.parse_parameter_list()?;
        self.expect(Token::RParen)?;

        while self.current_token() == &Token::Newline {
            self.advance();
        }

        let body = self.parse_block()?;
        self.expect_statement_end()?;

        Ok(Statement::FunctionDef {
            name,
            params,
            body,
            exported,
        })
    }

    fn parse_return(&mut self) -> Result<Statement, String> {
        self.expect(Token::Return)?;

        let expr = match self.current_token() {
            Token::Newline | Token::RBrace | Token::Eof => None,
            _ => Some(self.parse_expression()?),
        };

        self.expect_statement_end()?;
        Ok(Statement::Return(expr))
    }

    fn parse_parameter_list(&mut self) -> Result<Vec<String>, String> {
        let mut params = Vec::new();

        while self.current_token() != &Token::RParen {
            if let Token::Identifier(name) = self.current_token() {
                params.push(name.clone());
                self.advance();

                if self.current_token() == &Token::RParen {
                    break;
                }
                // Allow comma-separated parameters (optional)
                if matches!(self.current_token(), Token::Comma) {
                    self.advance();
                }
            } else {
                return Err("Expected parameter name".to_string());
            }
        }

        Ok(params)
    }

    fn parse_expression(&mut self) -> Result<Expression, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_and()?;

        while let Token::Or = self.current_token() {
            self.advance();
            let right = self.parse_and()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_equality()?;

        while let Token::And = self.current_token() {
            self.advance();
            let right = self.parse_equality()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_comparison()?;

        while let Token::EqualEqual = self.current_token() {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Equals,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_additive()?;

        loop {
            let op = match self.current_token() {
                Token::Greater => Some(BinaryOperator::GreaterThan),
                Token::Less => Some(BinaryOperator::LessThan),
                Token::GreaterEqual => Some(BinaryOperator::GreaterOrEqual),
                Token::LessEqual => Some(BinaryOperator::LessOrEqual),
                Token::NotEqual => Some(BinaryOperator::NotEquals),
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_additive()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
                continue;
            }
            break;
        }

        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_multiplicative()?;

        loop {
            match self.current_token() {
                Token::Plus => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Add,
                        right: Box::new(right),
                    };
                }
                Token::Minus => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Subtract,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, String> {
        let mut left = self.parse_unary()?;

        loop {
            match self.current_token() {
                Token::Star => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Multiply,
                        right: Box::new(right),
                    };
                }
                Token::Slash => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op: BinaryOperator::Divide,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expression, String> {
        match self.current_token() {
            Token::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expression::UnaryOp {
                    op: UnaryOperator::Not,
                    expr: Box::new(expr),
                })
            }
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expression::BinaryOp {
                    left: Box::new(Expression::Literal(Literal::Number(0))),
                    op: BinaryOperator::Subtract,
                    right: Box::new(expr),
                })
            }
            _ => {
                let primary = self.parse_primary()?;
                self.parse_postfix(primary)
            }
        }
    }

    fn parse_postfix(&mut self, mut expr: Expression) -> Result<Expression, String> {
        loop {
            match self.current_token() {
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expression()?;
                    self.expect(Token::RBracket)?;
                    expr = Expression::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Token::Dot => {
                    self.advance();
                    let member = match self.current_token() {
                        Token::Identifier(name) => {
                            let m = name.clone();
                            self.advance();
                            m
                        }
                        _ => return Err("Expected member name after '.'".to_string()),
                    };
                    expr = Expression::MemberAccess {
                        object: Box::new(expr),
                        member,
                    };
                }
                Token::LParen => {
                    self.advance();
                    let args = self.parse_argument_list()?;
                    self.expect(Token::RParen)?;
                    expr = Expression::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_object_literal(&mut self) -> Result<Expression, String> {
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();
        if self.current_token() != &Token::RBrace {
            loop {
                let key = match self.current_token() {
                    Token::Identifier(name) => {
                        let k = name.clone();
                        self.advance();
                        k
                    }
                    _ => return Err("Expected field name in object literal".to_string()),
                };
                self.expect(Token::Colon)?;
                let value = self.parse_expression()?;
                fields.push((key, value));
                if self.current_token() == &Token::RBrace {
                    break;
                }
                if self.current_token() == &Token::Comma {
                    self.advance();
                }
            }
        }
        self.expect(Token::RBrace)?;
        Ok(Expression::Literal(Literal::Object { fields }))
    }

    fn parse_array_literal(&mut self, mutable: bool) -> Result<Expression, String> {
        self.expect(Token::LBracket)?;
        let mut elements = Vec::new();
        if self.current_token() != &Token::RBracket {
            loop {
                elements.push(self.parse_expression()?);
                if self.current_token() == &Token::RBracket {
                    break;
                }
                if self.current_token() == &Token::Comma {
                    self.advance();
                }
            }
        }
        self.expect(Token::RBracket)?;
        Ok(Expression::Literal(Literal::Array { elements, mutable }))
    }

    fn parse_primary(&mut self) -> Result<Expression, String> {
        match self.current_token() {
            Token::Number(n) => {
                let n_val = *n;
                self.advance();
                Ok(Expression::Literal(Literal::Number(n_val)))
            }
            Token::String(s) => {
                let s_val = s.clone();
                self.advance();
                Ok(Expression::Literal(Literal::String(s_val)))
            }
            Token::True => {
                self.advance();
                Ok(Expression::Literal(Literal::Boolean(true)))
            }
            Token::False => {
                self.advance();
                Ok(Expression::Literal(Literal::Boolean(false)))
            }
            Token::Null => {
                self.advance();
                Ok(Expression::Literal(Literal::Null))
            }
            Token::Identifier(name) => {
                let name_val = name.clone();
                self.advance();

                // Check for input<type>("prompt")
                if name_val == "input" && self.current_token() == &Token::Less {
                    self.advance(); // consume <
                    let types = self.parse_input_types()?;
                    self.expect(Token::Greater)?;
                    self.expect(Token::LParen)?;
                    let prompt = self.parse_expression()?;
                    self.expect(Token::RParen)?;

                    Ok(Expression::Input {
                        types,
                        prompt: Box::new(prompt),
                    })
                }
                // Check for function call
                else if self.current_token() == &Token::LParen {
                    self.advance();
                    let args = self.parse_argument_list()?;
                    self.expect(Token::RParen)?;
                    self.parse_postfix(Expression::FunctionCall {
                        name: name_val,
                        args,
                    })
                } else {
                    self.parse_postfix(Expression::Identifier(name_val))
                }
            }
            Token::Fn => {
                self.advance();
                self.expect(Token::LParen)?;
                let params = self.parse_parameter_list()?;
                self.expect(Token::RParen)?;

                while self.current_token() == &Token::Newline {
                    self.advance();
                }

                let body = self.parse_block()?;
                Ok(Expression::FunctionExpr { params, body })
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(Token::RParen)?;
                self.parse_postfix(expr)
            }
            Token::LBracket => self.parse_array_literal(true),
            Token::Hash => {
                self.advance();
                self.parse_array_literal(false)
            }
            Token::LBrace => self.parse_object_literal(),
            _ => Err(format!(
                "Unexpected token in expression: {:?}",
                self.current_token()
            )),
        }
    }

    fn parse_argument_list(&mut self) -> Result<Vec<Expression>, String> {
        let mut args = Vec::new();

        while self.current_token() != &Token::RParen {
            args.push(self.parse_expression()?);

            if self.current_token() == &Token::RParen {
                break;
            }

            if matches!(self.current_token(), Token::Comma) {
                self.advance();
            }
        }

        Ok(args)
    }

    fn parse_input_types(&mut self) -> Result<Vec<crate::ast::InputType>, String> {
        use crate::ast::InputType;
        let mut types = Vec::new();

        loop {
            match self.current_token() {
                Token::Identifier(name) => {
                    let type_name = name.clone();
                    self.advance();

                    let input_type = match type_name.as_str() {
                        "str" => InputType::String,
                        "int" => InputType::Int,
                        "float" => InputType::Float,
                        _ => return Err(format!("Unknown input type: {}", type_name)),
                    };
                    types.push(input_type);

                    if self.current_token() == &Token::Pipe {
                        self.advance();
                    } else {
                        break;
                    }
                }
                _ => return Err("Expected input type (str, int, or float)".to_string()),
            }
        }

        if types.is_empty() {
            return Err("At least one input type is required".to_string());
        }

        Ok(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    #[test]
    fn object_literal_in_assignment() {
        let mut lexer = Lexer::new("set x = { a: 1 }");
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        parser.parse().expect("parse object assignment");
    }
}
