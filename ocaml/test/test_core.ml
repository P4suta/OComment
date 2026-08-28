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
    (Bytes.to_string result.output);
  let report = scan (Bytes.of_string "// swift-format-ignore\x0b#error ") Swift
    default_scan_options in
  Alcotest.(check bool) "vertical tab ends a swift-format marker" true
    ((List.hd report.comments).kind = Directive)

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

(* NOTE: Perl's POD blocks are opaque, and its division comments are
   comments. *)
let check_perl_pod () =
  let source = "=head1 NAME\n# not a comment\n=cut\nmy $x = 1; # comment\n" in
  let report = scan (Bytes.of_string source) Perl default_scan_options in
  Alcotest.(check bool) "valid" true report.valid;
  Alcotest.(check int) "only the real comment" 1 (List.length report.comments)

let raw_comments source (report : scan_report) =
  List.map (fun (comment : comment) ->
    Bytes.sub_string source comment.span.start (comment.span.finish - comment.span.start))
    report.comments

let check_sass_and_scss_compounds () =
  let scss = Bytes.of_string
    ".a { x: \"#{1 /* string */}\"; y: url( \"#{2 /* url */}\" ); z: url(foo\\)bar//opaque); // outer\n }" in
  let report = scan scss Css { default_scan_options with dialect = Scss } in
  Alcotest.(check bool) "scss valid" true report.valid;
  Alcotest.(check (list string)) "scss comments"
    ["/* string */"; "/* url */"; "// outer"] (raw_comments scss report);
  let sass = Bytes.of_string
    ".a\n  // parent\n    color: red\n      width: 1px\n  color: blue\n// root\n  nested: yes\n.b\n" in
  let report = scan sass Css { default_scan_options with dialect = Sass } in
  Alcotest.(check bool) "sass valid" true report.valid;
  Alcotest.(check (list string)) "sass comment bodies"
    ["// parent\n    color: red\n      width: 1px"; "// root\n  nested: yes"]
    (raw_comments sass report);
  let nested = scan (Bytes.of_string "#{#{") Css
    { default_scan_options with dialect = Sass } in
  Alcotest.(check bool) "nested interpolation is invalid" false nested.valid;
  Alcotest.(check int) "nested interpolation reports once" 1
    (List.length nested.diagnostics)

let check_kotlin_multi_dollar_and_quote_runs () =
  let source = Bytes.of_string
    "val a = \"\"\"opaque\"\"\"\"// after run\nval b = $$\"\"\"${ /* opaque */ 1 } $${ run { /* code */ } }\"\"\" // tail\n" in
  let report = scan source Kotlin default_scan_options in
  Alcotest.(check bool) "kotlin valid" true report.valid;
  Alcotest.(check (list string)) "kotlin comments"
    ["// after run"; "/* code */"; "// tail"] (raw_comments source report)

let check_scala_characters_and_symbols () =
  let source = Bytes.of_string
    "val slash = '/'// after char\nval quote = '\\''// after escape\nval double = '\"'// after double quote\nval symbol = 'name // after symbol\n" in
  let report = scan source Scala default_scan_options in
  Alcotest.(check bool) "scala valid" true report.valid;
  Alcotest.(check (list string)) "scala comments"
    ["// after char"; "// after escape"; "// after double quote"; "// after symbol"]
    (raw_comments source report)

let check_markdown_commonmark_boundaries () =
  let source = Bytes.of_string
    "before\r    <!-- opaque cr -->\r\n    <!-- opaque crlf -->\nnext\n```rust `bad\n// not a Rust fence\n```\n```{r, echo=FALSE}\n# r comment\n```\n` unmatched\n<!-- visible -->\n" in
  let report = scan source Markdown default_scan_options in
  Alcotest.(check bool) "markdown valid" true report.valid;
  Alcotest.(check (list string)) "markdown comments"
    ["# r comment"; "<!-- visible -->"] (raw_comments source report)

