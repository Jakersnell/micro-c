mod binary_expressions;
mod casting;
mod declarations;
mod expressions;
pub mod hlir;
mod statements;
mod symbols;

use crate::analysis::hlir::*;
use crate::analysis::symbols::SymbolResolver;
use crate::parser::ast::*;
use crate::util::error::{CompilerError, CompilerWarning, Reporter};
use crate::util::str_intern::InternedStr;
use crate::util::{Locatable, Span};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::DerefMut;
use std::rc::Rc;

pub type SharedReporter = Rc<RefCell<Reporter>>;

pub struct GlobalValidator {
    ast: Option<AbstractSyntaxTree>,
    scope: Box<RefCell<SymbolResolver>>,
    reporter: SharedReporter,
}

impl GlobalValidator {
    pub fn new(ast: AbstractSyntaxTree) -> Self {
        Self {
            ast: Some(ast),
            scope: Box::new(RefCell::new(SymbolResolver::create_root())),
            reporter: Rc::new(RefCell::new(Reporter::default())),
        }
    }

    pub fn validate(mut self) -> Result<HighLevelIR, SharedReporter> {
        let mut globals = HashMap::new();
        let mut functions = HashMap::new();
        let mut structs = HashMap::new();
        let ast = self.ast.take().expect("Ast must be Some(T)");
        for node in &*ast {
            use crate::parser::ast::InitDeclaration::*;
            match node {
                Declaration(locatable_variable) => {
                    if let Ok(result) = self.validate_variable_declaration(locatable_variable) {
                        globals.insert(
                            result.ident.clone(),
                            locatable_variable.location.into_locatable(result),
                        );
                    }
                }
                Function(locatable_function) => {
                    if let Ok(result) = self.validate_function_definition(locatable_function) {
                        functions.insert(
                            result.ident.clone(),
                            locatable_function.location.into_locatable(result),
                        );
                    }
                }
                Struct(locatable_struct) => {
                    if let Ok(result) = self.validate_struct_definition(locatable_struct) {
                        structs.insert(
                            result.ident.clone(),
                            locatable_struct.location.into_locatable(result),
                        );
                    }
                }
            }
        }
        let idents = self.scope.borrow().get_unused_idents();
        for ident in idents {
            if let Some(variable) = globals.get(&ident) {
                self.report_warning(CompilerWarning::UnusedVariable(variable.location));
            } else if let Some(function) = functions.get(&ident) {
                self.report_warning(CompilerWarning::UnusedFunction(function.location));
            } else if let Some(_struct) = structs.get(&ident) {
                self.report_warning(CompilerWarning::UnusedStruct(_struct.location))
            } else {
                panic!("Could not match unused ident to type: ident = '{}'", ident);
            }
        }
        fn deref_map<K: Eq + std::hash::Hash, T>(map: HashMap<K, Locatable<T>>) -> HashMap<K, T> {
            map.into_iter()
                .map(|(ident, item)| (ident, item.value))
                .collect()
        }
        let functions = deref_map(functions);
        let structs = deref_map(structs);
        let globals = deref_map(globals);
        if self.reporter.borrow().status().is_err() {
            Err(self.reporter)
        } else {
            Ok(HighLevelIR {
                functions,
                structs,
                globals,
            })
        }
    }

    fn report_error(&mut self, error: CompilerError) -> Result<(), ()> {
        self.reporter.borrow_mut().report_error(error);
        Err(())
    }

    fn report_warning(&mut self, warning: CompilerWarning) {
        self.reporter.borrow_mut().report_warning(warning);
    }

    fn add_variable_to_scope(&mut self, var: &HlirVariable, span: Span) -> Result<(), ()> {
        let array_size = match &var.ty.decl {
            HlirTypeDecl::Array(size) => Some(*size),
            _ => None,
        };
        let result = self.scope.borrow_mut().add_variable(
            &var.ident,
            &var.ty,
            var.is_const,
            var.initializer.is_some(),
            array_size,
            span,
        );
        if let Err(err) = result {
            self.report_error(err);
        }
        Ok(())
    }

    fn push_scope(&mut self) {
        let mut resolver = Box::new(RefCell::new(SymbolResolver::default())); // blank temp resolver
        std::mem::swap(&mut resolver, &mut self.scope);
        resolver = Box::new(RefCell::new(SymbolResolver::new(Some(resolver))));
        std::mem::swap(&mut resolver, &mut self.scope);
        debug_assert!(resolver.borrow().parent.is_none());
        debug_assert!(resolver.borrow().symbols.is_empty());
    }

    fn pop_scope(&mut self) {
        let mut resolver = Box::new(RefCell::new(SymbolResolver::default())); // blank temp resolver
        std::mem::swap(&mut resolver, &mut self.scope);
        resolver = (*resolver)
            .take()
            .remove_self()
            .expect("Popped scope on global scope.");
        std::mem::swap(&mut resolver, &mut self.scope);
        debug_assert!(resolver.borrow().parent.is_none());
        debug_assert!(resolver.borrow().symbols.is_empty());
    }

