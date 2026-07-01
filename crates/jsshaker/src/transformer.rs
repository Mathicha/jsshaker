use std::{
  cell::{Cell, RefCell},
  hash::{DefaultHasher, Hasher},
  rc::Rc,
};

use oxc::{
  allocator::{Allocator, ArenaBox, ArenaVec, CloneIn},
  ast::{
    AstBuilder, NONE,
    ast::{
      AssignmentTarget, BinaryOperator, BindingIdentifier, BindingPattern, Declaration, Expression,
      ForStatementLeft, FormalParameter, FormalParameterKind, FormalParameters, FunctionBody,
      IdentifierReference, LogicalOperator, NumberBase, ObjectPropertyKind, Program,
      SimpleAssignmentTarget, Statement, Str, UnaryOperator, VariableDeclarationKind,
      VariableDeclarator,
    },
  },
  semantic::{ScopeId, Semantic, SymbolId},
  span::{GetSpan, SPAN, Span},
};
use rustc_hash::FxHashMap;

use crate::{
  TreeShakeConfig,
  analyzer::conditional::ConditionalDataMap,
  dep::IncludedAtoms,
  folding::ConstantFolder,
  mangling::{Mangler, ManglingStats},
  utils::ExtraData,
};

pub struct Transformer<'a> {
  pub config: &'a TreeShakeConfig,
  pub allocator: &'a Allocator,
  pub path: Str<'a>,
  pub data: &'a ExtraData<'a>,
  pub included_atoms: &'a IncludedAtoms,
  pub conditional_data: &'a ConditionalDataMap<'a>,
  pub folder: &'a ConstantFolder<'a>,
  pub mangler: Rc<RefCell<&'a mut Mangler<'a>>>,
  pub semantic: Semantic<'a>,
  pub mangling_stats: Option<Rc<RefCell<ManglingStats>>>,

  pub ast: AstBuilder<'a>,

  pub var_decls: RefCell<FxHashMap<SymbolId, bool>>,
  /// The block statement has already exited, so we can and only can transform declarations themselves
  pub declaration_only: Cell<bool>,
  pub need_unused_assignment_target: Cell<bool>,
  pub need_non_nullish_helper: Cell<bool>,
  pub unused_identifier_names: RefCell<FxHashMap<u64, usize>>,
  pub has_super_class: RefCell<Vec<bool>>,
}

impl<'a> Transformer<'a> {
  pub fn new(
    config: &'a TreeShakeConfig,
    allocator: &'a Allocator,
    path: Str<'a>,
    data: &'a ExtraData<'a>,
    included_atoms: &'a IncludedAtoms,
    conditional_data: &'a ConditionalDataMap<'a>,
    folder: &'a ConstantFolder<'a>,
    mangler: Rc<RefCell<&'a mut Mangler<'a>>>,
    semantic: Semantic<'a>,
    mangling_stats: Option<Rc<RefCell<ManglingStats>>>,
  ) -> Self {
    Transformer {
      config,
      allocator,
      path,
      data,
      included_atoms,
      conditional_data,
      folder,
      mangler,
      semantic,
      mangling_stats,

      ast: AstBuilder::new(allocator),

      var_decls: Default::default(),
      declaration_only: Cell::new(false),
      need_unused_assignment_target: Cell::new(false),
      need_non_nullish_helper: Cell::new(false),
      unused_identifier_names: Default::default(),
      has_super_class: Default::default(),
    }
  }

  pub fn transform_program(&self, node: &'a Program<'a>) -> Program<'a> {
    let Program { span, source_type, source_text, comments, hashbang, directives, body, .. } = node;

    let mut transformed_body = ArenaVec::new_in(&self.ast);

    for statement in body {
      if let Some(statement) = self.transform_statement(statement) {
        transformed_body.push(statement);
      }
    }

    self.patch_var_declarations(node.scope_id.get().unwrap(), &mut transformed_body);

    if self.need_unused_assignment_target.get() {
      transformed_body.push(self.build_unused_assignment_target_definition());
    }
    if self.need_non_nullish_helper.get() {
      transformed_body.push(self.build_non_nullish_helper_definition());
    }

