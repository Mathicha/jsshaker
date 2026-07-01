use oxc::{
  allocator::ArenaVec,
  ast::ast::{ArrayExpression, ArrayExpressionElement, Expression, SpreadElement},
  span::GetSpan,
};

use crate::{analyzer::Analyzer, ast::AstKind2, entity::Entity, transformer::Transformer};

impl<'a> Analyzer<'a> {
  pub fn exec_array_expression(&mut self, node: &'a ArrayExpression<'a>) -> Entity<'a> {
    let array = self.new_empty_array();
    let mut rest = self.factory.vec();

    for element in &node.elements {
      let mut push_element = |element: Entity<'a>| {
        if rest.is_empty() {
          array.push_element(element);
        } else {
          rest.push(element);
        }
      };
      match element {
        ArrayExpressionElement::SpreadElement(node) => {
          let (els, r, d) = self.exec_spread_element(node);
          for el in els {
            push_element(self.factory.computed(el, d));
          }
          if let Some(r) = r {
            rest.push(self.factory.computed(r, d));
          }
        }
        ArrayExpressionElement::Elision(_node) => {
          push_element(self.factory.undefined);
        }
        _ => {
          let dep = AstKind2::ArrayExpressionElement(element);
          let value = self.exec_expression(element.to_expression());
          push_element(self.factory.computed(value, dep));
        }
      }
    }

    if !rest.is_empty() {
      array.init_rest(self.factory.union(rest));
    }

    array.into()
  }
}

impl<'a> Transformer<'a> {
  pub fn transform_array_expression(
    &self,
    node: &'a ArrayExpression<'a>,
    need_val: bool,
  ) -> Option<Expression<'a>> {
    let ArrayExpression { span, elements, .. } = node;

    let mut transformed_elements = ArenaVec::new_in(&self.ast);

    for element in elements {
      let span = element.span();
      match element {
        ArrayExpressionElement::SpreadElement(node) => {
          if let Some(element) = self.transform_array_spread_element(node) {
            transformed_elements.push(element);
          }
        }
        ArrayExpressionElement::Elision(_) => {
          if need_val {
            transformed_elements.push(ArrayExpressionElement::new_elision(span, &self.ast));
          }
        }
        _ => {
          let included = self.is_included(AstKind2::ArrayExpressionElement(element));
          let element = self.transform_expression(element.to_expression(), need_val && included);
          if let Some(inner) = element {
            transformed_elements.push(inner.into());
          } else if need_val {
            transformed_elements.push(ArrayExpressionElement::new_elision(span, &self.ast));
          }
        }
      }
    }

    if !need_val {
      if transformed_elements.is_empty() {
        return None;
      }
      if transformed_elements.len() == 1 {
        return Some(match transformed_elements.pop().unwrap() {
          ArrayExpressionElement::SpreadElement(inner) => {
            if self.config.iterate_side_effects {
              Expression::new_array_expression(
                *span,
                ArenaVec::from_value_in(ArrayExpressionElement::SpreadElement(inner), &self.ast),
                &self.ast,
              )
            } else {
              let SpreadElement { argument, .. } = inner.unbox();
              argument
            }
          }
          node => node.try_into().unwrap(),
        });
      }
    }

    Some(Expression::new_array_expression(*span, transformed_elements, &self.ast))
  }
}
