use crate::parser::event::Event;
use crate::syntax_node::{SyntaxElement, SyntaxNode, SyntaxToken};
use crate::{SyntaxKind, SyntaxNodeKind};
use php_lexer::Token;
use php_source::TextRange;

#[derive(Debug)]
struct NodeFrame {
    kind: SyntaxKind,
    children: Vec<SyntaxElement>,
}

/// Tree construction boundary for event-based parsing.
#[derive(Clone, Debug)]
pub struct TreeSink<'src> {
    source: &'src str,
    tokens: Vec<Token>,
    token_cursor: usize,
}

impl<'src> TreeSink<'src> {
    /// Creates a tree sink.
    #[must_use]
    pub fn new(source: &'src str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            token_cursor: 0,
        }
    }

    /// Builds a syntax tree from parser events.
    #[must_use]
    pub fn finish(mut self, events: Vec<Event>) -> SyntaxNode {
        let mut stack: Vec<NodeFrame> = Vec::new();
        let mut root: Option<SyntaxNode> = None;

        for event in events {
            match event {
                Event::Placeholder => {}
                Event::StartNode(kind) => stack.push(NodeFrame {
                    kind,
                    children: Vec::new(),
                }),
                Event::AddToken => {
                    if let Some(token) = self.next_token() {
                        attach_element(&mut stack, &mut root, SyntaxElement::Token(token));
                    }
                }
                Event::Error(diagnostic) => {
                    let node = SyntaxNode::new(
                        SyntaxKind::Node(SyntaxNodeKind::Error),
                        diagnostic.span,
                        Vec::new(),
                    );
                    attach_element(&mut stack, &mut root, SyntaxElement::Node(node));
                }
                Event::FinishNode => {
                    if let Some(frame) = stack.pop() {
                        let range = range_for_children(&frame.children);
                        let node = SyntaxNode::new(frame.kind, range, frame.children);
                        attach_element(&mut stack, &mut root, SyntaxElement::Node(node));
                    }
                }
            }
        }

        root.unwrap_or_else(|| {
            SyntaxNode::new(
                SyntaxKind::SOURCE_FILE,
                TextRange::new(0, self.source.len()),
                Vec::new(),
            )
        })
    }

    fn next_token(&mut self) -> Option<SyntaxToken> {
        let token = self.tokens.get(self.token_cursor)?;
        self.token_cursor += 1;
        let text = token.text(self.source).unwrap_or_default();
        Some(SyntaxToken::new(
            SyntaxKind::from_token_kind(token.kind),
            text,
            token.range,
            token.line,
        ))
    }
}

fn attach_element(stack: &mut [NodeFrame], root: &mut Option<SyntaxNode>, element: SyntaxElement) {
    if let Some(frame) = stack.last_mut() {
        frame.children.push(element);
    } else if let SyntaxElement::Node(node) = element {
        *root = Some(node);
    }
}

fn range_for_children(children: &[SyntaxElement]) -> TextRange {
    let start = children.first().map(element_start).unwrap_or(0);
    let end = children.iter().map(element_end).max().unwrap_or(start);
    TextRange::new(start, end)
}

fn element_start(element: &SyntaxElement) -> usize {
    match element {
        SyntaxElement::Node(node) => node.range().start().to_usize(),
        SyntaxElement::Token(token) => token.range().start().to_usize(),
    }
}

fn element_end(element: &SyntaxElement) -> usize {
    match element {
        SyntaxElement::Node(node) => node.range().end().to_usize(),
        SyntaxElement::Token(token) => token.range().end().to_usize(),
    }
}

#[cfg(test)]
mod tests {
    use super::TreeSink;
    use crate::SyntaxKind;
    use crate::parser::event::Event;
    use php_lexer::{LexerConfig, lex_all};

    #[test]
    fn sink_builds_source_file_from_events() {
        let source = "<?php echo 1;";
        let lexed = lex_all(source, LexerConfig::default());
        let sink = TreeSink::new(source, lexed.tokens);
        let root = sink.finish(vec![
            Event::StartNode(SyntaxKind::SOURCE_FILE),
            Event::AddToken,
            Event::FinishNode,
        ]);

        assert_eq!(*root.kind(), SyntaxKind::SOURCE_FILE);
        assert_eq!(root.children().len(), 1);
    }

    #[test]
    fn node_range_uses_max_child_end() {
        let source = "<?php";
        let lexed = lex_all(source, LexerConfig::default());
        let sink = TreeSink::new(source, lexed.tokens);
        let root = sink.finish(vec![
            Event::StartNode(SyntaxKind::SOURCE_FILE),
            Event::AddToken,
            Event::StartNode(SyntaxKind::ERROR),
            Event::FinishNode,
            Event::FinishNode,
        ]);

        assert_eq!(root.range().start().to_usize(), 0);
        assert_eq!(root.range().end().to_usize(), source.len());
    }
}
