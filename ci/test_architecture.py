"""Small adversarial fixtures for the lexical survey; no third-party dependencies."""

import unittest

from architecture import mask, named_fields, source_metrics


class MetricsTests(unittest.TestCase):
    def test_literals_comments_lifetimes_and_nested_blocks(self):
        source = '''
// fn misleading() { if true {} }
/* nested /* fn bad() {} */ comment */
fn actual<'a>(s: &'a str) -> &'a str {
    let a = r##" } fn fake() { // "# still string "##;
    let b = "escaped \\" } fn fake() {}";
    let c = b'{';
    let d = '\\u{1f600}';
    if s.is_empty() { return "}"; }
    s
}
'''
        masked = mask(source)
        self.assertEqual(len(masked), len(source))
        self.assertEqual(masked.count("\n"), source.count("\n"))
        _, functions, _ = source_metrics("fixture.rs", source)
        self.assertEqual([f["name"] for f in functions], ["actual"])
        self.assertEqual(functions[0]["branch_tokens"], 1)
        self.assertEqual(functions[0]["lines"], 8)

    def test_trait_methods_and_named_fields(self):
        source = '''
trait T { fn declaration(&self); fn implemented(&self) { if true {} } }
pub struct State<'a> {
    #[allow(dead_code)]
    field: Vec<(String, usize)>,
    pub(crate) callback: fn() -> bool,
    reference: &'a str,
}
'''
        _, functions, structs = source_metrics("fixture.rs", source)
        self.assertEqual([f["name"] for f in functions], ["implemented"])
        self.assertEqual(structs[0]["fields"], ["field", "callback", "reference"])

    def test_nested_field_types_are_not_struct_fields(self):
        self.assertEqual(named_fields('''
    value: Something<{
        nested: false
    }>,
    after: bool,
'''), ["value", "after"])


if __name__ == "__main__":
    unittest.main()
