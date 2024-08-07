use std::fmt::{Display, Formatter};
use std::hash::Hasher;
use std::ops::{Deref, DerefMut};

use derive_new::new;

use crate::data::ast::BinaryOp;
use crate::util::{Locatable, Span};
use crate::util::str_intern::InternedStr;

macro_rules! basic_ty {
    ($kind:expr) => {
        MlirType {
            kind: $kind,
            decl: MlirTypeDecl::Basic,
        }
    };
}
pub(crate) use basic_ty;

pub const VOID_PTR: MlirType = MlirType {
    kind: MlirTypeKind::Void,
    decl: MlirTypeDecl::Pointer,
};

pub const UNSIGNED_LONG_TYPE: MlirType = MlirType {
    kind: MlirTypeKind::Long(true),
    decl: MlirTypeDecl::Basic,
};

pub const SIGNED_INT_TYPE: MlirType = MlirType {
    kind: MlirTypeKind::Int(false),
    decl: MlirTypeDecl::Basic,
};

pub const VOID_TYPE: MlirType = MlirType {
    kind: MlirTypeKind::Void,
    decl: MlirTypeDecl::Basic,
};

#[derive(Debug, Default, PartialEq)]
pub struct MlirModule {
    pub functions: Vec<MlirFunction>,
    pub structs: Vec<MlirStruct>,
    pub globals: Vec<MlirVariable>,
}

impl MlirModule {
    pub fn get_struct(&self, ident: &str) -> Option<&MlirStruct> {
        self.structs
            .iter()
            .find(|_struct| _struct.ident.value.as_ref() == ident)
    }
    pub fn get_struct_member_index(&self, struct_ident: &InternedStr, member: &InternedStr) -> u32 {
        self.get_struct(struct_ident)
            .expect("Struct not found in module!")
            .get_member_offset(member)
    }

    pub fn get_struct_member_type(
        &self,
        struct_ident: &InternedStr,
        member: &InternedStr,
    ) -> &MlirType {
        self.get_struct(struct_ident)
            .unwrap()
            .get_member_type(member)
    }
}

#[derive(Debug, PartialEq)]
pub struct MlirStruct {
    pub ident: Locatable<InternedStr>,
    pub members: Vec<Locatable<MlirVariable>>,
    pub size: u64,
}

impl MlirStruct {
    fn get_member_offset(&self, member: &str) -> u32 {
        self.members
            .iter()
            .enumerate()
            .find(|(_, var)| var.ident.as_ref() == member)
            .map(|(idx, _)| idx as u32)
            .unwrap_or_else(|| {
                panic!(
                    "Struct member '{member}' does not exist in struct '{}'",
                    &self.ident.value
                )
            })
    }

    fn get_member_type(&self, member: &str) -> &MlirType {
        self.members
            .iter()
            .find(|var| var.ident.as_ref() == member)
            .map(|var| &var.ty)
            .unwrap_or_else(|| {
                panic!(
                    "Struct member '{member}' does not exist in struct '{}'",
                    &self.ident.value
                )
            })
    }
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
    pub uid: usize,
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
    #[inline]
    pub fn is_float(&self) -> bool {
        self.is_basic() && matches!(self.kind, MlirTypeKind::Float | MlirTypeKind::Double)
    }

    pub fn get_struct_ident(&self) -> &InternedStr {
        debug_assert!(self.is_basic());
        match &self.kind {
            MlirTypeKind::Struct(ident) => ident,
            unexpected => panic!(
                "Cannot unwrap '{:?}' as an MlirTypeKind::Struct(ident).",
                unexpected
            ),
        }
    }
    #[inline]
    pub fn is_unsigned_int(&self) -> bool {
        self.is_basic()
            && matches!(
                &self.kind,
                MlirTypeKind::Char(true) | MlirTypeKind::Int(true) | MlirTypeKind::Long(true)
            )
    }
    #[inline]
    pub fn as_basic(&self) -> Self {
        Self {
            decl: MlirTypeDecl::Basic,
            kind: self.kind.clone(),
        }
    }
    #[inline]
    pub fn is_array(&self) -> bool {
        matches!(self.decl, MlirTypeDecl::Array(_)) && !matches!(&self.decl, MlirTypeDecl::Pointer)
    }
    #[inline]
    pub fn is_pointer(&self) -> bool {
        matches!(self.decl, MlirTypeDecl::Pointer)
    }
    #[inline]
    pub fn is_numeric(&self) -> bool {
        self.kind.is_numeric()
            && !matches!(&self.decl, MlirTypeDecl::Pointer)
            && !matches!(&self.decl, MlirTypeDecl::Array(_))
    }