let check_perl_compound_opaque_constructs () =
  let source = Bytes.of_string
    "print $#items, $^X, $!, $1; # variables\nmy $q = \"escaped \\\" # opaque\"; # quote\n$x =~ s/foo#one/bar#two/g; # substitution\n$x =~ tr/a#b/c#d/; # transliteration\nprint <<  \"ONE\", <<~'TWO';\n# first\nONE\n  # second\n  TWO\n=pod\n# pod\n=cutlery\n# still pod\n=cut\nformat STDOUT =\n@<<<<\n# picture\n.\n# after format\n__DATA__\n# data\n" in
  let report = scan source Perl default_scan_options in
  Alcotest.(check bool) "perl valid" true report.valid;
  Alcotest.(check (list string)) "perl comments"
    ["# variables"; "# quote"; "# substitution"; "# transliteration";
     "# after format"] (raw_comments source report);
  let false_queue = Bytes.of_string
    "print <<'REAL', \"<<FAKE\"; # <<ALSO\n# body\nREAL\n# after\n" in
  let report = scan false_queue Perl default_scan_options in
  Alcotest.(check (list string)) "only real heredoc declarations queue"
    ["# <<ALSO"; "# after"] (raw_comments false_queue report);
  let shift = Bytes.of_string "my $x = 1 << 2; # once\n" in
  let report = scan shift Perl default_scan_options in
  Alcotest.(check (list string)) "failed heredoc probe does not duplicate"
    ["# once"] (raw_comments shift report);
  let false_format = Bytes.of_string
    "// swift-format-ignore=head1</div>//!#nullable enable" in
  let report = scan false_format Perl default_scan_options in
  Alcotest.(check (list string)) "format is not recognized inside an expression"
    ["#nullable enable"] (raw_comments false_format report);
  let ambiguous = Bytes.of_string "// dart format offq}//!:title=" in
  let report = scan ambiguous Perl default_scan_options in
  Alcotest.(check bool) "false format does not hide slash ambiguity" false report.valid;
  Alcotest.(check bool) "slash ambiguity remains visible" true
    (List.exists (fun (diagnostic : diagnostic) ->
       diagnostic.code = "lexical-ambiguity") report.diagnostics)

let check_sfc_exact_attributes_and_sass () =
  let vue = Bytes.of_string
    "<template data-lang=\"pug\"><div data-v-pre v-if=\"ok /* directive */\" title=\"/* opaque */\">{{ 1 /* mustache */ }}</div><div v-pre><div></div><!-- opaque --></div><!-- outer --></template>" in
  let report = scan vue Vue default_scan_options in
  Alcotest.(check bool) "vue valid" true report.valid;
  Alcotest.(check (list string)) "vue comments"
    ["/* directive */"; "/* mustache */"; "<!-- outer -->"]
    (raw_comments vue report);
  let svelte = Bytes.of_string
    "<button title=\"/* opaque */\" data-pattern={/}>/.test(x) /* regex attribute */} on:click={() => { /* attribute */ }}>x</button>{ 1 /* body */ }" in
  let report = scan svelte Svelte default_scan_options in
  Alcotest.(check bool) "svelte valid" true report.valid;
  Alcotest.(check (list string)) "svelte comments"
    ["/* regex attribute */"; "/* attribute */"; "/* body */"]
    (raw_comments svelte report);
  let malformed_svelte = Bytes.of_string "<div x={1 /* once */}" in
  let report = scan malformed_svelte Svelte default_scan_options in
  Alcotest.(check bool) "malformed ordinary Svelte tag stays valid" true report.valid;
  Alcotest.(check (list string)) "failed Svelte tag probe rolls back before rescan"
    ["/* once */"] (raw_comments malformed_svelte report);
  let sass_style = Bytes.of_string
    "<style lang=\"sass\">\n// parent\n  color: red\n</style><p>{/* svelte */}</p>" in
  let report = scan sass_style Svelte default_scan_options in
  Alcotest.(check (list string)) "SFC Sass dialect"
    ["// parent\n  color: red"; "/* svelte */"] (raw_comments sass_style report)

let check_all_spans_are_bounded () =
  let source = Bytes.of_string ") /[" in
  let report = scan source Perl default_scan_options in
  let bounded span = span.start <= span.finish && span.finish <= Bytes.length source in
  Alcotest.(check bool) "comment bounds" true
    (List.for_all (fun (comment : comment) -> bounded comment.span) report.comments);
  Alcotest.(check bool) "diagnostic bounds" true
    (List.for_all (fun (diagnostic : diagnostic) -> bounded diagnostic.span) report.diagnostics)

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
    Alcotest.test_case "perl-pod" `Quick check_perl_pod;
    Alcotest.test_case "sass-and-scss-compounds" `Quick check_sass_and_scss_compounds;
    Alcotest.test_case "kotlin-multi-dollar" `Quick check_kotlin_multi_dollar_and_quote_runs;
    Alcotest.test_case "scala-characters" `Quick check_scala_characters_and_symbols;
    Alcotest.test_case "markdown-commonmark" `Quick check_markdown_commonmark_boundaries;
    Alcotest.test_case "perl-compounds" `Quick check_perl_compound_opaque_constructs;
    Alcotest.test_case "sfc-attributes-and-sass" `Quick check_sfc_exact_attributes_and_sass;
    Alcotest.test_case "span-bounds" `Quick check_all_spans_are_bounded;
  ]
]
