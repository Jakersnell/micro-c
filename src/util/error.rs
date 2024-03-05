use crate::util::Span;
use thiserror::Error;

pub trait ErrorReporter {
    fn get_status(&self) -> Result<(), ()>;
    fn report_error(&mut self, error: CompilerError);
    fn report_warning(&mut self, warning: CompilerWarning);
    fn get_errors(&self) -> &Vec<CompilerError>;
    fn get_warnings(&self) -> &Vec<CompilerWarning>;
}

pub enum ProgramErrorStatus {
    MustStop, // must stop immediately
    Current,  // can continue this phase but not next
    Lexer,
    Parser,
    Analysis,
    Codegen,
}

pub struct ErrorReporterImpl {
    errors: Vec<CompilerError>,
    warnings: Vec<CompilerWarning>,
}

impl ErrorReporter for ErrorReporterImpl {
    fn get_status(&self) -> Result<(), ()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(())
        }
    }

    fn report_error(&mut self, error: CompilerError) {
        println!("{}", error);
        self.errors.push(error);
    }

    fn report_warning(&mut self, warning: CompilerWarning) {
        println!("{}", warning);
        self.warnings.push(warning);
    }

    fn get_errors(&self) -> &Vec<CompilerError> {
        &self.errors
    }

    fn get_warnings(&self) -> &Vec<CompilerWarning> {
        &self.warnings
    }
}

#[derive(Error, Debug)]
pub enum CompilerError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid integer literal: {0}")]
    ParseIntError(Span),

    #[error("Invalid float literal: {0}")]
    ParseFloatError(Span),

    #[error("Invalid integer suffix: {0}")]
    InvalidIntegerSuffix(String, Span),

    #[error("Invalid float suffix: {0}")]
    InvalidFloatSuffix(String, Span),

    #[error("Invalid symbol: {0}")]
    InvalidSymbol(String, Span),

    #[error("Found `{0}` but expected one of the following: \n\t{1}\n")]
    ExpectedVariety(String, String, Span),

    #[error("Expected `{0}` but found {1}")]
    ExpectedButFound(String, String, Span),

    #[error("Invalid hex literal: {0}")]
    InvalidHexLiteral(Span),

    #[error("Invalid octal literal: {0}")]
    InvalidOctalLiteral(Span),

    #[error("Invalid binary literal: {0}")]
    InvalidBinaryLiteral(Span),

    #[error("Invalid escape sequence: {0}")]
    InvalidEscapeSequence(Span),

    #[error("Invalid character literal: {0}")]
    InvalidCharacterLiteral(Span),

    #[error("Unclosed string literal: {0}")]
    UnclosedStringLiteral(Span),

    #[error("Unclosed char literal: {0}")]
    UnclosedCharLiteral(Span),

    #[error("Cannot cast {0} to {1}")]
    CannotCast(String, String, Span),

    #[error("Cannot assign {0} to type {1}")]
    CannotAssign(String, String, Span),

    #[error("Unknown identifier \"{0}\"")]
    UnknownIdentifier(String, Span),

    #[error("Must return type {0} due to declared type")]
    MustReturn(String, Span),

    #[error("Unclosed parenthesis")]
    UnclosedParenthesis,

    #[error("Unclosed block")]
    UnclosedBlock,

    #[error("Unclosed array")]
    UnclosedArray,

    #[error("Parenthesis has no opening.")]
    ParenthesisHasNoOpening,

    #[error("Curly has no opening.")]
    BlockHasNoOpening,

    #[error("Unexpected end of file.")]
    UnexpectedEOF,

    #[error("'This identifier already exists in this scope and cannot be redeclared.")]
    IdentifierExists(Span),

    #[error("{0}")]
    CustomError(String, Span),

    #[error("Else without if")]
    ElseWithNoIf(Span),
}

#[derive(Error, Debug)]
pub enum CompilerWarning {
    #[error("Unused variable: {0}")]
    UnusedVariable(Span),

    #[error("Unused function: {0}")]
    UnusedFunction(Span),

    #[error("Unused parameter: {0}")]
    UnusedParameter(Span),

    #[error("Unused constant: {0}")]
    UnusedConstant(Span),

    #[error("Unused struct: {0}")]
    UnusedStruct(Span),

    #[error("Unreachable code: {0}")]
    UnreachableCode(Span),

    #[error("Variable is not initialized at this point: {0}")]
    UninitializedVariable(Span),
}