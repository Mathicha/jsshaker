use oxc::{
  allocator,
  ast::ast::{Expression, JSXMemberExpression, MemberExpression},
};

use crate::{analyzer::Analyzer, ast::AstKind2, entity::Entity, transformer::Transformer};

impl<'a> Analyzer<'a> {
  pub fn exec_jsx_member_expression(&mut self, node: &'a JSXMemberExpression<'a>) -> Entity<'a> {
    let object = self.exec_jsx_member_expression_object(&node.object);
    let key = self.exec_jsx_identifier(&node.property);
    object.get_property(self, AstKind2::JSXMemberExpression(node), key)
  }
}

impl<'a> Transformer<'a> {
  pub fn transform_jsx_member_expression_effect_only(
    &self,
    node: &'a JSXMemberExpression<'a>,
    need_val: bool,
  ) -> Option<Expression<'a>> {
    let JSXMemberExpression { span, object, property, .. } = node;

    let need_access = need_val || self.is_included(AstKind2::JSXMemberExpression(node));
    if need_access {
      let object = self.transform_jsx_member_expression_object_effect_only(object, true).unwrap();
      Some(Expression::from(MemberExpression::new_static_member_expression(
        *span,
        object,
        self.transform_jsx_identifier_as_identifier_name(property),
        false,
        &self.ast,
      )))
    } else {
      self.transform_jsx_member_expression_object_effect_only(object, false)
    }
  }

  pub fn transform_jsx_member_expression_need_val(
    &self,
    node: &'a JSXMemberExpression<'a>,
  ) -> allocator::Box<'a, JSXMemberExpression<'a>> {
    let JSXMemberExpression { span, object, property, .. } = node;

    JSXMemberExpression::boxed(
      *span,
      self.transform_jsx_member_expression_object_need_val(object),
      self.transform_jsx_identifier(property),
      &self.ast,
    )
  }
}
