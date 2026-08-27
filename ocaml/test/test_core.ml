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

(* NOTE: The diagnostic message is the one thing an `expect` block in
   spec/fixtures does not record -- it pins the code and the span -- so the
   wording is held here and by the byte-for-byte comparison in
   tools/differential.py.  It is what a user reads, so it names the construct
   that was left open rather than saying "literal". *)
let check_unterminated_messages () =
  let cases = [
    Rust, "let s = \"unclosed // not a comment\n", "unterminated string";
    Go, "s := \"unclosed // not a comment\n", "unterminated string or rune literal";
    Go, "r := 'x // not a comment\n", "unterminated string or rune literal";
    Css, "a::before { content: \"unclosed\n", "unterminated CSS string";
    Css, "a::before { content: 'unclosed\n", "unterminated CSS string";
    Jsonc, "{ \"key: 1 }\n", "unterminated JSON string";
    Jsonc, "{ 'key: 1 }\n", "unterminated JSON string";
    Kotlin, "val c = 'x // not a comment\n", "unterminated Kotlin character literal";
    C, "const char *s = \"unclosed\n", "unterminated string or character literal";
    Rust, "let s = r\"unclosed\n", "unterminated Rust raw string";
    Yaml, "key: \"unclosed # not a comment\n", "unterminated YAML double-quoted scalar";
    Yaml, "key: 'unclosed # not a comment\n", "unterminated YAML single-quoted scalar";
    Php, "<?php $s = 'unclosed // not a comment\n", "unterminated PHP single-quoted string";
    Php, "<?php $s = \"unclosed // not a comment\n", "unterminated PHP double-quoted string";
    Php, "<?php $s = `unclosed // not a comment\n", "unterminated PHP backtick string";
    Php, "<?php $s = <<<EOT\n// not a comment\n", "unterminated PHP heredoc";
    Php, "<?php $s = <<<'NOW'\n# not a comment\n", "unterminated PHP nowdoc";
  ] in
  List.iter (fun (language, source, message) ->
    let report = scan (Bytes.of_string source) language default_scan_options in
    Alcotest.(check bool) ("invalid: " ^ source) false report.valid;
    Alcotest.(check (list string)) ("message: " ^ source) [message]
      (List.map (fun diagnostic -> diagnostic.message) report.diagnostics);
    Alcotest.(check (list string)) ("code: " ^ source) ["unterminated-string"]
      (List.map (fun diagnostic -> diagnostic.code) report.diagnostics)) cases

(* NOTE: Rust Reference, Lifetimes and loop labels: an apostrophe that no second
   apostrophe closes is a lifetime and opens no literal, so the line comment
   behind one is a comment. *)
let check_rust_lifetime () =
  let source = "let r: &'a str = s; // remove\n" in
  let report = scan (Bytes.of_string source) Rust default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "no diagnostics" 0 (List.length report.diagnostics);
  Alcotest.(check int) "one comment" 1 (List.length report.comments);
  Alcotest.(check int) "at the slashes" 20 (List.hd report.comments).span.start

(* NOTE: Rust Reference, raw string literals: one that never closes runs to the
   end of the file, and the diagnostic spans what the lexer consumed. *)
let check_rust_unterminated_raw_string () =
  List.iter (fun source ->
    let report = scan (Bytes.of_string source) Rust default_scan_options in
    Alcotest.(check bool) ("invalid: " ^ source) false report.valid;
    Alcotest.(check int) ("no comments: " ^ source) 0 (List.length report.comments);
    let diagnostic = List.hd report.diagnostics in
    Alcotest.(check int) ("from the r: " ^ source) 8 diagnostic.span.start;
    Alcotest.(check int) ("to the end: " ^ source) (String.length source) diagnostic.span.finish)
    ["let s = r\"unclosed\n"; "let s = r#\"unclosed // not a comment\n"]

(* NOTE: JSON5 4.4 writes a string with either quote, and this language owns
   ".json5" as well as ".jsonc", so a "//" inside an apostrophe is content. *)
let check_jsonc_single_quoted_string () =
  let source = "{ 'note': '// not a comment', \"other\": 1 } // remove\n" in
  let report = scan (Bytes.of_string source) Jsonc default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "one comment" 1 (List.length report.comments);
  Alcotest.(check int) "the trailing one" 43 (List.hd report.comments).span.start

