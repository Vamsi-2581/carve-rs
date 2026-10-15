use carve::{parse, render_carve, render_html, Attrs, BlockNode, InlineNode};

#[test]
fn a_decorated_delimiter_is_text() {
    for (source, expected) in [
        ("*️⃣abc*", "<p>*️⃣abc*</p>"),
        ("*a*️⃣", "<p>*a*️⃣</p>"),
        ("*a*́ b*", "<p><strong>a*́ b</strong></p>"),
        ("_︎x _y_", "<p>_︎x <u>y</u></p>"),
        ("~a~‍b~", "<p><s>a~‍b</s></p>"),
        (
            "*x* *️⃣ *y*",
            "<p><strong>x</strong> *️⃣ <strong>y</strong></p>",
        ),
        ("{*️⃣abc*}", "<p>{*️⃣abc*}</p>"),
        ("/*️a*/", "<p><em>*️a*</em></p>"),
        ("/*a*/́ b*/", "<p><strong><em>a*/́ b</em></strong></p>"),
        ("/*a*/́x", "<p>/<strong>a</strong>/́x</p>"),
        ("*ाx *y*", "<p>*ाx <strong>y</strong></p>"),
        ("{^ाx^}", "<p>{^ाx^}</p>"),
        ("{^⃣x^}", "<p>{^⃣x^}</p>"),
        ("{,︎x,}", "<p>{,︎x,}</p>"),
        ("{+⃣x+} {+y+}", "<p>{+⃣x+} <ins>y</ins></p>"),
        ("{-︎x-} {-y-}", "<p>{-︎x-} <del>y</del></p>"),
        (
            "{~́a~>b~} {~c~>d~}",
            "<p>{~́a~&gt;b~} <del>c</del><ins>d</ins></p>",
        ),
        (
            "{#️⃣ x#} {#y#}",
            "<p>{#️⃣ x#} <span class=\"critic-comment\">y</span></p>",
        ),
        ("a{% y %}b {%ाx%}", "<p>ab {%ाx%}</p>"),
    ] {
        assert_eq!(render_html(&parse(source)).unwrap(), expected, "{source}");
    }
}

#[test]
fn an_insert_starting_with_an_extender_has_no_source_spelling() {
    let mut document = parse("{+x+}");
    let BlockNode::Paragraph(paragraph) = &mut document.children[0] else {
        panic!("paragraph")
    };
    let InlineNode::CriticInsert(insert) = &mut paragraph.children[0] else {
        panic!("insert")
    };
    let InlineNode::Text(text) = &mut insert.children[0] else {
        panic!("text")
    };
    text.value = "ाx".into();

    assert!(render_carve(&document).is_err());
}

#[test]
fn a_superscript_starting_with_an_extender_has_no_source_spelling() {
    let mut document = parse("{^x^}");
    let BlockNode::Paragraph(paragraph) = &mut document.children[0] else {
        panic!("paragraph")
    };
    let InlineNode::Emphasis(superscript) = &mut paragraph.children[0] else {
        panic!("superscript")
    };
    let InlineNode::Text(text) = &mut superscript.children[0] else {
        panic!("text")
    };
    text.value = "ाx".into();

    assert!(render_carve(&document).is_err());
}

#[test]
fn a_combined_token_before_an_extender_uses_a_forced_closer() {
    let mut document = parse("/*x*/ a");
    let BlockNode::Paragraph(paragraph) = &mut document.children[0] else {
        panic!("paragraph")
    };
    let InlineNode::Text(text) = &mut paragraph.children[1] else {
        panic!("text")
    };
    text.value = "́a".into();

    assert_eq!(render_carve(&document).unwrap(), "{*/x/*}́a\n");
}

#[test]
fn combined_token_attributes_separate_the_closer_from_following_text() {
    let source = "/*a*/{.k}b";
    assert_eq!(render_carve(&parse(source)).unwrap(), "/*a*/{.k}b\n");
}

#[test]
fn a_combined_token_after_an_emphasis_closer_round_trips() {
    for source in ["/a/ /*x*/", "_a_ /*x*/", "/*a*/ /*x*/"] {
        let mut document = parse(source);
        let BlockNode::Paragraph(paragraph) = &mut document.children[0] else {
            panic!("paragraph")
        };
        paragraph.children.remove(1);

        let written = render_carve(&document).unwrap();
        assert_eq!(
            render_html(&parse(&written)),
            render_html(&document),
            "{source}"
        );
    }
}

#[test]
fn empty_combined_token_attributes_do_not_separate_a_closer_from_text() {
    let mut document = parse("/*x*/ b");
    let BlockNode::Paragraph(paragraph) = &mut document.children[0] else {
        panic!("paragraph")
    };
    let InlineNode::Emphasis(combined) = &mut paragraph.children[0] else {
        panic!("combined token")
    };
    combined.attrs = Some(Attrs::default());
    let InlineNode::Text(text) = &mut paragraph.children[1] else {
        panic!("text")
    };
    text.value = "b".into();

    let written = render_carve(&document).unwrap();
    assert_ne!(written, "/*x*/b\n");
    assert_eq!(render_html(&parse(&written)), render_html(&document));
}