    Program::new(
      *span,
      *source_type,
      source_text,
      self.clone_node(comments),
      self.clone_node(hashbang),
      self.clone_node(directives),
      transformed_body,
      &self.ast,
    )
  }

  pub fn update_var_decl_state(&self, symbol: SymbolId, is_declaration: bool) {
    if !self.semantic.scoping().symbol_flags(symbol).is_function_scoped_declaration() {
      return;
    }
    let mut var_decls = self.var_decls.borrow_mut();
    if is_declaration {
      var_decls.insert(symbol, false);
    } else {
      var_decls.entry(symbol).or_insert(true);
    }
  }

  /// Append missing var declarations at the end of the function body or program
  pub fn patch_var_declarations(
    &self,
    scope_id: ScopeId,
    statements: &mut oxc::allocator::Vec<'a, Statement<'a>>,
  ) {
    let bindings = self.semantic.scoping().get_bindings(scope_id);
    if bindings.is_empty() {
      return;
    }

    let var_decls = self.var_decls.borrow();
    let mut declarations = ArenaVec::new_in(&self.ast);
    for symbol_id in bindings.values() {
      if var_decls.get(symbol_id) == Some(&true) {
        let name = self.semantic.scoping().symbol_name(*symbol_id);
        let span = self.semantic.scoping().symbol_span(*symbol_id);
        declarations.push(VariableDeclarator::new(
          span,
          VariableDeclarationKind::Var,
          BindingPattern::new_binding_identifier(
            span,
            Str::from_str_in(name, &self.ast),
            &self.ast,
          ),
          NONE,
          None,
          false,
          &self.ast,
        ));
      }
    }

    if !declarations.is_empty() {
      statements.push(Statement::from(Declaration::new_variable_declaration(
        SPAN,
        VariableDeclarationKind::Var,
        declarations,
        false,
        &self.ast,
      )));
    }
  }
}

