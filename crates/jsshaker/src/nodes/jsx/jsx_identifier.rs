use oxc::ast::ast::{IdentifierName, JSXIdentifier};

use crate::{analyzer::Analyzer, ast::AstKind2, entity::Entity, transformer::Transformer};

impl<'a> Analyzer<'a> {
  pub fn exec_jsx_identifier(&mut self, node: &'a JSXIdentifier<'a>) -> Entity<'a> {
    self.exec_mangable_static_string(AstKind2::JSXIdentifier(node), &node.name)
  }
}

impl<'a> Transformer<'a> {
  pub fn transform_jsx_identifier(&self, node: &'a JSXIdentifier<'a>) -> JSXIdentifier<'a> {
    let JSXIdentifier { span, name, .. } = node;
    JSXIdentifier::new(
      *span,
      self.transform_mangable_static_string(AstKind2::JSXIdentifier(node), name),
      &self.ast,
    )
  }

  pub fn transform_jsx_identifier_as_identifier_name(
    &self,
    node: &'a JSXIdentifier<'a>,
  ) -> IdentifierName<'a> {
    let JSXIdentifier { span, name, .. } = node;
    IdentifierName::new(
      *span,
      self.transform_mangable_static_string(AstKind2::JSXIdentifier(node), name),
      &self.ast,
    )
  }
}
