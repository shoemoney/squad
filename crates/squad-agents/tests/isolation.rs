#[test]
fn executors_cannot_import_storage_or_bus() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/compile_fail/*.rs");
}
