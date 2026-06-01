#[test]
fn macro_param_expansion() {
    let t = trybuild::TestCases::new();
    t.pass("tests/ui/pass/service_routes.rs");
    t.pass("tests/ui/pass/bare_special_generics.rs");
    t.pass("tests/ui/pass/hook_map_scope_params.rs");
    t.pass("tests/ui/pass/map_raw_body.rs");
    t.pass("tests/ui/pass/map_raw_output.rs");
    t.pass("tests/ui/pass/map_borrowed_output.rs");
    t.pass("tests/ui/pass/map_validate_only.rs");
    t.pass("tests/ui/pass/map_multipart_raw.rs");
    t.compile_fail("tests/ui/fail/hook_not_async.rs");
    t.compile_fail("tests/ui/fail/ctx_not_mut.rs");
    t.compile_fail("tests/ui/fail/scope_with_ref_param.rs");
    t.compile_fail("tests/ui/fail/map_without_owned_input.rs");
    t.compile_fail("tests/ui/fail/raw_body_by_ref.rs");
    t.compile_fail("tests/ui/fail/raw_body_in_hook.rs");
    t.compile_fail("tests/ui/fail/map_borrows_local.rs");
    t.compile_fail("tests/ui/fail/map_raw_borrowed_slice.rs");
    t.compile_fail("tests/ui/fail/map_output_not_serialize.rs");
    t.compile_fail("tests/ui/fail/route_outside_service.rs");
    t.compile_fail("tests/ui/fail/route_unknown_arg.rs");
}
