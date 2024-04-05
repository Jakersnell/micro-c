use crate::data::ast::{BinaryOp, Block, TypeSpecifier};
use crate::util::error::CompilerError;
use crate::util::str_intern::InternedStr;
use crate::util::{Locatable, Span};
use arcstr::ArcStr;
use derive_new::new;
use std::cell::OnceCell;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::hash::Hasher;
use std::ops::Deref;
use std::sync::{Mutex, MutexGuard, OnceLock};

pub const VOID_TYPE: MlirType = MlirType {
    kind: MlirTypeKind::Void,
    decl: MlirTypeDecl::Basic,
};
#[derive(Debug, Default, PartialEq)]
pub struct MidLevelIR {
    pub functions: HashMap<InternedStr, MlirFunction>,
    pub structs: HashMap<InternedStr, MlirStruct>,
    pub globals: HashMap<InternedStr, MlirVariable>,
}

#[derive(Debug, PartialEq)]
pub struct MlirStruct {
    pub ident: Locatable<InternedStr>,
    pub fields: Vec<Locatable<MlirVariable>>,
    pub size: u64,
}

#[derive(Debug, PartialEq)]
pub struct MlirFunction {
    pub span: Span,
    pub ty: Locatable<MlirType>,
    pub ident: Locatable<InternedStr>,
    pub parameters: Vec<Locatable<MlirVariable>>,
    pub body: Locatable<MlirBlock>,
}

#[derive(Debug, PartialEq, Hash, PartialOrd, Eq)]
pub struct MlirVariable {
    pub span: Span,
    pub ty: Locatable<MlirType>,
    pub ident: Locatable<InternedStr>,
    pub is_const: bool,
    pub initializer: Option<Locatable<MlirVarInit>>,
}

#[derive(Debug, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirVarInit {
    Expr(MlirExpr),
    Array(Vec<MlirExpr>),
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, new, Eq)]
pub struct MlirType {
    pub(crate) kind: MlirTypeKind,
    pub(crate) decl: MlirTypeDecl,
}

impl Display for MlirType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.kind, self.decl)
    }
}

impl MlirType {
    pub fn is_array(&self) -> bool {
        matches!(self.decl, MlirTypeDecl::Array(_)) && !matches!(&self.decl, MlirTypeDecl::Pointer)
    }

    pub fn is_pointer(&self) -> bool {
        matches!(self.decl, MlirTypeDecl::Pointer)
    }

    pub fn is_numeric(&self) -> bool {
        use MlirTypeKind::*;
        matches!(&self.kind, Char(_) | Int(_) | Long(_) | Float | Double)
            && !matches!(&self.decl, MlirTypeDecl::Pointer)
            && !matches!(&self.decl, MlirTypeDecl::Array(_))
    }

    pub fn is_integer(&self) -> bool {
        use MlirTypeKind::*;
        matches!(&self.kind, Char(_) | Int(_) | Long(_)) && !self.is_pointer()
    }
    pub fn try_implicit_cast(&self, to: &MlirType) -> Option<MlirType> {
        if self == to {
            return None;
        }
        use MlirTypeDecl::*;
        use MlirTypeKind::*;
        let ty_kind = match (&self.kind, &to.kind, &self.decl, &to.decl) {
            (Char(_), Int(unsigned), Basic, Basic) => Some(Int(*unsigned)),
            (Char(_), Long(unsigned), Basic, Basic) => Some(Long(*unsigned)),
            (Char(_), Double, Basic, Basic) => Some(Double),
            (Int(_), Long(unsigned), Basic, Basic) => Some(Long(*unsigned)),
            (Int(_), Double, Basic, Basic) => Some(Double),
            _ => None,
        };
        ty_kind.map(|kind| MlirType::new(kind, Basic))
    }
    pub fn try_explicit_cast(&self, other: &MlirType) -> Option<MlirType> {
        let implicit = self.try_implicit_cast(other);
        if implicit.is_some() {
            return implicit;
        }

        use MlirTypeDecl::*;
        use MlirTypeKind::*;
        let ty_kind = match (&self.kind, &other.kind, &self.decl, &other.decl) {
            (Int(_), Char(unsigned), Basic, Basic) => Some(Char(*unsigned)),
            (Long(_), Char(unsigned), Basic, Basic) => Some(Char(*unsigned)),
            (Double, Char(unsigned), Basic, Basic) => Some(Char(*unsigned)),
            (Long(_), Int(unsigned), Basic, Basic) => Some(Int(*unsigned)),
            (Int(_), Double, Basic, Basic) => Some(Double),
            (Long(_), Double, Basic, Basic) => Some(Double),
            (_, Long(unsigned), Pointer, Basic) => Some(Long(*unsigned)), // all pointers can cast to long
            _ => None,
        };

        ty_kind.map(|kind| MlirType::new(kind, Basic))
    }