    pub fn get_is_unsigned(&self) -> bool {
        self.kind.get_is_unsigned()
    }

    #[inline]
    pub fn is_integer(&self) -> bool {
        self.kind.is_integer()
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

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Copy)]
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

impl MlirTypeKind {
    pub fn is_numeric(&self) -> bool {
        use MlirTypeKind::*;
        matches!(&self, Char(_) | Int(_) | Long(_) | Float | Double)
    }

    pub fn get_struct_ident(&self) -> &InternedStr {
        match self {
            MlirTypeKind::Struct(ident) => ident,
            _ => panic!(
                "'{:?} cannot be unwrapped as MlirTypeKind::Struct(ident).'",
                self
            ),
        }
    }

    pub fn is_integer(&self) -> bool {
        use MlirTypeKind::*;
        matches!(self, Char(_) | Int(_) | Long(_))
    }

    pub fn get_is_unsigned(&self) -> bool {
        use MlirTypeKind::*;
        match self {
            Char(is_unsigned) | Int(is_unsigned) | Long(is_unsigned) => *is_unsigned,
            _ => panic!("Type is not integer."),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum MlirLiteral {
    Char(i8),
    UChar(u8),
    Int(i32),
    UInt(u32),
    Long(i64),
    ULong(u64),
    Float(f32),
    Double(f64),
    String(Vec<u8>),
}

impl std::cmp::Eq for MlirLiteral {}

impl std::hash::Hash for MlirLiteral {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            MlirLiteral::Long(int) => int.hash(state),
            MlirLiteral::ULong(int) => int.hash(state),
            MlirLiteral::String(string) => string.hash(state),
            MlirLiteral::UChar(char) => char.hash(state),
            MlirLiteral::Double(float) => float.to_ne_bytes().hash(state),
            MlirLiteral::Char(char) => char.hash(state),
            MlirLiteral::Int(int) => int.hash(state),
            MlirLiteral::UInt(int) => int.hash(state),
            MlirLiteral::Float(float) => float.to_ne_bytes().hash(state),
        }
    }
}

#[derive(Debug, Clone, new, PartialEq, Hash, PartialOrd, Eq)]
pub struct MlirExpr {
    pub span: Span,
    pub ty: MlirType,
    pub is_lval: bool,
    pub kind: Box<MlirExprKind>,
}

impl MlirExpr {
    #[inline]
    pub fn is_literal(&self) -> bool {
        matches!(self.kind.as_ref(), MlirExprKind::Literal(_))
    }
    #[inline]
    pub fn is_string(&self) -> bool {
        matches!(
            self.kind.as_ref(),
            MlirExprKind::Literal(MlirLiteral::String(_))
        )
    }

    #[inline]
    pub fn is_integer_or_pointer(&self) -> bool {
        self.ty.is_integer() || self.ty.is_pointer()
    }

    #[inline]
    pub fn is_integer(&self) -> bool {
        self.ty.is_integer()
    }

    #[inline]
    pub fn is_pointer(&self) -> bool {
        self.ty.is_pointer()
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        self.ty.is_array()
    }

    #[inline]
    pub fn is_numeric(&self) -> bool {
        self.ty.is_numeric()
    }
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirExprKind {
    Literal(MlirLiteral),
    Variable(usize),

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
    Cast(MlirType, CastType, MlirExpr),
}

#[derive(Debug, Clone, PartialEq, Hash, PartialOrd, Eq, Copy)]
pub enum CastType {
    InvalidCast,
    ArrayToPointer,
    PointerToPointer,
    PointerToInt,
    IntToPointer,
    SignedToUnsigned,
    UnsignedToSigned,
    IntToInt,
    IntToFloat,
    FloatToInt,
    FloatToFloat,
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

impl DerefMut for MlirBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, PartialEq, Hash, PartialOrd, Eq)]
pub enum MlirStmt {
    Block(MlirBlock),
    Expression(MlirExpr),
    VariableDeclaration(MlirVariable),
    Label(InternedStr),
    Goto(InternedStr),
    CondGoto(MlirExpr, InternedStr, InternedStr),
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
            MlirStmt::CondGoto(_, _, _) => "goto false",
            MlirStmt::Return(_) => "return",
        }
        .to_string()
    }
}
