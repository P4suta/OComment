open Ocomment_ref

let check_transform () =
  let source = Bytes.of_string "let x = 1; (* remove\r\nthis *)\r\n" in
  let result = transform source Ocaml default_transform_options in
  Alcotest.(check string) "CRLF remains" "let x = 1; \r\n\r\n" (Bytes.to_string result.output);
  Alcotest.(check int) "one edit" 1 (List.length result.edits)

let check_nested () =
  let report = scan (Bytes.of_string "/* a /* b */ c */") Rust default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "one comment" 1 (List.length report.comments)

let check_string () =
  let report = scan (Bytes.of_string "\"// no\" // yes") JavaScript default_scan_options in
  Alcotest.(check int) "only real comment" 1 (List.length report.comments)

let check_rust_multiline_string () =
  let report =
    scan (Bytes.of_string "let s = \"a\n// no\nb\"; // yes") Rust default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "only real comment" 1 (List.length report.comments)

let () = Alcotest.run "ocomment-ref" [
  "core", [
    Alcotest.test_case "transform" `Quick check_transform;
    Alcotest.test_case "nested" `Quick check_nested;
    Alcotest.test_case "string" `Quick check_string;
    Alcotest.test_case "rust-multiline-string" `Quick check_rust_multiline_string;
  ]
]