    pub fn is_basic(&self) -> bool {
        self.decl == MlirTypeDecl::Basic
    }
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirTypeDecl {
    Basic,
    Pointer, // true if pointer is const
    Array(u64),
}

impl Display for MlirTypeDecl {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use MlirTypeDecl::*;
        match self {
            Basic => Ok(()),
            Pointer => write!(f, " *"),
            Array(size) => write!(f, " [{}]", size),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirTypeKind {
    Void,
    Char(bool), // 8
    Int(bool),  // signed/unsigned
    Long(bool), // i64
    Float,
    Double,
    Struct(InternedStr),
}

impl Display for MlirTypeKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        macro_rules! write_signed {
            ($name:literal, $unsigned:expr) => {
                write!(
                    f,
                    "{} {}",
                    if $unsigned { "unsigned" } else { "signed" },
                    $name
                )
            };
        }
        use MlirTypeKind::*;
        match self {
            Void => write!(f, "void"),
            Char(unsigned) => write_signed!("char", *unsigned),
            Int(unsigned) => write_signed!("int", *unsigned),
            Long(unsigned) => write_signed!("long", *unsigned),
            Float => write!(f, "float"),
            Double => write!(f, "double"),
            Struct(ident) => write!(f, "struct {}", ident),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum MlirLiteral {
    Int(i64),
    UInt(u64),
    Float(f64),
    String(Vec<u8>),
    Char(u8),
}

impl std::cmp::Eq for MlirLiteral {}

impl std::hash::Hash for MlirLiteral {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MlirLiteral::Int(int) => int.hash(state),
            MlirLiteral::UInt(int) => int.hash(state),
            MlirLiteral::String(string) => string.hash(state),
            MlirLiteral::Char(char) => char.hash(state),
            MlirLiteral::Float(float) => float.to_ne_bytes().hash(state),
        }
    }
}

#[derive(Debug, Clone, new, PartialEq, Hash, PartialOrd, Eq)]
pub struct MlirExpr {
    pub span: Span,
    pub kind: Box<MlirExprKind>,
    pub ty: MlirType,
    pub is_lval: bool,
}

impl MlirExpr {
    pub fn is_literal(&self) -> bool {
        matches!(self.kind.as_ref(), MlirExprKind::Literal(_))
    }
    pub fn is_string(&self) -> bool {
        matches!(
            self.kind.as_ref(),
            MlirExprKind::Literal(MlirLiteral::String(_))
        )
    }
    pub fn is_integer_or_pointer(&self) -> bool {
        self.ty.is_integer() || self.ty.is_pointer()
    }

    pub fn is_integer(&self) -> bool {
        self.ty.is_integer()
    }

    pub fn is_pointer(&self) -> bool {
        self.ty.is_pointer()
    }

    pub fn is_array(&self) -> bool {
        self.ty.is_array()
    }

