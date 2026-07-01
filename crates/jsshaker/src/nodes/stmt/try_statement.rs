use oxc::{
  allocator::ArenaVec,
  ast::ast::{BlockStatement, CatchClause, Statement, TryStatement},
};

use crate::{analyzer::Analyzer, transformer::Transformer};

impl<'a> Analyzer<'a> {
  pub fn exec_try_statement(&mut self, node: &'a TryStatement<'a>) {
    self.push_non_det_cf_scope();
    if self.scoping.try_catch_depth.is_none() {
      self.scoping.try_catch_depth = Some(self.scoping.cf.current_depth());
      self.exec_block_statement(&node.block);
      self.scoping.try_catch_depth = None;
    } else {
      self.exec_block_statement(&node.block);
    }
    self.pop_cf_scope();

    if let Some(handler) = &node.handler {
      self.exec_catch_clause(handler, self.factory.unknown);
    }

    if let Some(finalizer) = &node.finalizer {
      self.exec_block_statement(finalizer);
    }
  }
}

impl<'a> Transformer<'a> {
  pub fn transform_try_statement(&self, node: &'a TryStatement<'a>) -> Option<Statement<'a>> {
    let TryStatement { span, block, handler, finalizer, .. } = node;

    let block = self.transform_block_statement(block);

    let handler_span = handler.as_ref().map(|handler| handler.span).unwrap_or_default();
    let handler = if block.is_some() {
      handler.as_ref().map(|handler| self.transform_catch_clause(handler))
    } else {
      None
    };

    let finalizer =
      finalizer.as_ref().and_then(|finalizer| self.transform_block_statement(finalizer));

    match (block, finalizer) {
      (None, None) => None,
      (None, Some(finalizer)) => Some(Statement::BlockStatement(finalizer)),
      (Some(block), finalizer) => Some(Statement::new_try_statement(
        *span,
        block,
        if finalizer.is_some() {
          handler
        } else {
          Some(handler.unwrap_or_else(|| {
            CatchClause::new(
              handler_span,
              None,
              BlockStatement::new(handler_span, ArenaVec::new_in(&self.ast), &self.ast),
              &self.ast,
            )
          }))
        },
        finalizer,
        &self.ast,
      )),
    }
  }
}
