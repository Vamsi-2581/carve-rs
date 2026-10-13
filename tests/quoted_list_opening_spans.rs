use carve::ast::{BlockNode, Pos};
fn verify(nodes: &[BlockNode], source: &[char], count: &mut usize) {
    fn marker(pos: &Pos, source: &[char], count: &mut usize) {
        assert!(matches!(source[pos.start_offset], '-' | '1'), "{pos:?}");
        *count += 1;
    }
    for node in nodes {
        match node {
            BlockNode::List(list) => {
                marker(list.pos.as_ref().unwrap(), source, count);
                for item in &list.items {
                    marker(item.pos.as_ref().unwrap(), source, count);
                    verify(&item.children, source, count);
                }
            }
            BlockNode::BlockQuote(quote) => verify(&quote.children, source, count),
            _ => {}
        }
    }
}
#[test]
fn quoted_list_spans_begin_at_markers() {
    for depth in 1..=3 {
        for indent in [2, 3, 4, 6] {
            let prefix = "> ".repeat(depth);
            let source = format!(
                "{prefix}{}- a\n{prefix}- b\n{prefix}    - c\n{prefix}      :::\n1. lazy\n",
                " ".repeat(indent)
            );
            let options = carve::Options::default().with_positions(true);
            let doc = carve::parse_with_options(&source, &options);
            let mut count = 0;
            verify(
                &doc.children,
                &source.chars().collect::<Vec<_>>(),
                &mut count,
            );
            assert!(count >= 6, "{source}: {count}");
        }
    }
}
