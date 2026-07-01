use oxc::{
  allocator::ArenaVec,
  ast::{
    NONE,
    ast::{FormalParameter, FormalParameterRest, FormalParameters},
  },
  span::GetSpan,
};

use crate::{
  analyzer::Analyzer, ast::DeclarationKind, entity::Entity, transformer::Transformer,
  value::ArgumentsValue,
};

impl<'a> Analyzer<'a> {
  pub fn exec_formal_parameters(
    &mut self,
    node: &'a FormalParameters<'a>,
    args: ArgumentsValue<'a>,
    kind: DeclarationKind,
  ) {
    for param in &node.items {
      self.declare_binding_pattern(&param.pattern, None, kind);
    }

    for (param, &init) in node.items.iter().zip(args.elements) {
      self.exec_formal_parameter(param, kind, init);
    }
    if node.items.len() > args.elements.len() {
      let value = args.rest.unwrap_or(self.factory.undefined);
      for param in &node.items[args.elements.len()..] {
        self.exec_formal_parameter(param, kind, value);
      }
    }

    // In case of `function(x=arguments, y)`, `y` should also be included
    if self.call_scope_mut().need_include_arguments {
      let arguments_included = self.include_arguments();
      assert!(arguments_included);
    }

    if let Some(rest) = &node.rest {
      let arr = self.new_empty_array();
      if args.elements.len() > node.items.len() {
        for element in &args.elements[node.items.len()..] {
          arr.push_element(*element);
        }
      }
      if let Some(rest) = args.rest {
        arr.init_rest(rest);
      }

      self.declare_binding_rest_element(&rest.rest, None, kind);
      self.init_binding_rest_element(&rest.rest, kind, arr.into());
    }
  }

  fn exec_formal_parameter(
    &mut self,
    node: &'a FormalParameter<'a>,
    kind: DeclarationKind,
    arg: Entity<'a>,
  ) {
    let binding_val = if let Some(initializer) = &node.initializer {
      self.exec_with_default(initializer, arg)
    } else {
      arg
    };
    self.init_binding_pattern(&node.pattern, kind, Some(binding_val));
  }
}

impl<'a> Transformer<'a> {
  pub fn transform_formal_parameters(
    &self,
    node: &'a FormalParameters<'a>,
  ) -> FormalParameters<'a> {
    let FormalParameters { span, items, rest, kind, .. } = node;

    let mut transformed_items = ArenaVec::new_in(&self.ast);

    let mut counting_length = self.config.preserve_function_length;
    let mut used_length = 0;

    for (index, param) in items.iter().enumerate() {
      let FormalParameter { span, decorators, pattern, initializer, .. } = param;

      let pattern_span = pattern.span();
      let pattern = self.transform_binding_pattern(pattern, false);
      let initializer_span = initializer.as_ref().map(|init| init.span());
      let initializer = if let Some(initializer) = initializer {
        self.transform_with_default(initializer, pattern.is_some())
      } else {
        None
      };

      if initializer_span.is_some() {
        counting_length = false;
      }
      if counting_length || pattern.is_some() || initializer.is_some() {
        used_length = index + 1;
      }

      transformed_items.push(FormalParameter::new(
        *span,
        self.transform_decorators(decorators),
        pattern.unwrap_or_else(|| self.build_unused_binding_pattern(pattern_span)),
        NONE,
        if counting_length
          && initializer.is_none()
          && let Some(span) = initializer_span
        {
          Some(self.build_unused_expression(span))
        } else {
          initializer
        },
        false,
        None,
        false,
        false,
        &self.ast,
      ));
    }

    let transformed_rest = match rest {
      Some(rest) => self.transform_binding_rest_element(&rest.rest, false).map(|rest| {
        FormalParameterRest::new(rest.span(), ArenaVec::new_in(&self.ast), rest, NONE, &self.ast)
      }),
      None => None,
    };

    transformed_items.truncate(used_length);

    FormalParameters::new(*span, *kind, transformed_items, transformed_rest, &self.ast)
  }

  pub fn transform_uncalled_formal_parameters(
    &self,
    node: &'a FormalParameters<'a>,
  ) -> FormalParameters<'a> {
    let FormalParameters { span, items, kind, .. } = node;

    if !self.config.preserve_function_length {
      return FormalParameters::new(*span, *kind, ArenaVec::new_in(&self.ast), NONE, &self.ast);
    }

    let mut transformed_items = ArenaVec::new_in(&self.ast);
    for param in items.iter() {
      let FormalParameter { span, decorators, initializer, .. } = param;

      if initializer.is_some() {
        break;
      }

      transformed_items.push(FormalParameter::new(
        *span,
        self.transform_decorators(decorators),
        self.build_unused_binding_pattern(*span),
        NONE,
        NONE,
        false,
        None,
        false,
        false,
        &self.ast,
      ));
    }

    FormalParameters::new(*span, *kind, transformed_items, NONE, &self.ast)
  }
}