impl<'a> Transformer<'a> {
  pub fn clone_node<T: CloneIn<'a>>(&self, node: &T) -> T::Cloned {
    node.clone_in(self.allocator)
  }

  pub fn build_unused_binding_identifier(&self, span: Span) -> BindingIdentifier<'a> {
    let text = self.semantic.source_text().as_bytes();
    let start = 5.max(span.start as usize) - 5;
    let end = text.len().min(span.end as usize + 5);

    let mut hasher = DefaultHasher::new();
    hasher.write(&text[start..end]);
    let hash = hasher.finish() % 0xFFFF;
    let index =
      *self.unused_identifier_names.borrow_mut().entry(hash).and_modify(|e| *e += 1).or_insert(0);
    let name = if index == 0 {
      format!("__unused_{:04X}", hash)
    } else {
      format!("__unused_{:04X}_{}", hash, index - 1)
    };
    BindingIdentifier::new(span, Str::from_str_in(&name, &self.ast), &self.ast)
  }

  pub fn build_unused_binding_pattern(&self, span: Span) -> BindingPattern<'a> {
    BindingPattern::BindingIdentifier(ArenaBox::new_in(
      self.build_unused_binding_identifier(span),
      &self.ast,
    ))
  }

  pub fn build_unused_identifier_reference_write(&self, span: Span) -> IdentifierReference<'a> {
    self.need_unused_assignment_target.set(true);
    IdentifierReference::new(span, "__unused__", &self.ast)
  }

  pub fn build_unused_simple_assignment_target(&self, span: Span) -> SimpleAssignmentTarget<'a> {
    SimpleAssignmentTarget::AssignmentTargetIdentifier(ArenaBox::new_in(
      self.build_unused_identifier_reference_write(span),
      &self.ast,
    ))
  }

  pub fn build_unused_assignment_target(&self, span: Span) -> AssignmentTarget<'a> {
    // The commented doesn't work because nullish value can't be destructured
    // self.ast.assignment_target_assignment_target_pattern(
    //   self.ast.assignment_target_pattern_object_assignment_target(
    //     span,
    //     self.ast.vec(),
    //     None,
    //   ),
    // )
    AssignmentTarget::from(self.build_unused_simple_assignment_target(span))
  }

  pub fn build_unused_assignment_target_in_rest(&self, span: Span) -> AssignmentTarget<'a> {
    AssignmentTarget::from(self.build_unused_simple_assignment_target(span))
  }

  pub fn build_unused_for_statement_left(&self, span: Span) -> ForStatementLeft<'a> {
    ForStatementLeft::from(self.build_unused_assignment_target(span))
  }

  pub fn build_unused_expression(&self, span: Span) -> Expression<'a> {
    Expression::new_numeric_literal(span, 0.0, None, NumberBase::Decimal, &self.ast)
  }

  pub fn build_undefined(&self, span: Span) -> Expression<'a> {
    Expression::new_identifier(span, "undefined", &self.ast)
  }

  pub fn build_negate_expression(&self, expression: Expression<'a>) -> Expression<'a> {
    Expression::new_unary_expression(
      expression.span(),
      UnaryOperator::LogicalNot,
      expression,
      &self.ast,
    )
  }

  pub fn build_object_spread_effect(&self, span: Span, argument: Expression<'a>) -> Expression<'a> {
    Expression::new_object_expression(
      span,
      ArenaVec::from_value_in(
        ObjectPropertyKind::new_spread_property(span, argument, &self.ast),
        &self.ast,
      ),
      &self.ast,
    )
  }

  pub fn build_unused_assignment_target_definition(&self) -> Statement<'a> {
    Statement::from(Declaration::new_variable_declaration(
      SPAN,
      VariableDeclarationKind::Var,
      ArenaVec::from_value_in(
        VariableDeclarator::new(
          SPAN,
          VariableDeclarationKind::Var,
          BindingPattern::new_binding_identifier(SPAN, "__unused__", &self.ast),
          NONE,
          None,
          false,
          &self.ast,
        ),
        &self.ast,
      ),
      false,
      &self.ast,
    ))
  }

  pub fn build_non_nullish_helper_definition(&self) -> Statement<'a> {
    Statement::from(Declaration::new_variable_declaration(
      SPAN,
      VariableDeclarationKind::Var,
      ArenaVec::from_value_in(
        VariableDeclarator::new(
          SPAN,
          VariableDeclarationKind::Var,
          BindingPattern::new_binding_identifier(SPAN, "__non_nullish__", &self.ast),
          NONE,
          Some(Expression::new_arrow_function_expression(
            SPAN,
            true,
            false,
            NONE,
            FormalParameters::new(
              SPAN,
              FormalParameterKind::ArrowFormalParameters,
              ArenaVec::from_value_in(
                FormalParameter::new(
                  SPAN,
                  ArenaVec::new_in(&self.ast),
                  BindingPattern::new_binding_identifier(SPAN, "v", &self.ast),
                  NONE,
                  NONE,
                  false,
                  None,
                  false,
                  false,
                  &self.ast,
                ),
                &self.ast,
              ),
              NONE,
              &self.ast,
            ),
            NONE,
            FunctionBody::new(
              SPAN,
              ArenaVec::new_in(&self.ast),
              ArenaVec::from_value_in(
                Statement::new_expression_statement(
                  SPAN,
                  Expression::new_logical_expression(
                    SPAN,
                    Expression::new_binary_expression(
                      SPAN,
                      Expression::new_identifier(SPAN, "v", &self.ast),
                      BinaryOperator::StrictInequality,
                      Expression::new_null_literal(SPAN, &self.ast),
                      &self.ast,
                    ),
                    LogicalOperator::And,
                    Expression::new_binary_expression(
                      SPAN,
                      Expression::new_identifier(SPAN, "v", &self.ast),
                      BinaryOperator::StrictInequality,
                      Expression::new_identifier(SPAN, "undefined", &self.ast),
                      &self.ast,
                    ),
                    &self.ast,
                  ),
                  &self.ast,
                ),
                &self.ast,
              ),
              &self.ast,
            ),
            &self.ast,
          )),
          false,
          &self.ast,
        ),
        &self.ast,
      ),
      false,
      &self.ast,
    ))
  }

  pub fn build_chain_expression_mock(
    &self,
    span: Span,
    left: Expression<'a>,
    right: Expression<'a>,
  ) -> Expression<'a> {
    self.need_non_nullish_helper.set(true);
    Expression::new_logical_expression(
      span,
      Expression::new_call_expression(
        left.span(),
        Expression::new_identifier(span, "__non_nullish__", &self.ast),
        NONE,
        ArenaVec::from_value_in(left.into(), &self.ast),
        false,
        &self.ast,
      ),
      LogicalOperator::And,
      right,
      &self.ast,
    )
  }
}
