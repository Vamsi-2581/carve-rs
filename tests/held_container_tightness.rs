#[test]
fn a_held_container_keeps_its_own_blank_lines() {
    for closed in [false, true] {
        let mut source = "- ::: box\n  first\n\n  second\n".to_string();
        if closed {
            source.push_str("  :::\n");
        }
        let ast = carve::parse(&source);
        let carve::BlockNode::List(list) = &ast.children[0] else {
            panic!("expected list")
        };
        assert!(list.tight, "{source}");
    }
}