(* NOTE: A byte order mark is consumed before the first line is read -- CPython's
   check_bom, Lua's skipBOM -- so the "#!" line behind one is still a preamble.
   A shell is the exception: "#" opens a comment only where no word has begun,
   and the mark's bytes begin one. *)
let check_byte_order_mark () =
  let python = "\xef\xbb\xbf#!/usr/bin/env python3\nx = 1\n" in
  let report = scan (Bytes.of_string python) Python default_scan_options in
  Alcotest.(check int) "python: one comment" 1 (List.length report.comments);
  Alcotest.(check bool) "python: a shebang" true ((List.hd report.comments).kind = Shebang);
  Alcotest.(check string) "python: kept whole" python
    (Bytes.to_string (transform (Bytes.of_string python) Python default_transform_options).output);
  let lua = "\xef\xbb\xbf#!/usr/bin/lua\nx = 1\n" in
  let report = scan (Bytes.of_string lua) Lua default_scan_options in
  Alcotest.(check bool) "lua: a shebang" true ((List.hd report.comments).kind = Shebang);
  Alcotest.(check int) "lua: behind the mark" 3 (List.hd report.comments).span.start;
  let lua_comment = "\xef\xbb\xbf# not a shebang\nx = 1\n" in
  let report = scan (Bytes.of_string lua_comment) Lua default_scan_options in
  Alcotest.(check bool) "lua: an ordinary comment" true ((List.hd report.comments).kind = Line);
  let shell = "\xef\xbb\xbf#!/bin/sh\necho 1\n" in
  let report = scan (Bytes.of_string shell) Shell default_scan_options in
  Alcotest.(check int) "shell: the mark opens a word" 0 (List.length report.comments)

(* NOTE: A directive named after the tool that reads it ends at a boundary, and
   the end of the comment is one; letters running on past it are still prose. *)
let check_keyword_directive_without_argument () =
  let kept = [Toml, "#:schema\nkey = 1\n"; Shell, "# shellcheck\ncat x\n";
    Shell, "# hadolint\nRUN true\n"] in
  List.iter (fun (language, source) ->
    let report = scan (Bytes.of_string source) language default_scan_options in
    Alcotest.(check bool) ("directive: " ^ source) true
      ((List.hd report.comments).kind = Directive);
    Alcotest.(check bool) ("kept: " ^ source) true
      ((List.hd report.comments).disposition <> Remove)) kept;
  let removed = [Toml, "#:schemata are plural\nkey = 1\n";
    Shell, "# shellcheckish note\ncat x\n"] in
  List.iter (fun (language, source) ->
    let report = scan (Bytes.of_string source) language default_scan_options in
    Alcotest.(check bool) ("removed: " ^ source) true
      ((List.hd report.comments).disposition = Remove)) removed

(* NOTE: The classifier trims Unicode whitespace, so a directive word behind a
   no-break space or a line separator is still the directive.  The layout
   arithmetic trims ASCII whitespace, which leaves the vertical tab out, so a
   line carrying one is not blank. *)
let check_unicode_and_vertical_tab () =
  let report = scan (Bytes.of_string "//\xe2\x80\xa8region\n") Rust default_scan_options in
  Alcotest.(check bool) "a directive behind U+2028" true
    ((List.hd report.comments).kind = Directive);
  let report = scan (Bytes.of_string "//\xc2\xa0region\n") Go default_scan_options in
  Alcotest.(check bool) "a directive behind U+00A0" true
    ((List.hd report.comments).kind = Directive);
  let result = transform (Bytes.of_string "\x0b// note\nx = 1\n") Rust
    { default_transform_options with layout = Compact } in
  Alcotest.(check string) "the vertical tab is not blank" "\x0b\nx = 1\n"
    (Bytes.to_string result.output)

(* NOTE: YAML 1.2.2, 8.1: the body of a block scalar is every following line
   more indented than the node it hangs off, so a "#" inside it is content of
   the scalar and only the one on the line that ends the body is a comment. *)
let check_yaml_block_scalar () =
  let source = "script: |\n  # not a comment\n  echo hi\ndone: 1 # remove\n" in
  let report = scan (Bytes.of_string source) Yaml default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "one comment" 1 (List.length report.comments);
  Alcotest.(check int) "the trailing one" 46 (List.hd report.comments).span.start

(* NOTE: YAML 1.2.2, 8.1.1.1 with 6.9: the body of a block scalar is measured
   from the node it hangs off, never from the column its own "|" sits in.  A tag
   or an anchor may stand between the two, and the header may sit on the line
   below the key that owns it, so a body shallower than the header is still
   content and none of the "#" in it is a comment. *)
let check_yaml_block_scalar_owner () =
  List.iter (fun source ->
    let report = scan (Bytes.of_string source) Yaml default_scan_options in
    Alcotest.(check bool) ("valid: " ^ String.escaped source) true report.valid;
    Alcotest.(check int) ("no comment: " ^ String.escaped source) 0
      (List.length report.comments))
    ["key: !!str |\n  # a\n"; "key: &x |\n  # a\n"; "key:\n    |\n  # a\n";
     "!!str |\n # a\n"; "- - |\n    # a\n"; "? |\n  # a\n: v\n"]

(* NOTE: YAML 1.2.2, 8.1.1 and 8.1.1.2: a whole-line comment under a block
   scalar body is "l-trail-comments" and is not part of the value, but every
   hole a removal could leave on that line is.  A line of spaces as wide as the
   comment -- what `columns` writes -- is indented into the body it was
   terminating; an empty line -- what `lines` writes -- is content under "|+"
   and ">+".  So the removal takes the whole line, terminator included, under
   every layout and under every chomping indicator. *)
let check_yaml_keep_chomped_trail () =
  let source = Bytes.of_string "k: |+\n  body\n\n# after\nnext: 1 # yes\n" in
  List.iter (fun (layout, expected) ->
    let result = transform source Yaml { default_transform_options with layout } in
    Alcotest.(check string) expected expected (Bytes.to_string result.output))
    [Lines, "k: |+\n  body\n\nnext: 1 \n";
     Columns, "k: |+\n  body\n\nnext: 1      \n";
     Compact, "k: |+\n  body\n\nnext: 1\n"];
  List.iter (fun header ->
    List.iter (fun layout ->
      let document = Bytes.of_string ("k: " ^ header ^ "\n  body\n\n# after\nnext: 1\n") in
      let result = transform document Yaml { default_transform_options with layout } in
      Alcotest.(check string) header ("k: " ^ header ^ "\n  body\n\nnext: 1\n")
        (Bytes.to_string result.output))
      [Lines; Columns; Compact])
    ["|"; "|-"; "|+"; ">"; ">-"; ">+"]

(* NOTE: The half of the rule that is about the lines the comment was
   sheltering.  Under "|+" an empty line trailing a body is content
   (YAML 1.2.2, 8.1.1.2) -- but only until "l-trail-comments" begins, after
   which every empty line is "l-comment" and belongs to nobody.  Removing a
   trail comment hands those lines back to the "+", so the removal takes them
   with it; the empty lines above the first comment were already content and are
   left exactly where they were. *)
let check_yaml_keep_chomped_sheltered_run () =
  List.iter (fun (source, expected) ->
    List.iter (fun layout ->
      let result = transform (Bytes.of_string source) Yaml
        { default_transform_options with layout } in
      Alcotest.(check string) (String.escaped source) expected
        (Bytes.to_string result.output))
      [Lines; Columns; Compact])
    ["k: |+\n  a\n# c\n\nz: 1\n", "k: |+\n  a\nz: 1\n";
     "k: |+\n  a\n# c\n\n\nz: 1\n", "k: |+\n  a\nz: 1\n";
     "k: |+\n  a\n\n# c\n\nz: 1\n", "k: |+\n  a\n\nz: 1\n";
     "k: |+\n  a\n\n# c\nz: 1\n", "k: |+\n  a\n\nz: 1\n";
     "k: |+\n  a\n# c\n\n# yamllint disable\n\nz: 1\n",
       "k: |+\n  a\n# yamllint disable\n\nz: 1\n";
     "k: |\n  a\n# c\n\nz: 1\n", "k: |\n  a\n\nz: 1\n"]

(* NOTE: A block scalar body ends at the first line shallower than its content
   (YAML 1.2.2, 8.1.1), and taking that line away hands everything under it back
   to the body.  When what comes back up is a comment the run keeps, no removal
   preserves the value, so the comment that ends the body is kept and the reason
   says why.  The content indentation is what the depth is read against, not the
   floor a body line has to clear: the third and fourth shapes below are the same
   trail under a body written deeper, where the directive is a comment on both
   sides of the removal. *)
let check_yaml_structural_trail () =
  let report = scan (Bytes.of_string "k: |\n  a\n# shallow\n  # yamllint disable\nz: 1\n")
    Yaml default_scan_options in
  Alcotest.(check int) "two comments" 2 (List.length report.comments);
  Alcotest.(check bool) "the shallow one is structural" true
    ((List.hd report.comments).disposition = Keep "structural in a YAML block scalar trail");
  List.iter (fun (source, expected) ->
    List.iter (fun layout ->
      let result = transform (Bytes.of_string source) Yaml
        { default_transform_options with layout } in
      Alcotest.(check string) (String.escaped source) expected
        (Bytes.to_string result.output))
      [Lines; Columns; Compact])
    ["k: |\n  a\n# shallow\n  # yamllint disable\nz: 1\n",
       "k: |\n  a\n# shallow\n  # yamllint disable\nz: 1\n";
     "k: |+\n  a\n# shallow\n\n  # yamllint disable\nz: 1\n",
       "k: |+\n  a\n# shallow\n\n  # yamllint disable\nz: 1\n";
     "k: |\n    a\n# shallow\n  # yamllint disable\nz: 1\n",
       "k: |\n    a\n  # yamllint disable\nz: 1\n";
     "- |\n   a\n# shallow\n  # yamllint disable\n",
       "- |\n   a\n  # yamllint disable\n";
     "k: |\n  a\n# shallow\n  # deep\nz: 1\n", "k: |\n  a\nz: 1\n"]

(* NOTE: "k: a |+" ends a plain scalar with two characters that look like a
   header.  Hanging a keep-chomped trail off it would take the line of a comment
   that shelters nothing, so only a header the scan itself recognised opens
   one. *)
let check_yaml_phantom_header () =
  let result = transform (Bytes.of_string "k: a |+\n# c\n\nz: 1\n") Yaml
    default_transform_options in
  Alcotest.(check int) "one comment" 1 (List.length result.report.comments);
  Alcotest.(check string) "the line stays" "k: a |+\n\n\nz: 1\n"
    (Bytes.to_string result.output)

(* NOTE: PHP is two languages in one file: the inline HTML around the tags is
   opaque, a "//" or "#" comment ends at the closing tag as well as at the line
   break, and "#[" opens an attribute rather than a comment (PHP 8.0). *)
let check_php_modes () =
  let source = "<p># not a comment</p>\n<?php #[A] // remove ?>\n<p>/* nor this */</p>\n" in
  let report = scan (Bytes.of_string source) Php default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "one comment" 1 (List.length report.comments);
  Alcotest.(check int) "at the //" 34 (List.hd report.comments).span.start;
  Alcotest.(check int) "ends at the ?>" 44 (List.hd report.comments).span.finish


(* NOTE: Scala's XML literal is the one construct where the compiler's lexer and
   parser disagree: the lexer reports a "//" in the element text as a comment
   and the parser reads it as text.  The scanner follows the parser, and the
   comment inside the interpolation is code. *)
let check_scala_xml_and_interpolation () =
  let source =
    "val a = <a>// text</a>\nval b = s\"${1 /* keep */}\" // remove\n" in
  let report = scan (Bytes.of_string source) Scala default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "two comments" 2 (List.length report.comments)

(* NOTE: Vue's template is HTML with code in its mustaches, and its script
   and style bodies are scanned as their own languages; the v-pre directive
   makes an element's content raw text. *)
let check_vue_component () =
  let source =
    "<div v-pre>{{ x // not }}</div>\n<template>\n<!-- note -->\n</template>\n" in
  let report = scan (Bytes.of_string source) Vue default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "only the html comment" 1 (List.length report.comments)

(* NOTE: Markdown's fenced code blocks are scanned as the language their
   info string names, and its inline code is opaque. *)
let check_markdown_fences () =
  let source =
    "```rust\n// c\n```\n`// inline`\n" in
  let report = scan (Bytes.of_string source) Markdown default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "only the rust comment" 1 (List.length report.comments)

let () = Alcotest.run "ocomment-ref" [
  "core", [
    Alcotest.test_case "transform" `Quick check_transform;
    Alcotest.test_case "nested" `Quick check_nested;
    Alcotest.test_case "string" `Quick check_string;
    Alcotest.test_case "rust-multiline-string" `Quick check_rust_multiline_string;
    Alcotest.test_case "unterminated-messages" `Quick check_unterminated_messages;
    Alcotest.test_case "rust-lifetime" `Quick check_rust_lifetime;
    Alcotest.test_case "rust-unterminated-raw-string" `Quick check_rust_unterminated_raw_string;
    Alcotest.test_case "jsonc-single-quoted-string" `Quick check_jsonc_single_quoted_string;
    Alcotest.test_case "byte-order-mark" `Quick check_byte_order_mark;
    Alcotest.test_case "keyword-directive" `Quick check_keyword_directive_without_argument;
    Alcotest.test_case "unicode-and-vertical-tab" `Quick check_unicode_and_vertical_tab;
    Alcotest.test_case "yaml-block-scalar" `Quick check_yaml_block_scalar;
    Alcotest.test_case "yaml-block-scalar-owner" `Quick check_yaml_block_scalar_owner;
    Alcotest.test_case "yaml-keep-chomped-trail" `Quick check_yaml_keep_chomped_trail;
    Alcotest.test_case "yaml-keep-chomped-sheltered-run" `Quick
      check_yaml_keep_chomped_sheltered_run;
    Alcotest.test_case "yaml-structural-trail" `Quick check_yaml_structural_trail;
    Alcotest.test_case "yaml-phantom-header" `Quick check_yaml_phantom_header;
    Alcotest.test_case "php-modes" `Quick check_php_modes;
    Alcotest.test_case "scala-xml-and-interpolation" `Quick
      check_scala_xml_and_interpolation;
    Alcotest.test_case "vue-component" `Quick check_vue_component;
    Alcotest.test_case "markdown-fences" `Quick check_markdown_fences;
  ]
]