    fn validate_type(
        &mut self,
        declaration: &DeclarationSpecifier,
        location: Span,
        is_function_return_ty: bool,
    ) -> Result<HlirType, ()> {
        #[derive(Debug, PartialEq)]
        enum State {
            Start,
            SeenUnsigned,
            SeenSigned,
            End,
        }
        let mut hlir_type: Option<HlirTypeKind> = None;
        let mut state = State::Start;
        let mut iter = declaration.ty.iter();
        macro_rules! seen_signed_or_unsigned {
            ($ty_spec:ident, $unsigned:literal) => {
                match $ty_spec {
                    Some(TypeSpecifier::Char) => {
                        hlir_type = Some(HlirTypeKind::Char($unsigned));
                        state = State::End;
                    }
                    Some(TypeSpecifier::Int) => {
                        hlir_type = Some(HlirTypeKind::Int($unsigned));
                        state = State::End;
                    }
                    Some(TypeSpecifier::Long) => {
                        hlir_type = Some(HlirTypeKind::Long($unsigned));
                        state = State::End;
                    }
                    Some(ty) => {
                        let err =
                            CompilerError::TypeCannotBeSignedOrUnsigned(ty.to_string(), location);
                        self.report_error(err);
                        return Err(());
                    }
                    None => {
                        let err = CompilerError::ExpectedTypeSpecifier(location);
                        self.report_error(err);
                        return Err(());
                    }
                }
            };
        }

        loop {
            let mut ty_spec = if state != State::End {
                iter.next()
            } else {
                None
            };

            match state {
                State::Start => match ty_spec {
                    Some(TypeSpecifier::Void) => {
                        hlir_type = Some(HlirTypeKind::Void);
                        state = State::End;
                    }
                    Some(TypeSpecifier::Char) => {
                        hlir_type = Some(HlirTypeKind::Char(false));
                        state = State::End;
                    }
                    Some(TypeSpecifier::Int) => {
                        hlir_type = Some(HlirTypeKind::Int(false));
                        state = State::End;
                    }
                    Some(TypeSpecifier::Long) => {
                        hlir_type = Some(HlirTypeKind::Long(false));
                        state = State::End;
                    }
                    Some(TypeSpecifier::Double) => {
                        hlir_type = Some(HlirTypeKind::Double);
                        state = State::End;
                    }
                    Some(TypeSpecifier::Struct(ident)) => {
                        hlir_type = Some(HlirTypeKind::Struct(ident.clone()));
                        state = State::End;
                    }
                    Some(TypeSpecifier::Signed) => {
                        state = State::SeenSigned;
                    }
                    Some(TypeSpecifier::Unsigned) => {
                        state = State::SeenUnsigned;
                    }
                    None => panic!("Span: {:?}", location),
                },
                State::SeenUnsigned => seen_signed_or_unsigned!(ty_spec, true),
                State::SeenSigned => seen_signed_or_unsigned!(ty_spec, false),
                State::End => break,
            }
        }
        debug_assert!(hlir_type.is_some());
        if iter.next().is_some() {
            let err = CompilerError::InvalidTypeSpecifier(location);
            self.report_error(err);
            return Err(());
        }
        let ty_kind = hlir_type.unwrap();

        let ty_dec = if declaration.pointer {
            HlirTypeDecl::Pointer
        } else {
            HlirTypeDecl::Basic
        };

        if !is_function_return_ty
            && matches!(ty_kind, HlirTypeKind::Void)
            && !matches!(ty_dec, HlirTypeDecl::Pointer)
        {
            self.report_error(CompilerError::IncompleteType(location));
            return Err(());
        }

        Ok(HlirType {
            kind: ty_kind,
            decl: ty_dec,
        })
    }
}

#[cfg(test)]
macro_rules! make_dec_specifier {
    ($types:expr, $is_ptr:expr) => {
        DeclarationSpecifier {
            specifiers: vec![],
            qualifiers: vec![],
            ty: $types,
            pointer: $is_ptr,
        }
    };
}

#[test]
fn test_validate_type_returns_error_for_invalid_type_orientations() {
    use TypeSpecifier::*;
    let type_tests = [
        vec![Int, Int, Int],
        vec![Long, Int, Int],
        vec![Int, Long, Int],
        vec![Unsigned, Double],
        vec![Unsigned, Signed, Int],
        vec![Signed, Unsigned, Long],
        vec![Signed, Double],
        vec![Void],
    ];
    for types in type_tests {
        let mut validator = GlobalValidator::new(AbstractSyntaxTree::default());
        let types = make_dec_specifier!(types, false);
        let result = validator.validate_type(&types, Span::default(), false);
        if result.is_ok() {
            panic!("Expected error, got ok, test: {:?}", types);
        }
    }
}

#[test]
fn test_validate_type_returns_ok_for_valid_type_orientations() {
    use TypeSpecifier::*;
    let type_tests = [
        (vec![Int], HlirTypeKind::Int(false), HlirTypeDecl::Basic),
        (vec![Long], HlirTypeKind::Long(false), HlirTypeDecl::Basic),
        (
            vec![Unsigned, Int],
            HlirTypeKind::Int(true),
            HlirTypeDecl::Basic,
        ),
        (
            vec![Unsigned, Long],
            HlirTypeKind::Long(true),
            HlirTypeDecl::Basic,
        ),
        (
            vec![Signed, Int],
            HlirTypeKind::Int(false),
            HlirTypeDecl::Basic,
        ),
        (
            vec![Signed, Long],
            HlirTypeKind::Long(false),
            HlirTypeDecl::Basic,
        ),
        (vec![Double], HlirTypeKind::Double, HlirTypeDecl::Basic),
        (vec![Void], HlirTypeKind::Void, HlirTypeDecl::Pointer),
    ];
    for (types, expected, decl) in type_tests {
        let mut validator = GlobalValidator::new(AbstractSyntaxTree::default());
        let dec_spec = make_dec_specifier!(types, decl == HlirTypeDecl::Pointer);
        let expected = HlirType {
            decl,
            kind: expected,
        };
        let result = validator.validate_type(&dec_spec, Span::default(), false);
        assert_eq!(result, Ok(expected));
    }
}