    pub fn is_numeric(&self) -> bool {
        self.ty.is_numeric()
    }
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirExprKind {
    Literal(MlirLiteral),
    Variable(InternedStr),

    // unary
    PostIncrement(MlirExpr),
    PostDecrement(MlirExpr),
    Negate(MlirExpr),
    LogicalNot(MlirExpr),
    BitwiseNot(MlirExpr),
    Deref(MlirExpr),
    AddressOf(MlirExpr),

    // binary
    Add(MlirExpr, MlirExpr),
    Sub(MlirExpr, MlirExpr),
    Mul(MlirExpr, MlirExpr),
    Div(MlirExpr, MlirExpr),
    Mod(MlirExpr, MlirExpr),
    Equal(MlirExpr, MlirExpr),
    NotEqual(MlirExpr, MlirExpr),
    GreaterThan(MlirExpr, MlirExpr),
    GreaterThanEqual(MlirExpr, MlirExpr),
    LessThan(MlirExpr, MlirExpr),
    LessThanEqual(MlirExpr, MlirExpr),
    LogicalAnd(MlirExpr, MlirExpr),
    LogicalOr(MlirExpr, MlirExpr),
    BitwiseAnd(MlirExpr, MlirExpr),
    BitwiseOr(MlirExpr, MlirExpr),
    BitwiseXor(MlirExpr, MlirExpr),
    LeftShift(MlirExpr, MlirExpr),
    RightShift(MlirExpr, MlirExpr),

    // assign
    Assign(MlirExpr, MlirExpr),

    // other
    FunctionCall {
        location: Option<&'static str>,
        ident: InternedStr,
        args: Vec<MlirExpr>,
    },
    Index(MlirExpr, MlirExpr),
    Member(MlirExpr, InternedStr),
    Cast(MlirType, MlirExpr),
}

impl MlirExprKind {
    pub fn is_literal(&self) -> bool {
        matches!(self, MlirExprKind::Literal(_))
    }
    pub fn try_from_binop(op: BinaryOp, left: MlirExpr, right: MlirExpr) -> Result<Self, ()> {
        match op {
            BinaryOp::Add => Ok(MlirExprKind::Add(left, right)),
            BinaryOp::Sub => Ok(MlirExprKind::Sub(left, right)),
            BinaryOp::Mul => Ok(MlirExprKind::Mul(left, right)),
            BinaryOp::Div => Ok(MlirExprKind::Div(left, right)),
            BinaryOp::Mod => Ok(MlirExprKind::Mod(left, right)),
            BinaryOp::Equal => Ok(MlirExprKind::Equal(left, right)),
            BinaryOp::NotEqual => Ok(MlirExprKind::NotEqual(left, right)),
            BinaryOp::GreaterThan => Ok(MlirExprKind::GreaterThan(left, right)),
            BinaryOp::GreaterThanEqual => Ok(MlirExprKind::GreaterThanEqual(left, right)),
            BinaryOp::LessThan => Ok(MlirExprKind::LessThan(left, right)),
            BinaryOp::LessThanEqual => Ok(MlirExprKind::LessThanEqual(left, right)),
            BinaryOp::LogicalAnd => Ok(MlirExprKind::LogicalAnd(left, right)),
            BinaryOp::LogicalOr => Ok(MlirExprKind::LogicalOr(left, right)),
            BinaryOp::BitwiseAnd => Ok(MlirExprKind::BitwiseAnd(left, right)),
            BinaryOp::BitwiseOr => Ok(MlirExprKind::BitwiseOr(left, right)),
            BinaryOp::BitwiseXor => Ok(MlirExprKind::BitwiseXor(left, right)),
            BinaryOp::LeftShift => Ok(MlirExprKind::LeftShift(left, right)),
            BinaryOp::RightShift => Ok(MlirExprKind::RightShift(left, right)),
            _ => Err(()),
        }
    }
}

#[derive(Debug, PartialEq, Hash, PartialOrd)]
pub struct MlirBlock(pub Vec<MlirStmt>);
impl std::cmp::Eq for MlirBlock {}
impl Deref for MlirBlock {
    type Target = Vec<MlirStmt>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirStmt {
    Block(MlirBlock),
    Expression(MlirExpr),
    VariableDeclaration(MlirVariable),
    Label(InternedStr),
    Goto(InternedStr),
    GotoTrue(MlirExpr, InternedStr),
    GotoFalse(MlirExpr, InternedStr),
    Return(Option<MlirExpr>),
}

impl MlirStmt {
    pub fn type_to_string(&self) -> String {
        match self {
            MlirStmt::Block(_) => "block",
            MlirStmt::Expression(_) => "expression",
            MlirStmt::VariableDeclaration(_) => "declaration",
            MlirStmt::Label(_) => "label",
            MlirStmt::Goto(_) => "goto",
            MlirStmt::GotoTrue(_, _) => "goto true",
            MlirStmt::GotoFalse(_, _) => "goto false",
            MlirStmt::Return(_) => "return",
        }
        .to_string()
    }
}