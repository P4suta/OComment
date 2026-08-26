type language =
  | Rust | Ocaml | C | Cpp | Go | Java | JavaScript | TypeScript | Python
  | Shell | Html | Css | Jsonc | Sql | Kotlin | Toml | Lua | Yaml | Php | Ruby
  | Zig | R | Dart | Unknown

type dialect =
  | Standard | Jsx | Tsx | ObjectiveC | ObjectiveCpp | GnuC | GnuCpp | Cuda
  | PosixSh | Bash53 | Zsh | PostgreSql | MySql | Sqlite | TSql | Oracle

type byte_span = { start : int; finish : int }

type comment_kind =
  | Line | Block | DocLine | DocBlock | Directive | License | HtmlComment
  | Shebang | Encoding | OptimizerHint | VersionComment

type disposition = Remove | Keep of string
type severity = Error | Warning | Info | Hint
type diagnostic = { code : string; message : string; severity : severity; span : byte_span }
type comment = { span : byte_span; kind : comment_kind; disposition : disposition }
type policy = Safe | Legal | All
type layout = Lines | Columns | Compact

type scan_options = {
  policy : policy;
  dialect : dialect;
  force_invalid : bool;
  force_protected : bool;
  keep_kinds : comment_kind list;
  remove_kinds : comment_kind list;
  keep_regex : string list;
  remove_regex : string list;
}

type transform_options = { scan : scan_options; layout : layout }
type scan_report = { language : language; comments : comment list; diagnostics : diagnostic list; valid : bool }
type edit = { span : byte_span; replacement : bytes }
type source_map_segment = { original : byte_span; output : byte_span; exact : bool }
type source_map = source_map_segment list
type transform_result = { output : bytes; edits : edit list; report : scan_report; source_map : source_map }

type line_delimiter = {
  line_start : string;
  requires_boundary : bool;
  line_kind : comment_kind;
}

type block_delimiter = {
  block_start : string;
  block_end_token : string;
  nested : bool;
  block_kind : comment_kind;
}

type string_delimiter = {
  string_start : string;
  string_end : string;
  escape : string option;
  multiline : bool;
}

type protected_pattern = { pattern : string; reason : string }

type declarative_profile = {
  name : string;
  extensions : string list;
  line_comments : line_delimiter list;
  block_comments : block_delimiter list;
  strings : string_delimiter list;
  protected_patterns : protected_pattern list;
}

let default_scan_options = {
  policy = Safe; dialect = Standard; force_invalid = false; force_protected = false;
  keep_kinds = []; remove_kinds = []; keep_regex = []; remove_regex = [];
}

let default_transform_options = { scan = default_scan_options; layout = Lines }

let lowercase value = String.lowercase_ascii value

let language_of_string value =
  match lowercase value with
  | "rust" | "rs" -> Ok Rust | "ocaml" | "ml" -> Ok Ocaml | "c" -> Ok C
  | "cpp" | "c++" | "cxx" -> Ok Cpp | "go" -> Ok Go | "java" -> Ok Java
  | "javascript" | "js" | "jsx" -> Ok JavaScript
  | "typescript" | "ts" | "tsx" -> Ok TypeScript | "python" | "py" -> Ok Python
  | "shell" | "sh" | "bash" | "zsh" -> Ok Shell | "html" | "htm" -> Ok Html
  | "css" -> Ok Css | "jsonc" | "json5" -> Ok Jsonc | "sql" -> Ok Sql
  | "kotlin" | "kt" | "kts" -> Ok Kotlin | "toml" -> Ok Toml | "lua" -> Ok Lua
  | "yaml" | "yml" -> Ok Yaml | "php" -> Ok Php | "ruby" | "rb" -> Ok Ruby
  | "zig" -> Ok Zig | "r" | "rscript" -> Ok R | "dart" -> Ok Dart
  | other -> Error ("unsupported language `" ^ other ^ "`")

let string_of_language = function
  | Rust -> "rust" | Ocaml -> "ocaml" | C -> "c" | Cpp -> "cpp" | Go -> "go"
  | Java -> "java" | JavaScript -> "javascript" | TypeScript -> "typescript"
  | Python -> "python" | Shell -> "shell" | Html -> "html" | Css -> "css"
  | Jsonc -> "jsonc" | Sql -> "sql" | Kotlin -> "kotlin" | Toml -> "toml"
  | Lua -> "lua" | Yaml -> "yaml" | Php -> "php" | Ruby -> "ruby"
  | Zig -> "zig" | R -> "r" | Dart -> "dart" | Unknown -> "unknown"

let string_of_comment_kind = function
  | Line -> "line" | Block -> "block" | DocLine -> "doc-line" | DocBlock -> "doc-block"
  | Directive -> "directive" | License -> "license" | HtmlComment -> "html-comment"
  | Shebang -> "shebang" | Encoding -> "encoding" | OptimizerHint -> "optimizer-hint"
  | VersionComment -> "version-comment"

let starts source index token =
  let source_length = Bytes.length source and token_length = String.length token in
  index >= 0 && index + token_length <= source_length &&
  let rec loop offset = offset = token_length ||
    (Bytes.get source (index + offset) = String.get token offset && loop (offset + 1)) in
  loop 0

let find_from source index token =
  let source_length = Bytes.length source and token_length = String.length token in
  let index = max 0 index in
  if token_length = 0 then Some (min index source_length)
  else if token_length <= 16 then
    let rec loop cursor =
      if cursor + token_length > source_length then None
      else if starts source cursor token then Some cursor else loop (cursor + 1)
    in loop index
  else begin
    let prefix = Array.make token_length 0 in
    let matched = ref 0 in
    for cursor = 1 to token_length - 1 do
      while !matched > 0 && String.get token !matched <> String.get token cursor do
        matched := prefix.(!matched - 1)
      done;
      if String.get token !matched = String.get token cursor then incr matched;
      prefix.(cursor) <- !matched
    done;
    let rec fallback matched character =
      if matched > 0 && String.get token matched <> character
      then fallback prefix.(matched - 1) character
      else matched in
    let rec search cursor matched =
      if cursor >= source_length then None else
      let character = Bytes.get source cursor in
      let matched = fallback matched character in
      let matched = if String.get token matched = character then matched + 1 else matched in
      if matched = token_length then Some (cursor + 1 - token_length)
      else search (cursor + 1) matched
    in search index 0
  end

let line_end source index =
  let rec loop cursor =
    if cursor >= Bytes.length source then cursor else
    match Bytes.get source cursor with '\r' | '\n' -> cursor | _ -> loop (cursor + 1)
  in loop index

(* NOTE: ASCII whitespace as `u8::is_ascii_whitespace` defines it: space, tab,
   line feed, form feed, carriage return.  The vertical tab is deliberately not
   in it, which is what several rules below turn on. *)
let ascii_whitespace = function
  | ' ' | '\t' | '\n' | '\r' | '\012' -> true
  | _ -> false

(* NOTE: ECMAScript WhiteSpace and LineTerminator, as far as one byte can say
   (ECMA-262 12.2, 12.3).  <VT> is whitespace to JavaScript, so a comparison
   written `a<VT><div>` is a comparison and not a JSX element.  The non-ASCII
   members -- U+00A0, U+FEFF, and the Zs category -- take more than one byte and
   are not decided here. *)
let js_is_space = function
  | ' ' | '\t' | '\n' | '\011' | '\012' | '\r' -> true
  | _ -> false

let js_identifier_start character =
  (character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
  character = '_' || character = '$' || Char.code character land 0x80 <> 0

let js_identifier_continue character =
  js_identifier_start character || (character >= '0' && character <= '9')

let mem_kind kind kinds = List.exists (( = ) kind) kinds

let compile_policy_regex pattern =
  let parsed_flags =
    if String.starts_with ~prefix:"(?" pattern then
      match String.index_from_opt pattern 2 ')' with
      | Some finish ->
        let flags = String.sub pattern 2 (finish - 2) in
        let rec options index result =
          if index = String.length flags then Some result else
          match String.get flags index with
          | 'i' -> options (index + 1) (`Caseless :: result)
          | 'm' -> options (index + 1) (`Multiline :: result)
          | 's' -> options (index + 1) (`Dotall :: result)
          | 'U' -> options (index + 1) (`Ungreedy :: result)
          | _ -> None in
        Option.map (fun opts -> opts, String.sub pattern (finish + 1)
          (String.length pattern - finish - 1)) (options 0 [])
      | None -> None
    else None in
  match parsed_flags with
  | Some (opts, body) -> Re.Perl.compile_pat ~opts body
  | None -> Re.Perl.compile_pat pattern

let regex_matches patterns raw =
  List.exists (fun pattern ->
    try Re.execp (compile_policy_regex pattern) raw
    with Re.Perl.Parse_error | Re.Perl.Not_supported -> false) patterns

let disposition options kind raw =
  if mem_kind kind options.keep_kinds || regex_matches options.keep_regex raw then
    Keep "kept by kind or regex override"
  else if (kind = Shebang || kind = Encoding) && not options.force_protected then Keep "required source preamble"
  else if mem_kind kind options.remove_kinds || regex_matches options.remove_regex raw then Remove
  else if options.policy = All then Remove
  else if kind = HtmlComment then Keep "HTML comments are DOM-observable"
  else if kind = Directive || kind = OptimizerHint || kind = VersionComment then Keep "tool or language directive"
  else if kind = License && options.policy = Legal then Keep "legal policy"
  else Remove

let contains text needle =
  let text_length = String.length text and needle_length = String.length needle in
  let rec loop index =
    index + needle_length <= text_length &&
    (String.sub text index needle_length = needle || loop (index + 1))
  in needle_length = 0 || loop 0

(* NOTE: The scalars Unicode gives the White_Space property.  Rust's
   `str::trim` removes every one of them and OCaml's `String.trim` removes five
   ASCII bytes, so a comment whose body opens with a no-break space or a line
   separator would classify differently on the two sides: `region` behind one is
   still the folding directive an editor reads, and keeping it is the
   conservative half of that disagreement. *)
let unicode_whitespace = function
  | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d | 0x20 | 0x85 | 0xa0 | 0x1680
  | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 -> true
  | value -> value >= 0x2000 && value <= 0x200a

(* NOTE: One UTF-8 scalar at `index`, as (scalar, width).  A byte that opens no
   well-formed sequence comes back on its own as U+FFFD, which is what
   `String.from_utf8_lossy` hands the Rust trim; the widths the two assign to a
   malformed run may differ, and cannot matter, because neither side calls
   U+FFFD whitespace.  Overlong encodings and surrogates are rejected for the
   same reason: `\xc0\xa0` is not a space to either lexer. *)
let utf8_decode text index =
  let length = String.length text in
  let byte offset = Char.code (String.get text (index + offset)) in
  let continuation offset = index + offset < length && byte offset land 0xc0 = 0x80 in
  let head = byte 0 in
  if head < 0x80 then (head, 1)
  else if head land 0xe0 = 0xc0 && head >= 0xc2 && continuation 1 then
    (((head land 0x1f) lsl 6) lor (byte 1 land 0x3f), 2)
  else if head land 0xf0 = 0xe0 && continuation 1 && continuation 2 then
    let scalar = ((head land 0x0f) lsl 12) lor ((byte 1 land 0x3f) lsl 6) lor (byte 2 land 0x3f) in
    if scalar < 0x800 || (scalar >= 0xd800 && scalar <= 0xdfff) then (0xfffd, 1) else (scalar, 3)
  else if head land 0xf8 = 0xf0 && continuation 1 && continuation 2 && continuation 3 then
    let scalar = ((head land 0x07) lsl 18) lor ((byte 1 land 0x3f) lsl 12) lor
      ((byte 2 land 0x3f) lsl 6) lor (byte 3 land 0x3f) in
    if scalar < 0x10000 || scalar > 0x10ffff then (0xfffd, 1) else (scalar, 4)
  else (0xfffd, 1)

let unicode_trim text =
  let length = String.length text in
  let rec front index =
    if index >= length then index
    else let scalar, width = utf8_decode text index in
      if unicode_whitespace scalar then front (index + width) else index in
  let start = front 0 in
  let rec back index finish =
    if index >= length then finish
    else let scalar, width = utf8_decode text index in
      back (index + width) (if unicode_whitespace scalar then finish else index + width) in
  String.sub text start (back start start - start)

let trim_markers raw =
  let markers = ["<!--"; "///"; "//!"; "//"; "/**"; "/*"; "(*"; "--"; "#"] in
  let endings = ["-->"; "*/"; "*)"] in
  let start = match List.find_opt (fun marker -> String.starts_with ~prefix:marker raw) markers with
    | Some marker -> String.length marker | None -> 0 in
  let finish = match List.find_opt (fun marker -> String.ends_with ~suffix:marker raw) endings with
    | Some marker -> String.length raw - String.length marker | None -> String.length raw in
  String.sub raw start (max 0 (finish - start)) |> unicode_trim |> lowercase

let is_legal text =
  List.exists (contains text) ["spdx-license-identifier"; "copyright"; "licensed under";
    "permission is hereby granted"; "all rights reserved"]

(* NOTE: A directive named after the tool that reads it is followed by the
   argument that tool takes, and whitespace of the writer's choosing separates
   the two, so the keyword ends at a boundary rather than at one particular
   byte. Matching the bare prefix would read prose that merely opens with those
   letters -- "# shellcheckish note" -- as an instruction as well.

   The end of the comment ends the keyword too: "#:schema" with its URL still to
   be typed is the directive it is about to be, and the text arrives trimmed, so
   refusing the empty remainder would protect the directive or not depending on
   a trailing space. *)
let opens_with_keyword compact keyword =
  let length = String.length keyword in
  String.starts_with ~prefix:keyword compact &&
  (String.length compact = length ||
   (match compact.[length] with
    | ' ' | '\t' | '\n' | '\012' | '\r' -> true
    | _ -> false))

(* NOTE: trim_markers takes the "--" off a Lua comment and leaves the third dash
   of "---@diagnostic" behind, which this is what removes. *)
let trim_dashes text =
  let length = String.length text in
  let rec loop index = if index < length && text.[index] = '-' then loop (index + 1) else index in
  let start = loop 0 in
  String.sub text start (length - start)

(* NOTE: ASCII whitespace as u8::is_ascii_whitespace defines it, taken off the
   end alone: dart_style compares a comment's text, which is trimmed there and
   not at the front. *)
let trim_ascii_end text =
  let rec loop finish =
    if finish > 0 && ascii_whitespace text.[finish - 1] then loop (finish - 1) else finish in
  String.sub text 0 (loop (String.length text))

(* NOTE: Dart's language version comment, which the scanner itself reads rather
   than a tool: "tokenizeLanguageVersionOrSingleLineComment" accepts exactly two
   slashes -- a third sends it to tokenizeSingleLineComment instead -- then
   spaces, "@dart" in lower case, spaces, "=", spaces, a run of digits, ".", a
   second run of digits, spaces, and the end of the line.  Only the space is
   skipped and not the tab: the scanner compares against $SPACE.

   The comment is honoured only ahead of the first real token of a file, and
   this is asked of every comment in one.  Reading a later one as an instruction
   keeps a comment a removal would otherwise take, which is the direction to be
   wrong in. *)
let dart_language_version raw =
  let length = String.length raw in
  let spaces index =
    let rec loop cursor =
      if cursor < length && raw.[cursor] = ' ' then loop (cursor + 1) else cursor in
    loop index in
  let digits index =
    let rec loop cursor =
      if cursor < length && raw.[cursor] >= '0' && raw.[cursor] <= '9' then loop (cursor + 1)
      else cursor in
    let finish = loop index in
    if finish = index then None else Some finish in
  let literal index token =
    let token_length = String.length token in
    if index + token_length <= length && String.sub raw index token_length = token
    then Some (index + token_length) else None in
  match literal 0 "//" with
  | None -> false
  | Some index ->
    if index < length && raw.[index] = '/' then false
    else match literal (spaces index) "@dart" with
      | None -> false
      | Some index ->
        match literal (spaces index) "=" with
        | None -> false
        | Some index ->
          match digits (spaces index) with
          | None -> false
          | Some index ->
            match literal index "." with
            | None -> false
            | Some index ->
              match digits index with
              | None -> false
              | Some index -> spaces index = length

let is_directive language text raw =
  let compact = text |> String.to_seq |>
    Seq.drop_while (fun character -> String.contains "!/*#@ " character) |> String.of_seq in
  let prefixes = ["sourcemappingurl="; "sourceurl="; "#__pure__"; "@__pure__";
    "__pure__"; "#__no_side_effects__"; "__no_side_effects__"; "ts-ignore";
    "ts-expect-error"; "ts-nocheck"; "ts-check"; "eslint"; "prettier-ignore";
    "stylelint";
    "noinspection"; "nolint"; "noqa"; "type: ignore"; "fmt:"; "rustfmt::";
    "clang-format"; "spotless:"; "ktlint-disable"; "ktlint-enable"; "detekt:";
    "istanbul ignore"; "c8 ignore"; "coverage:";
    "ocomment:"; "region"; "endregion"] in
  List.exists (fun prefix -> String.starts_with ~prefix compact) prefixes ||
  opens_with_keyword compact "shellcheck" ||
  match language with
  | Go -> String.starts_with ~prefix:"go:" compact || String.starts_with ~prefix:"+build" compact ||
      String.starts_with ~prefix:"line " compact
  | TypeScript -> String.starts_with ~prefix:"///" raw && String.starts_with ~prefix:"<" compact
  | C | Cpp -> String.starts_with ~prefix:"pragma" compact || String.starts_with ~prefix:"line " compact
  | Python -> List.exists (fun prefix -> String.starts_with ~prefix compact)
      ["pyright:"; "mypy:"; "ruff:"; "fmt:"]
  | Shell -> opens_with_keyword compact "hadolint" ||
      String.starts_with ~prefix:"syntax=" compact
  | Toml -> opens_with_keyword compact ":schema" ||
      String.starts_with ~prefix:"taplo:" compact
  (* NOTE: The language server's annotations are the only Lua comments that open
     with "---@", and "diagnostic" is the only one of them that instructs a tool
     rather than describing a type, so "raw" is what tells the annotation from
     prose about it. The four checkers below are addressed as "-- <tool>:",
     which carries its own boundary in the colon. *)
  | Lua -> (String.starts_with ~prefix:"---@" raw &&
      String.starts_with ~prefix:"@diagnostic" (trim_dashes text)) ||
      List.exists (fun prefix -> String.starts_with ~prefix compact)
        ["luacheck:"; "selene:"; "stylua:"; "luacov:"]
  (* NOTE: "@schema" is asked of the trimmed text rather than of "compact",
     because "compact" is what takes the "@" off: the annotation the Helm schema
     generator reads is spelled with it, and "schema" on its own is a word any
     comment about a schema opens with.  The three keywords after it are the
     whole word their tool answers to and end at a boundary; the four prefixes
     carry their own in a colon. *)
  | Yaml -> opens_with_keyword text "@schema" ||
      List.exists (opens_with_keyword compact) ["yamllint"; "nosec"; "kics-scan"] ||
      List.exists (fun prefix -> String.starts_with ~prefix compact)
        ["yaml-language-server:"; "renovate:"; "checkov:skip"; "trivy:ignore"]
  (* NOTE: Three of the four are asked of the trimmed text rather than of
     "compact", because "compact" is what takes the "@" off, and the "@" is what
     tells the annotation from prose about it.  "@psalm-suppress" is followed by
     the issue it silences after whitespace, so it ends at a boundary;
     "@phpstan-ignore" and "@codeCoverageIgnore" are namespaces whose members
     differ only in what runs on past them, so a prefix is the whole rule there.
     "phpcs:" carries its own boundary in the colon and covers "ignore",
     "disable", "enable" and "ignoreFile" alike. *)
  | Php -> opens_with_keyword text "@psalm-suppress" ||
      String.starts_with ~prefix:"@phpstan-ignore" text ||
      String.starts_with ~prefix:"@codecoverageignore" text ||
      String.starts_with ~prefix:"phpcs:" compact
  (* NOTE: Three of these six are Ruby's own magic comments, which the
     interpreter reads out of the head of a file: "frozen_string_literal"
     decides whether every literal string in it is frozen,
     "shareable_constant_value" what Ractor may share, and "warn_indent" whether
     the parser complains about the indentation.  The other three are the tools
     every Ruby project runs -- RuboCop, StandardRB, and Sorbet's "# typed:"
     sigil.  Each carries its own boundary in the colon and covers the whole
     namespace behind it.  The encoding declaration is deliberately absent: it
     is a kind of its own, classified before this runs. *)
  | Ruby -> List.exists (fun prefix -> String.starts_with ~prefix compact)
      ["frozen_string_literal:"; "warn_indent:"; "shareable_constant_value:";
       "rubocop:"; "standard:"; "typed:"]
  (* NOTE: The two comments an R tool reads rather than a reader.  styler turns
     its formatter off between "# styler: off" and "# styler: on", and the colon
     carries the marker's own boundary; covr excludes the lines between
     "# nocov start" and "# nocov end", and "nocov" is the whole word it looks
     for -- "start", "end" and nothing at all all follow it -- so that one ends
     at a boundary instead.  lintr's "# nolint" is protected for every language
     already and is deliberately absent here. *)
  | R -> opens_with_keyword compact "nocov" ||
      String.starts_with ~prefix:"styler:" compact
  (* NOTE: "zig fmt" reads its one instruction by equality rather than by prefix:
     Render.zig takes two bytes off the trimmed comment, trims the white space
     that follows, and compares the remainder with "zig fmt: off" and
     "zig fmt: on".  So "// zig fmt: off please" turns nothing off, and neither
     does "/// zig fmt: off" or "//// zig fmt: off" -- the first leaves one "/"
     in front of the phrase and the second two.  "raw" is what tells those
     apart, because trim_markers takes a "///" off whole; the comparison itself
     is against the trimmed text, which is folded to lower case here where
     "zig fmt" is case-sensitive, and folding can only keep a comment a removal
     would otherwise take. *)
  | Zig ->
    String.starts_with ~prefix:"//" raw &&
    not (String.length raw > 2 && (raw.[2] = '/' || raw.[2] = '!')) &&
    (text = "zig fmt: off" || text = "zig fmt: on")
  (* NOTE: Four instructions, and only one of them is addressed to a tool.
     "// @dart = 2.12" is read by the Dart scanner itself and decides which
     version of the language the file is written in, so a removal that took it
     would change what the remaining code means.  "dart format" is matched by
     equality on the whole comment rather than by prefix, because that is how
     dart_style matches it: piece_writer.dart switches on comment.text against
     "// dart format off" and "// dart format on", so "//   dart format off"
     with a second space and "/// dart format off" with a third slash turn
     nothing off -- measured on dart format from SDK 3.13.2, which reformatted
     both.  comment.text is trimmed at the end and not at the front, and this is
     asked of "raw" for the reason Zig's is: trim_markers takes a "///" off
     whole and would leave the two spellings indistinguishable.  The analyzer's
     two ignore comments each carry their own boundary in the colon
     (ignore_comments/ignore_info.dart). *)
  | Dart ->
    dart_language_version raw ||
    (let phrase = trim_ascii_end raw in
     phrase = "// dart format off" || phrase = "// dart format on") ||
    List.exists (fun prefix -> String.starts_with ~prefix compact)
      ["ignore:"; "ignore_for_file:"]
  | _ -> false

let within_first_two_lines source finish =
  let limit = min finish (Bytes.length source) in
  let rec loop index line_breaks =
    if index >= limit then true
    else if Bytes.get source index = '\r' then
      let next = if index + 1 < limit && Bytes.get source (index + 1) = '\n'
        then index + 2 else index + 1 in
      if line_breaks = 1 then false else loop next (line_breaks + 1)
    else if Bytes.get source index = '\n' then
      if line_breaks = 1 then false else loop (index + 1) (line_breaks + 1)
    else loop (index + 1) line_breaks
  in loop 0 0

(* NOTE: How many bytes of UTF-8 byte order mark the source opens with: three,
   or none.  A BOM is consumed before the first line is read -- CPython's
   `check_bom`, Lua's `skipBOM` -- so the line behind one is still the first
   line, and a preamble rule that asked for byte 0 alone would miss it.  The
   bytes stay where they are; only the question "is this the first line?" skips
   them. *)
let byte_order_mark_width source = if starts source 0 "\xef\xbb\xbf" then 3 else 0

(* NOTE: Python and Ruby share the phrase, down to the spelling: PEP 263 asks
   for "coding[:=]\s*([-\w.]+)" in one of the first two lines, and Ruby's
   magic_comment reads the same phrase out of the same two lines.  The Emacs
   form "# -*- coding: utf-8 -*-" satisfies both, which is why both languages
   are written with it.

   What the two do not share is which second line counts, and the rule here is
   neither of theirs: any "coding:" comment on either of the first two lines is
   a declaration, whatever stands on the line above it.  Ruby reads the second
   line only behind a "#!" line, and Python only behind a line that is itself a
   comment or blank -- so "x = 1\n# coding: us-ascii\n" names an encoding to
   neither of them (Ruby 3.3.12 reports __ENCODING__ as UTF-8, and
   tokenize.detect_encoding reports utf-8), and this function calls it a
   declaration all the same.  Saying yes only ever keeps a comment "safe" would
   otherwise remove, and the two ways to be wrong are not the same size: a
   missed declaration removes the line a file's encoding is written on, an
   invented one leaves an ordinary comment in place. *)
let encoding_declaration source start raw =
  if not (within_first_two_lines source start) || not (String.starts_with ~prefix:"#" raw)
  then false else
  let rec find_line_start index =
    if index = 0 then 0
    else if Bytes.get source (index - 1) = '\r' || Bytes.get source (index - 1) = '\n'
    then index else find_line_start (index - 1) in
  let line_start = find_line_start start in
  let prefix_start = if line_start = 0 then byte_order_mark_width source else line_start in
  let rec prefix_is_space index =
    index >= start || match Bytes.get source index with
      | ' ' | '\t' | '\x0c' -> prefix_is_space (index + 1)
      | _ -> false in
  let body = String.sub raw 1 (String.length raw - 1) in
  let rec find_coding index =
    if index + 6 > String.length body then None
    else if String.sub body index 6 = "coding" then Some (index + 6)
    else find_coding (index + 1) in
  let valid_encoding_char = function
    | 'a' .. 'z' | 'A' .. 'Z' | '0' .. '9' | '-' | '_' | '.' -> true
    | _ -> false in
  prefix_is_space prefix_start && match find_coding 0 with
  | None -> false
  | Some cursor ->
    if cursor >= String.length body ||
      (String.get body cursor <> ':' && String.get body cursor <> '=') then false
    else
      let rec skip_space index =
        if index < String.length body &&
          (String.get body index = ' ' || String.get body index = '\t')
        then skip_space (index + 1) else index in
      let encoding = skip_space (cursor + 1) in
      encoding < String.length body && valid_encoding_char (String.get body encoding)

let classify source language lexical start finish =
  let raw = Bytes.sub_string source start (finish - start) in
  let text = trim_markers raw in
  if start = byte_order_mark_width source && String.starts_with ~prefix:"#!" raw then Shebang
  else if (language = Python || language = Ruby) &&
    encoding_declaration source start raw then Encoding
  else if language = Sql && String.starts_with ~prefix:"/*+" raw then OptimizerHint
  else if language = Sql && String.starts_with ~prefix:"/*!" raw then VersionComment
  else if is_legal text then License else if is_directive language text raw then Directive else lexical

(* NOTE: One YAML block scalar, as the two things the lines below it depend on:
   where its body stopped, and whether its header asked to keep the empty lines
   trailing it.  Where a body ends is decided by the column of the node the
   header hangs off, which is not written on the header's own line -- "key:" on
   one line and "|" on the next is the same scalar as "key: |" -- so only a scan
   knows it.  The chomping indicator is carried as a bool rather than as the
   "chomping" type, which the YAML section further down defines: "keep" is the
   only one of the three these lines can tell apart.  The content indentation is
   the other half a trail needs: the explicit indication indicator counted from
   the owner, or the indentation of the first non-empty line where the header
   spelled none out (8.1.1.1).  A line under the body that reaches that depth is
   content of it and one that does not is outside it, which is the difference
   between a trail comment a removal may take and one it may not. *)
type yaml_block_scalar = { body_end : int; content_indent : int; keeps_empties : bool }

type accumulator = {
  mutable comments_rev : comment list;
  mutable diagnostics_rev : diagnostic list;
  mutable yaml_blocks_rev : yaml_block_scalar list;
}

let add_comment accumulator source language options lexical start finish =
  let kind = classify source language lexical start finish in
  let raw = Bytes.sub_string source start (finish - start) in
  accumulator.comments_rev <- { span = { start; finish }; kind; disposition = disposition options kind raw } :: accumulator.comments_rev

let add_error accumulator code message start finish =
  accumulator.diagnostics_rev <- { code; message; severity = Error; span = { start; finish } } :: accumulator.diagnostics_rev

let quoted_end source start multiline =
  let quote = Bytes.get source start in
  let rec loop index =
    if index >= Bytes.length source then (index, false)
    else match Bytes.get source index with
      | '\\' -> loop (min (Bytes.length source) (index + 2))
      | character when character = quote -> (index + 1, true)
      | ('\r' | '\n') when not multiline -> (index, false)
      | _ -> loop (index + 1)
  in loop (start + 1)

let block_end source start nested =
  let rec loop index depth =
    if index >= Bytes.length source then (index, false)
    else if nested && starts source index "/*" then loop (index + 2) (depth + 1)
    else if starts source index "*/" then if depth = 1 then (index + 2, true) else loop (index + 2) (depth - 1)
    else loop (index + 1) depth
  in loop (start + 2) 1

type mapped_bytes = { mapped : bytes; origins : byte_span array; original_length : int }

let mapped_span mapping span =
  if span.start = span.finish then
    let point = if span.start < Array.length mapping.origins
      then mapping.origins.(span.start).start else mapping.original_length in
    { start = point; finish = point }
  else
    let start = if span.start < Array.length mapping.origins
      then mapping.origins.(span.start).start else mapping.original_length in
    let finish = if span.finish = Bytes.length mapping.mapped then mapping.original_length
      else if span.finish > 0 && span.finish - 1 < Array.length mapping.origins
      then mapping.origins.(span.finish - 1).finish else mapping.original_length in
    { start; finish }

let merge_mapped accumulator report mapping =
  List.iter (fun (comment : comment) ->
    accumulator.comments_rev <- { comment with span = mapped_span mapping comment.span }
      :: accumulator.comments_rev) report.comments;
  List.iter (fun (diagnostic : diagnostic) ->
    accumulator.diagnostics_rev <- { diagnostic with span = mapped_span mapping diagnostic.span }
      :: accumulator.diagnostics_rev) report.diagnostics

let without_c_line_splices source =
  let buffer = Buffer.create (Bytes.length source) and origins = ref [] in
  let rec loop index =
    if index >= Bytes.length source then ()
    else if starts source index "\\\r\n" then loop (index + 3)
    else if starts source index "\\\n" then loop (index + 2)
    else begin
      Buffer.add_char buffer (Bytes.get source index);
      origins := { start = index; finish = index + 1 } :: !origins;
      loop (index + 1)
    end
  in
  loop 0;
  { mapped = Bytes.of_string (Buffer.contents buffer);
    origins = Array.of_list (List.rev !origins); original_length = Bytes.length source }

let hex_digit = function
  | '0' .. '9' as value -> Some (Char.code value - Char.code '0')
  | 'a' .. 'f' as value -> Some (Char.code value - Char.code 'a' + 10)
  | 'A' .. 'F' as value -> Some (Char.code value - Char.code 'A' + 10)
  | _ -> None

let hex4 source start =
  let rec loop index value =
    if index = start + 4 then Some value
    else match hex_digit (Bytes.get source index) with
      | Some digit -> loop (index + 1) (value * 16 + digit)
      | None -> None
  in
  if start + 4 <= Bytes.length source then loop start 0 else None

let java_unicode source =
  let buffer = Buffer.create (Bytes.length source) and origins = ref [] and invalid = ref [] in
  let add_origin start finish character =
    Buffer.add_char buffer character;
    origins := { start; finish } :: !origins in
  let add_codepoint start finish value =
    let encoded = Buffer.create 4 in
    Buffer.add_utf_8_uchar encoded (Uchar.of_int value);
    String.iter (add_origin start finish) (Buffer.contents encoded) in
  let rec loop index slash_run last_was_escape =
    if index >= Bytes.length source then () else
    let eligible = Bytes.get source index = '\\' && (last_was_escape || slash_run mod 2 = 0) in
    if eligible then begin
      let cursor = ref (index + 1) in
      while !cursor < Bytes.length source && Bytes.get source !cursor = 'u' do incr cursor done;
      match if !cursor > index + 1 then hex4 source !cursor else None with
      | Some value when value <= 0x7f ->
        add_origin index (!cursor + 4) (Char.chr value);
        if value = Char.code '\\' then loop (!cursor + 4) (slash_run + 1) true
        else loop (!cursor + 4) 0 false
      | Some value when value < 0xd800 || (value > 0xdfff && value <= 0x10ffff) ->
        add_codepoint index (!cursor + 4) value;
        loop (!cursor + 4) 0 false
      | Some _ when !cursor > index + 1 ->
        add_origin index (!cursor + 4) (Char.chr 0x80);
        loop (!cursor + 4) 0 false
      | _ ->
        if !cursor > index + 1 then
          invalid := { start = index; finish = min (Bytes.length source) (!cursor + 4) } :: !invalid;
        add_origin index (index + 1) (Bytes.get source index);
        loop (index + 1) (slash_run + 1) false
    end else begin
      let character = Bytes.get source index in
      add_origin index (index + 1) character;
      loop (index + 1) (if character = '\\' then slash_run + 1 else 0) false
    end
  in
  loop 0 0 false;
  ({ mapped = Bytes.of_string (Buffer.contents buffer);
    origins = Array.of_list (List.rev !origins); original_length = Bytes.length source },
   List.rev !invalid)

let cpp_raw_end source index =
  let prefixes = ["R\""; "u8R\""; "uR\""; "UR\""; "LR\""] in
  match List.find_opt (starts source index) prefixes with
  | None -> None
  | Some prefix ->
    let delimiter_start = index + String.length prefix in
    (match find_from source delimiter_start "(" with
    | None -> None
    | Some opening ->
      let delimiter = Bytes.sub_string source delimiter_start (opening - delimiter_start) in
      (* NOTE: [lex.string]: a d-char is any member of the basic source
         character set except space, "(", ")", "\\", and the control characters
         horizontal tab, vertical tab, form feed and new-line. *)
      if String.length delimiter > 16 || String.exists
        (fun character -> String.contains " ()\\\t\011\012\n\r" character) delimiter
      then None
      else let closing = ")" ^ delimiter ^ "\"" in
        match find_from source (opening + 1) closing with
        | Some finish -> Some (finish + String.length closing, true)
        | None -> Some (Bytes.length source, false))

(* NOTE: The C++ raw string literal a '"' opens, as the offset of its prefix, or
   None when the quote opens an ordinary one.  The question is asked at the
   quote and answered backwards, because a prefix is only a prefix where no
   identifier runs into it: `aR"(x)"` is the identifier `aR` and then a plain
   string, not a raw string beginning in the middle of a name. *)
let cpp_raw_start_at_quote source quote =
  let prefixes = ["R\""; "u8R\""; "uR\""; "UR\""; "LR\""] in
  List.find_map (fun prefix ->
    let start = quote - String.length prefix + 1 in
    if start >= 0 && starts source start prefix &&
      (start = 0 || not (js_identifier_continue (Bytes.get source (start - 1))))
    then Some start else None) prefixes

let c_quote_start source index =
  let length = Bytes.length source in
  if index >= length then None
  else if Bytes.get source index = '"' || Bytes.get source index = '\'' then Some index
  else if String.contains "LuU" (Bytes.get source index) && index + 1 < length &&
    (Bytes.get source (index + 1) = '"' || Bytes.get source (index + 1) = '\'')
  then Some (index + 1)
  else if (starts source index "u8\"" || starts source index "u8'") then Some (index + 2)
  else None

(* NOTE: The raw string literal a '"' closes the opener of, as (start, hashes),
   or None when the quote opens an ordinary one.  The question is asked at the
   quote and answered backwards, because that is where the lexer stands: the
   run of '#' before it, then the 'r', then an optional 'b' or 'c' prefix, and
   then a byte that must not continue an identifier -- `bar"x"` is a call on a
   string, not a raw string starting in the middle of a name. *)
let rust_raw_start_at_quote source quote =
  let cursor = ref quote in
  while !cursor > 0 && Bytes.get source (!cursor - 1) = '#' do decr cursor done;
  let hashes = quote - !cursor in
  if !cursor = 0 || Bytes.get source (!cursor - 1) <> 'r' then None
  else begin
    let start = ref (!cursor - 1) in
    if !start > 0 && (Bytes.get source (!start - 1) = 'b' || Bytes.get source (!start - 1) = 'c')
    then decr start;
    if !start > 0 && js_identifier_continue (Bytes.get source (!start - 1)) then None
    else Some (!start, hashes)
  end

(* INVARIANT: The two bytes that end a line everywhere a checkpoint may be
   offered.  A bounded lookahead that decides a token asks this before it reads
   one byte further: a checkpoint sits at the line start behind a terminator,
   and it promises that nothing decided before it depends on bytes after it. *)
let is_line_terminator character = character = '\r' || character = '\n'

(* NOTE: Rust Reference, Lifetimes and loop labels: an apostrophe followed by an
   identifier that no second apostrophe closes is a lifetime, so it opens no
   literal at all and a `//` behind it on the same line is a comment.  What
   tells the two apart is the shape after the quote: an escape closed four bytes
   on, a single byte closed two bytes on, or a non-ASCII character with a quote
   near enough behind it to be the closing one.
   INVARIANT: none of those windows may run past a line terminator.  A Rust
   character literal ends at the line (Rust Reference, Tokens) and `\` before a
   line terminator is a string continuation rather than a character escape, so
   every shape a crossing window would have caught is invalid Rust -- and
   `rustc` 1.97 reads `'<non-ASCII>` with the closing quote on the next line as
   a lifetime, reporting E0762 against that next line instead. *)
let rust_char_start source index =
  let length = Bytes.length source in
  index + 1 < length &&
  let next = Bytes.get source (index + 1) in
  (not (is_line_terminator next)) &&
  if next = '\\' then
    index + 3 < length
    && (not (is_line_terminator (Bytes.get source (index + 2))))
    && Bytes.get source (index + 3) = '\''
  else if index + 2 < length && Bytes.get source (index + 2) = '\'' then true
  else Char.code next land 0x80 <> 0 &&
    (let limit = min length (index + 6) in
     let rec loop cursor =
       cursor < limit && (not (is_line_terminator (Bytes.get source cursor)))
       && (Bytes.get source cursor = '\'' || loop (cursor + 1)) in
     loop (index + 1))

(* NOTE: One quoted literal, with the diagnostic the language spells for it when
   nothing closes it.  The construct is named -- "unterminated Rust raw string",
   "unterminated string or rune literal" -- because the message is what a user
   reads, and "literal" tells them nothing they did not already know. *)
let quoted_or_error source accumulator start multiline name =
  let finish, closed = quoted_end source start multiline in
  if not closed then
    add_error accumulator "unterminated-string" ("unterminated " ^ name) start finish;
  finish

let rec scan_kotlin_string source options accumulator start triple depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit" "Kotlin string-template nesting limit exceeded"
      start start;
    Bytes.length source
  end else
  let delimiter = if triple then "\"\"\"" else "\"" in
  let rec loop index =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-string"
        (if triple then "unterminated Kotlin triple-quoted string"
          else "unterminated Kotlin string") start index;
      index
    end else if starts source index delimiter then index + String.length delimiter
    else if not triple && Bytes.get source index = '\\' then
      loop (min (Bytes.length source) (index + 2))
    else if starts source index "${" then
      loop (scan_kotlin_expression source options accumulator (index + 2) (depth + 1))
    else if not triple && (Bytes.get source index = '\r' || Bytes.get source index = '\n')
    then begin
      add_error accumulator "unterminated-string" "unterminated Kotlin string" start index;
      index
    end else loop (index + 1)
  in loop (start + String.length delimiter)

and scan_kotlin_expression source options accumulator index depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit" "Kotlin string-template nesting limit exceeded"
      index index;
    Bytes.length source
  end else
  let rec loop index braces =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-template-expression"
        "unterminated Kotlin string-template expression" index index;
      index
    end else if starts source index "//" then begin
      let finish = line_end source (index + 2) in
      let kind = if starts source index "///" || starts source index "//!" then DocLine else Line in
      add_comment accumulator source Kotlin options kind index finish;
      loop finish braces
    end else if starts source index "/*" then begin
      let finish, closed = block_end source index true in
      let kind = if starts source index "/**" || starts source index "/*!" then DocBlock else Block in
      add_comment accumulator source Kotlin options kind index finish;
      if not closed then add_error accumulator "unterminated-comment"
        "unterminated Kotlin block comment" index finish;
      loop finish braces
    end else if starts source index "\"\"\"" then
      loop (scan_kotlin_string source options accumulator index true (depth + 1)) braces
    else match Bytes.get source index with
    | '"' -> loop (scan_kotlin_string source options accumulator index false (depth + 1)) braces
    | '\'' ->
      let finish, closed = quoted_end source index false in
      if not closed then add_error accumulator "unterminated-string"
        "unterminated Kotlin character literal" index finish;
      loop finish braces
    | '{' -> loop (index + 1) (braces + 1)
    | '}' -> let remaining = braces - 1 in
      if remaining = 0 then index + 1 else loop (index + 1) remaining
    | _ -> loop (index + 1) braces
  in loop index 1

let scan_slash_unmapped source language options accumulator =
  let nested = language = Rust || language = Kotlin in
  let line_comments = language <> Css in
  let rec loop index =
    if index >= Bytes.length source then ()
    else if line_comments && starts source index "//" then begin
      let finish = line_end source (index + 2) in
      let kind = if starts source index "///" || starts source index "//!" then DocLine else Line in
      add_comment accumulator source language options kind index finish; loop finish
    end else if starts source index "/*" then begin
      let finish, closed = block_end source index nested in
      let kind = if starts source index "/**" || starts source index "/*!" then DocBlock else Block in
      add_comment accumulator source language options kind index finish;
      if not closed then add_error accumulator "unterminated-comment" "unterminated block comment" index finish;
      loop finish
    end else
      let character = Bytes.get source index in
      match language with
      | Rust when character = '"' ->
        (match rust_raw_start_at_quote source index with
        | Some (raw_start, hashes) ->
          let ending = "\"" ^ String.make hashes '#' in
          (match find_from source (index + 1) ending with
          | Some close -> loop (close + String.length ending)
          | None ->
            add_error accumulator "unterminated-string" "unterminated Rust raw string"
              raw_start (Bytes.length source))
        (* INVARIANT: a Rust string or byte-string literal carries a bare
           newline as content, so only its closing quote or the end of the file
           ends one; a Rust character literal still ends at the line. *)
        | None -> loop (quoted_or_error source accumulator index true "string"))
      | Rust when character = '\'' && rust_char_start source index ->
        loop (quoted_or_error source accumulator index false "character literal")
      (* NOTE: What is left is an apostrophe the window read as no literal, and
         nothing on its line says whether it opens one.  A Rust identifier is
         `XID_Start XID_Continue*` (Rust Reference, Identifiers) and has been
         since 1.53, so `'ä` is as good a lifetime or loop label as `'a` --
         `fn f<'ä>() {}` and `'ä: loop {}` both compile -- and an unterminated
         non-ASCII character literal is spelled the same way within one line.
         `rustc` tells the two apart in the parser, which is where E0762 is
         raised; this scanner is a lexer with a line-bounded window and cannot.
         So it reports neither: over-keeping a comment is the safe direction,
         calling a valid file invalid is not. *)
      | Rust when character = '\'' -> loop (index + 1)
      (* NOTE: A raw string prefix is only a prefix where no identifier runs
         into it and the delimiter is made of d-chars, so both questions are
         asked before the quote is read as one; otherwise it opens an ordinary
         literal, exactly as `c_quote_start` says. *)
      | C | Cpp ->
        let raw =
          if language = Cpp && character = '"' then
            Option.bind (cpp_raw_start_at_quote source index) (fun start ->
              Option.map (fun (finish, closed) -> (start, finish, closed))
                (cpp_raw_end source start))
          else None in
        (match raw with
        | Some (start, finish, closed) ->
          if not closed then add_error accumulator "unterminated-string"
            "unterminated C++ raw string" start finish;
          loop finish
        | None -> (match c_quote_start source index with
          | Some quote ->
            loop (quoted_or_error source accumulator quote false "string or character literal")
          | None -> loop (index + 1)))
      | Go when character = '`' ->
        (match find_from source (index + 1) "`" with
        | Some finish -> loop (finish + 1)
        | None -> add_error accumulator "unterminated-string" "unterminated raw string"
            index (Bytes.length source))
      | Go when character = '"' || character = '\'' ->
        loop (quoted_or_error source accumulator index false "string or rune literal")
      | Kotlin when starts source index "\"\"\"" ->
        loop (scan_kotlin_string source options accumulator index true 0)
      | Kotlin when character = '"' ->
        loop (scan_kotlin_string source options accumulator index false 0)
      | Kotlin when character = '\'' ->
        loop (quoted_or_error source accumulator index false "Kotlin character literal")
      (* NOTE: JSON5 4.4 writes a string with either quote, and this language is
         "JSON with comments, including JSON5" -- it owns ".json5" as well as
         ".jsonc".  An apostrophe is already invalid in the stricter dialect, so
         reading one as a string only hides a "//" that dialect could not have
         meant as a comment. *)
      | Jsonc when character = '"' || character = '\'' ->
        loop (quoted_or_error source accumulator index false "JSON string")
      | Css when character = '"' || character = '\'' ->
        loop (quoted_or_error source accumulator index true "CSS string")
      | _ -> loop (index + 1)
  in loop 0

let scan_slash source language options accumulator =
  if (language = C || language = Cpp) &&
    (find_from source 0 "\\\n" <> None || find_from source 0 "\\\r\n" <> None)
  then begin
    let mapping = without_c_line_splices source in
    let child = { comments_rev = []; diagnostics_rev = []; yaml_blocks_rev = [] } in
    scan_slash_unmapped mapping.mapped language options child;
    let comments = List.rev child.comments_rev and diagnostics = List.rev child.diagnostics_rev in
    merge_mapped accumulator
      { language; comments; diagnostics;
        valid = not (List.exists (fun diagnostic -> diagnostic.severity = Error) diagnostics) }
      mapping
  end else scan_slash_unmapped source language options accumulator

let java_text_block_end source start =
  let length = Bytes.length source in
  let rec preceding_backslashes cursor count =
    if cursor > start + 3 && Bytes.get source (cursor - 1) = '\\'
    then preceding_backslashes (cursor - 1) (count + 1)
    else count in
  let rec loop index =
    if index + 2 >= length then (length, false)
    else if starts source index "\"\"\"" &&
      preceding_backslashes index 0 mod 2 = 0
    then (index + 3, true)
    else loop (index + 1) in
  loop (start + 3)

let scan_java source language options accumulator =
  let mapping, invalid_unicode = java_unicode source in
  List.iter (fun span -> add_error accumulator "invalid-unicode-escape"
    "invalid Java Unicode escape" span.start span.finish) invalid_unicode;
  let mapped = mapping.mapped and child = { comments_rev = []; diagnostics_rev = []; yaml_blocks_rev = [] } in
  let rec loop index =
    if index >= Bytes.length mapped then ()
    else if starts mapped index "//" then begin
      let finish = line_end mapped (index + 2) in
      add_comment child mapped language options
        (if starts mapped index "///" then DocLine else Line) index finish;
      loop finish
    end else if starts mapped index "/*" then begin
      let finish, closed = block_end mapped index false in
      add_comment child mapped language options
        (if starts mapped index "/**" then DocBlock else Block) index finish;
      if not closed then add_error child "unterminated-comment" "unterminated block comment" index finish;
      loop finish
    end else if starts mapped index "\"\"\"" then begin
      let finish, closed = java_text_block_end mapped index in
      if closed then loop finish
      else add_error child "unterminated-string" "unterminated Java text block" index finish
    end else if Bytes.get mapped index = '"' || Bytes.get mapped index = '\'' then begin
      let finish, closed = quoted_end mapped index false in
      if not closed then add_error child "unterminated-string" "unterminated Java literal" index finish;
      loop finish
    end else loop (index + 1)
  in
  loop 0;
  let comments = List.rev child.comments_rev and diagnostics = List.rev child.diagnostics_rev in
  merge_mapped accumulator
    { language; comments; diagnostics;
      valid = not (List.exists (fun diagnostic -> diagnostic.severity = Error) diagnostics) }
    mapping

let unicode_line_terminator_width source index =
  if starts source index "\r\n" then Some 2
  else if index < Bytes.length source &&
    (Bytes.get source index = '\r' || Bytes.get source index = '\n')
  then Some 1
  else if starts source index "\226\128\168" || starts source index "\226\128\169"
  then Some 3
  else None

let js_line_end source index =
  let rec loop cursor =
    if cursor >= Bytes.length source || unicode_line_terminator_width source cursor <> None
    then cursor else loop (cursor + 1) in
  loop index

let js_quoted_end source start =
  let quote = Bytes.get source start in
  let rec loop index =
    if index >= Bytes.length source then (index, false)
    else if Bytes.get source index = '\\' then
      let escaped = index + 1 in
      (match unicode_line_terminator_width source escaped with
      | Some width -> loop (escaped + width)
      | None -> loop (min (Bytes.length source) (index + 2)))
    else if Bytes.get source index = quote then (index + 1, true)
    else if unicode_line_terminator_width source index <> None then (index, false)
    else loop (index + 1)
  in loop (start + 1)

(* NOTE: ECMA-262 12.5 makes a SingleLineHTMLCloseComment of a "-->" that
   nothing but white space precedes on its line.  U+FEFF is <ZWNBSP>, which 12.2
   lists among WhiteSpace wherever it sits and however many of it there are --
   the start of a file is only the most common place to meet one -- and it takes
   three bytes, which is why the prefix is walked rather than handed to
   js_is_space byte by byte. *)
let js_html_close_comment source index =
  starts source index "-->" &&
  let rec line_start cursor =
    if cursor = 0 then 0 else
    match Bytes.get source (cursor - 1) with
    | '\r' | '\n' -> cursor
    | _ -> line_start (cursor - 1) in
  let rec whitespace cursor =
    cursor >= index ||
    (if starts source cursor "\xef\xbb\xbf" then whitespace (cursor + 3)
     else js_is_space (Bytes.get source cursor) && whitespace (cursor + 1)) in
  whitespace (line_start index)

let js_regex_end source start =
  let rec loop index in_class =
    if index >= Bytes.length source ||
      unicode_line_terminator_width source index <> None
    then None else
    match Bytes.get source index with
    | '\\' ->
      if unicode_line_terminator_width source (index + 1) <> None then None
      else loop (min (Bytes.length source) (index + 2)) in_class
    | '[' -> loop (index + 1) true
    | ']' -> loop (index + 1) false
    | '/' when not in_class ->
      let rec flags cursor =
        if cursor < Bytes.length source then
          let character = Bytes.get source cursor in
          if (character >= 'a' && character <= 'z') ||
            (character >= 'A' && character <= 'Z') || character = '_'
          then flags (cursor + 1) else cursor
        else cursor
      in Some (flags (index + 1))
    | _ -> loop (index + 1) in_class
  in loop (start + 1) false

let jsx_open source index =
  index + 1 < Bytes.length source && Bytes.get source index = '<' &&
  let next = Bytes.get source (index + 1) in
  (next >= 'a' && next <= 'z') || (next >= 'A' && next <= 'Z') ||
  next = '>' || next = '_'

let rec scan_js_code source language options accumulator index stop_brace depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit" "JavaScript lexical nesting limit exceeded" index index;
    Bytes.length source
  end else
  let rec loop index brace_depth regex_allowed control_parentheses pending_control
      brace_blocks statement_start pending_block =
    if index >= Bytes.length source then begin
      (match stop_brace with Some _ -> add_error accumulator
        "unterminated-template-expression" "unterminated JavaScript template expression"
        index index | None -> ());
      index
    end else if starts source index "//" then begin
      let finish = js_line_end source (index + 2) in
      add_comment accumulator source language options
        (if starts source index "///" || starts source index "//!" then DocLine else Line)
        index finish;
      loop finish brace_depth regex_allowed control_parentheses pending_control
        brace_blocks statement_start pending_block
    end else if starts source index "/*" then begin
      let finish, closed = block_end source index false in
      add_comment accumulator source language options
        (if starts source index "/**" || starts source index "/*!" then DocBlock else Block)
        index finish;
      if not closed then add_error accumulator "unterminated-comment"
        "unterminated JavaScript block comment" index finish;
      loop finish brace_depth regex_allowed control_parentheses pending_control
        brace_blocks statement_start pending_block
    end else if starts source index "<!--" || js_html_close_comment source index then begin
      let finish = js_line_end source (index + 3) in
      add_comment accumulator source language options Line index finish;
      loop finish brace_depth regex_allowed control_parentheses pending_control
        brace_blocks statement_start pending_block
    end else if (options.dialect = Jsx || options.dialect = Tsx) && regex_allowed &&
      jsx_open source index
    then loop (scan_jsx_element source language options accumulator index (depth + 1))
      brace_depth false control_parentheses false brace_blocks false false
    else match Bytes.get source index with
    | '\'' | '"' ->
      let finish, closed = js_quoted_end source index in
      if not closed then add_error accumulator "unterminated-string"
        "unterminated JavaScript string" index finish;
      loop finish brace_depth false control_parentheses false brace_blocks false false
    | '`' -> loop (scan_js_template source language options accumulator index (depth + 1))
        brace_depth false control_parentheses false brace_blocks false false
    | '/' when regex_allowed -> (match js_regex_end source index with
      | Some finish ->
        loop finish brace_depth false control_parentheses false brace_blocks false false
      | None ->
        loop (index + 1) brace_depth true control_parentheses false
          brace_blocks false false)
    | '{' ->
      let is_block = pending_block || not regex_allowed || statement_start in
      let new_depth = if stop_brace <> None then brace_depth + 1 else brace_depth in
      loop (index + 1) new_depth true control_parentheses false
        (is_block :: brace_blocks) is_block false
    | '}' ->
      let new_depth = if stop_brace <> None then max 0 (brace_depth - 1)
        else brace_depth in
      if stop_brace <> None && new_depth = 0 then index + 1
      else let is_block, tail = match brace_blocks with
        | value :: rest -> value, rest | [] -> true, [] in
        loop (index + 1) new_depth is_block control_parentheses false
          tail is_block false
    | character when js_identifier_start character ||
        (character >= '0' && character <= '9') ->
      let rec identifier_end cursor =
        if cursor < Bytes.length source && js_identifier_continue (Bytes.get source cursor)
        then identifier_end (cursor + 1) else cursor in
      let finish = identifier_end (index + 1) in
      let token = Bytes.sub_string source index (finish - index) in
      let control = List.mem token ["if"; "while"; "for"; "with"; "switch"; "catch"] in
      let allows = control || List.mem token
        ["return"; "throw"; "case"; "delete"; "void"; "typeof"; "yield";
         "await"; "new"; "in"; "of"; "else"; "do"] in
      let block = List.mem token ["else"; "do"; "try"; "finally"] in
      loop finish brace_depth allows control_parentheses control
        brace_blocks false block
    | '(' -> loop (index + 1) brace_depth true
        (pending_control :: control_parentheses) false brace_blocks false false
    | ')' -> let control, tail = match control_parentheses with
        | value :: rest -> value, rest | [] -> false, [] in
      loop (index + 1) brace_depth control tail false brace_blocks control control
    | ']' ->
      loop (index + 1) brace_depth false control_parentheses false
        brace_blocks false false
    | ('+' | '-' as character) when index + 1 < Bytes.length source &&
        Bytes.get source (index + 1) = character ->
      loop (index + 2) brace_depth false control_parentheses false
        brace_blocks false false
    | character when js_is_space character ->
      loop (index + 1) brace_depth regex_allowed control_parentheses pending_control
        brace_blocks statement_start pending_block
    | '=' when index + 1 < Bytes.length source && Bytes.get source (index + 1) = '>' ->
      loop (index + 2) brace_depth true control_parentheses false
        brace_blocks true true
    | ';' ->
      loop (index + 1) brace_depth true control_parentheses false
        brace_blocks true false
    | ':' ->
      let statement = match brace_blocks with value :: _ -> value | [] -> true in
      loop (index + 1) brace_depth true control_parentheses false
        brace_blocks statement false
    | _ -> loop (index + 1) brace_depth true control_parentheses false
        brace_blocks false false
  in loop index (match stop_brace with Some value -> value | None -> 0)
    true [] false [] (stop_brace = None) false

and scan_js_template source language options accumulator start depth =
  let rec loop index =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-string" "unterminated JavaScript template literal"
        start index;
      index
    end else match Bytes.get source index with
    | '\\' -> loop (min (Bytes.length source) (index + 2))
    | '`' -> index + 1
    | '$' when index + 1 < Bytes.length source && Bytes.get source (index + 1) = '{' ->
      loop (scan_js_code source language options accumulator (index + 2) (Some 1) depth)
    | _ -> loop (index + 1)
  in loop (start + 1)

and scan_jsx_element source language options accumulator start depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit" "JSX lexical nesting limit exceeded" start start;
    Bytes.length source
  end else
  let rec loop index element_depth =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-jsx-element" "unterminated JSX element"
        start (Bytes.length source);
      Bytes.length source
    end else if Bytes.get source index = '{' then
      loop (scan_js_code source language options accumulator (index + 1) (Some 1) (depth + 1))
        element_depth
    else if Bytes.get source index <> '<' then loop (index + 1) element_depth
    else
      let closing = index + 1 < Bytes.length source && Bytes.get source (index + 1) = '/' in
      if not closing && not (jsx_open source index) then loop (index + 1) element_depth
      else
        let rec tag cursor quote =
          if cursor >= Bytes.length source then None else match quote with
          | Some active ->
            if Bytes.get source cursor = '\\' then tag (min (Bytes.length source) (cursor + 2)) quote
            else if Bytes.get source cursor = active then tag (cursor + 1) None
            else tag (cursor + 1) quote
          | None -> (match Bytes.get source cursor with
            | '\'' | '"' as character -> tag (cursor + 1) (Some character)
            | '{' when not closing ->
              tag (scan_js_code source language options accumulator (cursor + 1)
                (Some 1) (depth + 1)) None
            | '>' ->
              let rec previous value =
                if value > index && js_is_space (Bytes.get source (value - 1))
                then previous (value - 1) else value in
              let before = previous cursor in
              Some (cursor + 1, before > index && Bytes.get source (before - 1) = '/')
            | _ -> tag (cursor + 1) None)
        in
        match tag (index + if closing then 2 else 1) None with
        | None ->
          add_error accumulator "unterminated-jsx-tag" "unterminated JSX tag"
            index (Bytes.length source);
          Bytes.length source
        | Some (finish, self_closing) ->
          if closing then let new_depth = max 0 (element_depth - 1) in
            if new_depth = 0 then finish else loop finish new_depth
          else if not self_closing then loop finish (element_depth + 1)
          else if element_depth = 0 then finish else loop finish element_depth
  in loop start 0

(* NOTE: ECMA-262 12.5: a hashbang comment opens a Script or a Module and
   nothing else, and OComment reads "a Script" as "a file": a preamble is a
   preamble at absolute offset 0.  The embedded scan of a <script> element is
   handed a slice that begins at its own 0, so `offset` is what tells that slice
   from a file.  Without it a "#!" inside a page reads as the page's preamble,
   which no page has. *)
let scan_javascript ?(offset = 0) source language options accumulator =
  let start =
    if offset = 0 && starts source 0 "#!" then begin
      let finish = js_line_end source 2 in
      add_comment accumulator source language options Line 0 finish;
      finish
    end else 0 in
  ignore (scan_js_code source language options accumulator start None 0)

let ocaml_quoted_end source index =
  if Bytes.get source index <> '{' then None else
  match find_from source (index + 1) "|" with
  | Some pipe when
      let valid = ref true in
      for cursor = index + 1 to pipe - 1 do
        let character = Bytes.get source cursor in
        if not ((character >= 'a' && character <= 'z') || character = '_') then valid := false
      done;
      !valid ->
    let identifier = Bytes.sub_string source (index + 1) (pipe - index - 1) in
    let ending = "|" ^ identifier ^ "}" in
    (match find_from source (pipe + 1) ending with
    | Some finish -> Some (finish + String.length ending, true)
    | None -> Some (Bytes.length source, false))
  | _ -> None

(* NOTE: A character literal is an apostrophe, one character or one escape
   sequence, and a closing apostrophe (OCaml manual, Lexical conventions).  The
   shape is what decides it: the second apostrophe two bytes on, or a backslash
   and an apostrophe close enough behind it to close the longest escape there
   is.  Anything else leaves the apostrophe an ordinary byte -- of a type
   variable outside a comment, and of the comment's own text inside one, where
   the string that follows it still has to terminate. *)
(* INVARIANT: The rule `rust_char_start` states, for OCaml's two windows.  `'\`
   before a line terminator is an illegal backslash escape (`ocamlc` 5.5.0
   rejects it), so the escaped window gives up nothing valid by stopping there.
   The bare window costs the one shape OCaml does accept -- an apostrophe, a
   literal newline, an apostrophe -- which this scanner never read as a literal
   anyway: it ends a character literal at the line, so that shape used to be
   reported as an unterminated literal and is now simply not one. *)
let ocaml_char_start source index =
  let length = Bytes.length source in
  index + 1 < length &&
  let next = Bytes.get source (index + 1) in
  (not (is_line_terminator next)) &&
  ((index + 2 < length && Bytes.get source (index + 2) = '\'') ||
   (next = '\\' &&
     let rec has_quote cursor remaining =
       remaining > 0 && cursor < length
       && (not (is_line_terminator (Bytes.get source cursor)))
       && (Bytes.get source cursor = '\'' || has_quote (cursor + 1) (remaining - 1)) in
     has_quote (index + 2) 6))

let scan_ocaml source language options accumulator =
  let rec comment_end index depth =
    if index >= Bytes.length source then (index, false)
    else if starts source index "(*" then comment_end (index + 2) (depth + 1)
    else if starts source index "*)" then if depth = 1 then (index + 2, true) else comment_end (index + 2) (depth - 1)
    else if Bytes.get source index = '"' then
      let finish, _ = quoted_end source index true in comment_end finish depth
    else if Bytes.get source index = '{' then
      (match ocaml_quoted_end source index with
      | Some (finish, _) -> comment_end finish depth
      | None -> comment_end (index + 1) depth)
    else if Bytes.get source index = '\'' && ocaml_char_start source index then
      let finish, _ = quoted_end source index false in comment_end finish depth
    else comment_end (index + 1) depth in
  let rec loop index =
    if index >= Bytes.length source then ()
    else if starts source index "(*" then begin
      let finish, closed = comment_end (index + 2) 1 in
      add_comment accumulator source language options (if starts source index "(**" then DocBlock else Block) index finish;
      if not closed then add_error accumulator "unterminated-comment" "unterminated OCaml comment" index finish;
      loop finish
    end else if Bytes.get source index = '"' then begin
      let finish, closed = quoted_end source index true in
      if not closed then add_error accumulator "unterminated-string"
        "unterminated OCaml string" index finish;
      loop finish
    end
    else if Bytes.get source index = '\'' && ocaml_char_start source index then begin
      (* NOTE: A literal nothing closed ends at the line, not at the file: the
         rest of the source still holds comments, so the scan goes on from where
         the literal stopped rather than giving up on the file. *)
      let finish, closed = quoted_end source index false in
      if not closed then add_error accumulator "unterminated-string"
        "unterminated OCaml character literal" index finish;
      loop finish
    end
    else if Bytes.get source index = '{' then
      (match ocaml_quoted_end source index with
      | Some (finish, closed) ->
        if not closed then add_error accumulator "unterminated-string"
          "unterminated OCaml quoted string" index finish;
        loop finish
      | None -> loop (index + 1))
    else loop (index + 1)
  in loop 0

let python_string_start source index =
  let length = Bytes.length source in
  let quote cursor = cursor < length && (Bytes.get source cursor = '\'' || Bytes.get source cursor = '"') in
  if quote index then
    let character = Bytes.get source index in
    Some (index, starts source index (String.make 3 character), false, false)
  else if index < length && String.contains "rRuUbBfFtT" (Bytes.get source index) &&
    (index = 0 || let previous = Bytes.get source (index - 1) in
      not (((previous >= 'a' && previous <= 'z') || (previous >= 'A' && previous <= 'Z')) ||
        (previous >= '0' && previous <= '9') || previous = '_'))
  then
    let cursor = ref index in
    while !cursor < length && !cursor - index < 3 && String.contains "rRuUbBfFtT" (Bytes.get source !cursor) do incr cursor done;
    if quote !cursor then
      let prefix = lowercase (Bytes.sub_string source index (!cursor - index)) in
      Some (!cursor, starts source !cursor (String.make 3 (Bytes.get source !cursor)),
        String.contains prefix 'f' || String.contains prefix 't', String.contains prefix 'r')
    else None else None

let rec scan_python_delimited source accumulator token_start quote_start triple =
  let delimiter_length = if triple then 3 else 1 in
  let delimiter = Bytes.sub_string source quote_start delimiter_length in
  let rec loop index =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-string"
        (if triple then "unterminated Python triple-quoted string" else "unterminated Python string")
        token_start index;
      index
    end else if starts source index delimiter then index + delimiter_length
    else if Bytes.get source index = '\\' then loop (min (Bytes.length source) (index + 2))
    else if not triple && (Bytes.get source index = '\r' || Bytes.get source index = '\n') then begin
      add_error accumulator "unterminated-string" "unterminated Python string" token_start index;
      index
    end else loop (index + 1)
  in loop (quote_start + delimiter_length)

and scan_python_fstring source language options accumulator token_start quote_start triple _raw depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit" "Python f-string nesting limit exceeded"
      token_start token_start;
    Bytes.length source
  end else
  let delimiter_length = if triple then 3 else 1 in
  let delimiter = Bytes.sub_string source quote_start delimiter_length in
  let rec loop index =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-string" "unterminated Python f-string"
        token_start index;
      index
    end else if starts source index delimiter then index + delimiter_length
    else if Bytes.get source index = '\\' then loop (min (Bytes.length source) (index + 2))
    else if starts source index "{{" || starts source index "}}" then loop (index + 2)
    else if Bytes.get source index = '{' then
      loop (scan_python_expression source language options accumulator (index + 1) (depth + 1))
    else if not triple && (Bytes.get source index = '\r' || Bytes.get source index = '\n') then begin
      add_error accumulator "unterminated-string" "unterminated Python f-string"
        token_start index;
      index
    end else loop (index + 1)
  in loop (quote_start + delimiter_length)

and scan_python_expression source language options accumulator index depth =
  let rec loop index braces =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-fstring-expression"
        "unterminated Python f-string expression" index index;
      index
    end else if Bytes.get source index = '#' then begin
      let finish = line_end source (index + 1) in
      add_comment accumulator source language options Line index finish;
      loop finish braces
    end else match python_string_start source index with
    | Some (quote_start, triple, formatted, raw) ->
      let finish = if formatted then
        scan_python_fstring source language options accumulator index quote_start triple raw (depth + 1)
      else scan_python_delimited source accumulator index quote_start triple in
      loop finish braces
    | None -> (match Bytes.get source index with
      | '{' -> loop (index + 1) (braces + 1)
      | '}' -> let remaining = braces - 1 in
        if remaining = 0 then index + 1 else loop (index + 1) remaining
      | _ -> loop (index + 1) braces)
  in loop index 1

let scan_python source language options accumulator =
  let rec loop index =
    if index >= Bytes.length source then ()
    else if Bytes.get source index = '#' then let finish = line_end source (index + 1) in
      add_comment accumulator source language options Line index finish; loop finish
    else match python_string_start source index with
      | Some (quote_start, triple, formatted, raw) ->
        let finish = if formatted then
          scan_python_fstring source language options accumulator index quote_start triple raw 0
        else scan_python_delimited source accumulator index quote_start triple in
        loop finish
      | None -> loop (index + 1)
  in loop 0

let toml_quote_run source start quote =
  let rec loop index =
    if index < Bytes.length source && Bytes.get source index = quote then loop (index + 1)
    else index - start
  in loop start

(* NOTE: A basic string takes backslash escapes and a literal string takes none,
   so a backslash is a byte of a literal string (TOML v1.0.0, String). Three of
   either quote open the multi-line form, where a newline is content rather than
   the end of an unterminated string, and where the three-quote delimiter may
   absorb up to two content quotes in front of it. *)
let scan_toml_string source accumulator start =
  let quote = Bytes.get source start in
  let multiline = starts source start (String.make 3 quote) in
  let escapes = quote = '"' in
  let rec loop index =
    if index >= Bytes.length source then begin
      add_error accumulator "unterminated-string"
        (if multiline then "unterminated TOML multi-line string"
          else "unterminated TOML string") start index;
      index
    end else if escapes && Bytes.get source index = '\\' then
      loop (min (Bytes.length source) (index + 2))
    else if Bytes.get source index <> quote then begin
      if (not multiline) && (Bytes.get source index = '\r' || Bytes.get source index = '\n')
      then begin
        add_error accumulator "unterminated-string" "unterminated TOML string" start index;
        index
      end else loop (index + 1)
    end else if not multiline then index + 1
    else
      let run = toml_quote_run source index quote in
      if run >= 3 then index + min run 5 else loop (index + run)
  in loop (start + if multiline then 3 else 1)

let scan_toml source language options accumulator =
  let rec loop index =
    if index >= Bytes.length source then ()
    else match Bytes.get source index with
      | '#' -> let finish = line_end source (index + 1) in
        add_comment accumulator source language options Line index finish; loop finish
      | '"' | '\'' -> loop (scan_toml_string source accumulator index)
      | _ -> loop (index + 1)
  in loop 0

(* NOTE: An opening long bracket is '[', a run of '=', then '[', and the length
   of that run is its level (Lua 5.4 reference manual, 3.1). The second bracket
   is what tells "[[" from the two brackets of a[b[1]], so a bracket that never
   reaches it opens nothing at all. *)
let long_bracket_level source index =
  let length = Bytes.length source in
  if index >= length || Bytes.get source index <> '[' then None
  else
    let rec loop cursor =
      if cursor < length && Bytes.get source cursor = '=' then loop (cursor + 1)
      else if cursor < length && Bytes.get source cursor = '[' then Some (cursor - index - 1)
      else None
    in loop (index + 1)

(* NOTE: Long brackets do not nest, so the first close at the right level ends
   one and a run of the wrong length is content: a level-two bracket carries
   "]]" and "]=]" and ends only at "]==]". *)
let long_bracket_end source content level =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then (length, false)
    else if Bytes.get source index = ']' then
      let rec equals cursor =
        if cursor < length && Bytes.get source cursor = '=' then equals (cursor + 1) else cursor in
      let cursor = equals (index + 1) in
      if cursor - index - 1 = level && cursor < length && Bytes.get source cursor = ']'
      then (cursor + 1, true) else loop (index + 1)
    else loop (index + 1)
  in loop (min content length)

(* NOTE: What the \z escape skips is C's isspace in the default locale, which
   takes the vertical tab as well. *)
let lua_is_space = function
  | ' ' | '\t' | '\n' | '\011' | '\012' | '\r' -> true
  | _ -> false

(* NOTE: Lua counts "\r\n" and "\n\r" alike as one line (llex.c,
   inclinenumber), so a backslash in front of either escapes the whole pair. *)
let lua_newline_width source index =
  let length = Bytes.length source in
  if index >= length then None
  else match Bytes.get source index with
    | '\r' -> Some (if index + 1 < length && Bytes.get source (index + 1) = '\n' then 2 else 1)
    | '\n' -> Some (if index + 1 < length && Bytes.get source (index + 1) = '\r' then 2 else 1)
    | _ -> None

(* NOTE: "---" opens the documentation comment LDoc and the Lua language server
   read; a fourth dash makes an ordinary divider. *)
let lua_line_kind source index =
  if starts source index "---" && not (starts source index "----") then DocLine else Line

(* NOTE: The \z escape skips the whitespace that follows it, newlines included,
   and a backslash before a real line terminator carries that terminator into
   the string; any other unescaped terminator ends a string that was never
   closed. The remaining escapes carry neither a quote nor a newline, so
   skipping the byte after the backslash finds the same closing quote. *)
let scan_lua_short_string source accumulator start =
  let length = Bytes.length source in
  let quote = Bytes.get source start in
  let rec loop index =
    if index >= length then begin
      add_error accumulator "unterminated-string" "unterminated Lua string" start index;
      index
    end else if Bytes.get source index = '\\' then
      let escaped = index + 1 in
      if escaped < length && Bytes.get source escaped = 'z' then
        let rec space cursor =
          if cursor < length && lua_is_space (Bytes.get source cursor) then space (cursor + 1)
          else cursor in
        loop (space (escaped + 1))
      else (match lua_newline_width source escaped with
        | Some width -> loop (escaped + width)
        | None -> loop (min length (escaped + 1)))
    else if Bytes.get source index = quote then index + 1
    else (match lua_newline_width source index with
      | Some _ ->
        add_error accumulator "unterminated-string" "unterminated Lua string" start index;
        index
      | None -> loop (index + 1))
  in loop (start + 1)

(* NOTE: A long bracket immediately after the "--" opens a long comment, which
   runs to the closing bracket of its own level; anything else is a short
   comment to the end of the line, so "--[=" is one and "--[=[" is not. *)
let scan_lua_comment source language options accumulator start =
  match long_bracket_level source (start + 2) with
  | Some level ->
    let finish, closed = long_bracket_end source (start + 2 + level + 2) level in
    add_comment accumulator source language options Block start finish;
    if not closed then
      add_error accumulator "unterminated-comment" "unterminated Lua long comment" start finish;
    finish
  | None ->
    let finish = line_end source (start + 2) in
    add_comment accumulator source language options (lua_line_kind source start) start finish;
    finish

(* NOTE: a[b[1]] indexes twice and opens no string, so a bracket that is not a
   long one is one byte of the chunk. *)
let scan_lua_long_string source accumulator start =
  match long_bracket_level source start with
  | None -> start + 1
  | Some level ->
    let finish, closed = long_bracket_end source (start + level + 2) level in
    if not closed then
      add_error accumulator "unterminated-string" "unterminated Lua long string" start finish;
    finish

(* NOTE: The loader skips a first line that opens with '#' before it lexes
   anything (lauxlib.c, skipcomment), which is what lets a chunk carry a "#!"
   line. It is that one byte at that one offset: '#' is the length operator
   everywhere else. *)
let scan_lua source language options accumulator =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then ()
    else if starts source index "--" then
      loop (scan_lua_comment source language options accumulator index)
    else match Bytes.get source index with
      | '"' | '\'' -> loop (scan_lua_short_string source accumulator index)
      | '[' -> loop (scan_lua_long_string source accumulator index)
      | _ -> loop (index + 1)
  in
  (* NOTE: `skipcomment` (lauxlib.c) calls `skipBOM` before it tests the first
     byte for '#', so a chunk behind a byte order mark still has its first line
     skipped.  It is that one byte at that one offset: '#' is the length
     operator everywhere else. *)
  let preamble = byte_order_mark_width source in
  if length > preamble && Bytes.get source preamble = '#' then begin
    let finish = line_end source (preamble + 1) in
    add_comment accumulator source language options Line preamble finish;
    loop finish
  end else loop 0

(* NOTE: "///" documents the declaration under it and "//!" the container the
   file is (Zig Language Reference, Doc comments), which std.zig.Tokenizer tags
   doc_comment and container_doc_comment.  A fourth slash takes the first back:
   .doc_comment_start falls to .line_comment when it meets one, so "////" is an
   ordinary comment and only exactly three slashes document anything.  "//!!"
   stays a top-level doc comment, because the tokenizer decides that one at the
   "!" and reads no further. *)
let zig_line_kind source index =
  if starts source index "////" then Line
  else if starts source index "///" || starts source index "//!" then DocLine
  else Line

(* NOTE: A Zig string and a Zig character literal are one rule: .string_literal
   and .char_literal of std.zig.Tokenizer differ only in the quote that closes
   them.  A backslash carries the next byte into the literal, and a real line
   terminator ends neither -- the tokenizer marks the token invalid at it -- so
   a quote that never closes is reported at the line break rather than
   swallowing the lines below it.  A backslash in front of that terminator does
   not carry it either, for the same reason. *)
let scan_zig_quoted source accumulator start =
  let length = Bytes.length source in
  let quote = Bytes.get source start in
  let message =
    if quote = '"' then "unterminated Zig string"
    else "unterminated Zig character literal" in
  let carries index =
    Bytes.get source index = '\\' && index + 1 < length &&
    Bytes.get source (index + 1) <> '\r' && Bytes.get source (index + 1) <> '\n' in
  let rec loop index =
    if index >= length then begin
      add_error accumulator "unterminated-string" message start index;
      index
    end else if carries index then loop (index + 2)
    else if Bytes.get source index = quote then index + 1
    else if Bytes.get source index = '\r' || Bytes.get source index = '\n' then begin
      add_error accumulator "unterminated-string" message start index;
      index
    end else loop (index + 1)
  in loop (start + 1)

(* NOTE: Zig has no block comment at all -- "/*" is the division operator and
   then multiplication, which std.zig.Tokenizer reports as slash and asterisk --
   so this is its own small lexer rather than scan_slash with one delimiter
   taken away.  Everything it has to know ends at a line break: a comment runs
   to the end of its line, a quoted literal may not cross one, and a multiline
   string literal is one line of content at a time.

   "\\\\" is the whole opener of such a line, and the tokenizer takes it wherever
   a token may begin rather than only as the first thing on a line, so that the
   backslashes of "const b = \\\\text" open one just as an indented pair does.
   Everything to the end of the line is content, and the next line starts in
   code again, so consecutive lines are separate tokens the parser joins.  A
   single backslash is an invalid token to Zig and an ordinary byte here:
   nothing it could open is a state a comment can hide in.  "@\"quoted\"" needs
   no rule of its own either -- the "@" is an ordinary byte and what follows it
   is lexed as the string literal it is spelled as. *)
let scan_zig source language options accumulator =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then ()
    else if starts source index "//" then begin
      let finish = line_end source (index + 2) in
      add_comment accumulator source language options (zig_line_kind source index) index finish;
      loop finish
    end
    else if starts source index "\\\\" then loop (line_end source (index + 2))
    else match Bytes.get source index with
      | '"' | '\'' -> loop (scan_zig_quoted source accumulator index)
      | _ -> loop (index + 1)
  in loop 0

(* NOTE: R's parser has one comment token and calls every "#" line a COMMENT
   (measured on R 4.3.3: utils::getParseData gives "#' doc" and "# line" the same
   token name).  "#'" is roxygen2's marker for the prose it turns into a manual
   page, so it is documentation here for the reason Lua's "---" and Zig's "///"
   are: the tool that reads it is what makes it one.  Nothing takes the marker
   back the way a fourth slash does in Zig, so the test is the two bytes and no
   more. *)
let r_line_kind source index = if starts source index "#'" then DocLine else Line

(* NOTE: Whether a byte may continue an R name, and so cannot be followed by the
   "r" that opens a raw string.  SymbolValue (gram.y) reads a name while the
   bytes are alphanumeric, "." or "_", and it is entered on a multi-byte
   character as well, so every byte with the high bit set counts here.  Counting
   one that does not only refuses the raw reading, which falls back to an
   ordinary string and hides more rather than less. *)
let is_r_name_byte character =
  (character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
  (character >= '0' && character <= '9') || character = '.' || character = '_' ||
  Char.code character >= 0x80

(* NOTE: The end of the R raw string whose quote is at the given index, and
   whether it closed -- or None when no raw string opens there at all.  The
   literal is "r" or "R", the quote, a run of dashes, and one of "(", "[" or
   "{"; it closes on the matching bracket, the same run of dashes, and the same
   quote (?Quotes; R 4.0.0 and later).  R puts no limit on the dash run -- 100
   dashes were measured accepted on R 4.3.3 -- so it is copied out of the source
   rather than counted twice.  The "r" opens the literal only where it begins a
   token: "xr\"(a)\"" is the name "xr" and then an ordinary string, which is
   what R's lexer reads there too. *)
let r_raw_string source quote =
  let length = Bytes.length source in
  if quote = 0 then None
  else
    let prefix = quote - 1 in
    let opener = Bytes.get source prefix in
    if opener <> 'r' && opener <> 'R' then None
    else if prefix > 0 && is_r_name_byte (Bytes.get source (prefix - 1)) then None
    else
      let rec dashes index =
        if index < length && Bytes.get source index = '-' then dashes (index + 1) else index in
      let bracket = dashes (quote + 1) in
      if bracket >= length then None
      else
        let closing = match Bytes.get source bracket with
          | '(' -> Some ')' | '[' -> Some ']' | '{' -> Some '}' | _ -> None in
        match closing with
        | None -> None
        | Some closing ->
          let close =
            String.make 1 closing ^
            Bytes.sub_string source (quote + 1) (bracket - quote - 1) ^
            String.make 1 (Bytes.get source quote) in
          Some (match find_from source (bracket + 1) close with
            | Some relative -> (relative + String.length close, true)
            | None -> (length, false))

(* NOTE: The end of the R literal that runs to the next unescaped delimiter, and
   whether that delimiter was there at all.  One function for the two quoted
   strings and the backquoted name, because R lexes all three the same way: a
   backslash carries the next byte in -- a line break included, which is why a
   literal that never closes runs to the end of the file rather than to the end
   of its line -- and nothing but the delimiter ends them. *)
let r_delimited_end source start close =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then (length, false)
    else match Bytes.get source index with
      | '\\' -> loop (min (index + 2) length)
      | character when character = close -> (index + 1, true)
      | _ -> loop (index + 1)
  in loop start

(* NOTE: One R script (R Language Definition, 10 Parser; ?Quotes).  "#" opens a
   comment that runs to the end of the line and that is the whole comment
   grammar -- there is no block form and no nesting.  What makes this more than
   a search for "#" is the four literals that carry one as content: a quoted
   string, a raw string, a backquoted name, and the "%...%" operator.

   Three of the four may cross a line break.  The fourth may not: SpecialValue in
   gram.y pushes every byte up to the next "%" into the operator's name and
   returns ERROR at a line break instead, so the name may hold a "#", a quote or
   a backquote, takes no escapes, and an unterminated one is reported where it is
   rather than swallowing the rest of the file.

   Where the bytes look like a raw string and are not one -- "r\"<a>\"", whose
   delimiter is not a bracket -- R refuses the file outright with "malformed raw
   string literal".  Falling back to the ordinary reading is what happens here
   instead: it is the same fallback the C++ raw string takes, and it hides the
   "#" behind a quote rather than exposing it in a file no interpreter would
   run.

   Measured against the interpreter over the 42 R files the R 4.3.3 distribution
   ships: every one of the 1,330 comments utils::getParseData reports as a
   COMMENT token comes back with the same byte span, and no file is called
   invalid. *)
let scan_r source language options accumulator =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then ()
    else match Bytes.get source index with
      | '#' ->
        let finish = line_end source (index + 1) in
        add_comment accumulator source language options (r_line_kind source index) index finish;
        loop finish
      | '"' | '\'' ->
        let raw = r_raw_string source index in
        (match raw with
         | Some (finish, closed) ->
           if not closed then
             add_error accumulator "unterminated-string" "unterminated R raw string"
               (index - 1) finish;
           loop finish
         | None ->
           let finish, closed = r_delimited_end source (index + 1) (Bytes.get source index) in
           if not closed then
             add_error accumulator "unterminated-string" "unterminated R string" index finish;
           loop finish)
      | '`' ->
        let finish, closed = r_delimited_end source (index + 1) '`' in
        if not closed then
          add_error accumulator "unterminated-identifier" "unterminated R backquoted name"
            index finish;
        loop finish
      | '%' ->
        let stop = line_end source (index + 1) in
        let rec percent cursor =
          if cursor >= stop then None
          else if Bytes.get source cursor = '%' then Some cursor
          else percent (cursor + 1) in
        (match percent (index + 1) with
         | Some closing -> loop (closing + 1)
         | None ->
           add_error accumulator "unterminated-operator" "unterminated R special operator"
             index stop;
           loop stop)
      | _ -> loop (index + 1)
  in loop 0

(* NOTE: "tokenizeSingleLineComment" (_fe_analyzer_shared,
   src/scanner/abstract_scanner.dart) reads the byte behind "//" and sets
   dartdoc when it is a third slash, then reads no further, so a fourth slash
   leaves "////" a DartDocToken just as "///" is one -- which is where Dart
   parts company with Lua's "----" and Zig's "////".  "//!" is Rust's inner-doc
   marker and means nothing here. *)
let dart_line_kind source index = if starts source index "///" then DocLine else Line

(* NOTE: "tokenizeMultiLineComment" sets dartdoc from the single byte behind
   "/*", so "/**" opens the documentation comment dart doc reads and "/**/" is
   an empty one.  "/*!" is Doxygen's marker, which C and C++ honour and Dart
   does not. *)
let dart_block_kind source index = if starts source index "/**" then DocBlock else Block

let dart_unterminated_string raw triple =
  match raw, triple with
  | true, true -> "unterminated Dart raw multiline string"
  | true, false -> "unterminated Dart raw string"
  | false, true -> "unterminated Dart multiline string"
  | false, false -> "unterminated Dart string"

(* NOTE: A Dart identifier is spelled with ASCII letters, digits, "_" and "$"
   and nothing wider (Dart Language Specification, 17.4). *)
let dart_identifier_continue = function
  | 'a'..'z' | 'A'..'Z' | '0'..'9' | '_' | '$' -> true
  | _ -> false

(* NOTE: "tokenizeRawStringKeywordOrIdentifier" is reached from the scanner's
   main switch, so the "r" has to begin a token: an "r" that continues an
   identifier is a letter of that identifier, and the quote behind it opens an
   ordinary string.  Only a lower-case "r" does it -- "R'x'" is the identifier
   "R" and then a string.

   What decides it is the run of identifier bytes ending just before the "r".
   An empty run means nothing precedes it.  A run that begins with a letter,
   "_" or "$" is an identifier the "r" continues.  A run that begins with a
   digit is a number, and a number token always ends before an "r" -- "r" is
   not a digit, a hex digit, "x", "e", "." or "_" -- so the "r" begins a token
   there too.  Measured on Dart 3.13.2: "1r'x'" and "0x1r'x'" are a number and
   then a raw string, while "xr'x'", "_r'x'" and "$r'x'" are one identifier and
   then an ordinary one. *)
let dart_raw_string_prefix source quote =
  quote > 0 && Bytes.get source (quote - 1) = 'r' &&
  let rec loop cursor =
    if cursor > 0 && dart_identifier_continue (Bytes.get source (cursor - 1)) then loop (cursor - 1)
    else cursor in
  let start = loop (quote - 1) in
  start = quote - 1 || (match Bytes.get source start with '0'..'9' -> true | _ -> false)

(* NOTE: One Dart string beginning at its opening quote (Dart Language
   Specification, 17.6 Strings).  The six forms are one rule with two switches:
   "triple" is three of the same quote, which makes a line break content instead
   of the end of the literal, and "raw" is the "r" in front of it, which takes
   away both the backslash escape and "${" interpolation.

   A backslash in an ordinary string carries the next byte in -- that is what
   hides a "\'" -- but it does not carry a line terminator: tokenizeSingleLineString
   leaves .string_literal at the break either way, so "'x\<newline>y'" is an
   unterminated string rather than a continuation.  The closing delimiter is the
   first unescaped run of it and not the last: "''''x''''" is "'''" then "'x"
   then "'''" with one quote left over, which is what the Dart scanner reports
   for those bytes. *)
let rec scan_dart_string source language options accumulator quote depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit"
      "Dart string interpolation nesting limit exceeded" quote quote;
    Bytes.length source
  end else
  let length = Bytes.length source in
  let raw = dart_raw_string_prefix source quote in
  let start = if raw then quote - 1 else quote in
  let character = Bytes.get source quote in
  let triple = quote + 2 < length &&
    Bytes.get source (quote + 1) = character && Bytes.get source (quote + 2) = character in
  let width = if triple then 3 else 1 in
  let delimiter = String.make width character in
  let carries index =
    (not raw) && Bytes.get source index = '\\' && index + 1 < length &&
    Bytes.get source (index + 1) <> '\r' && Bytes.get source (index + 1) <> '\n' in
  let rec loop index =
    if index >= length then begin
      add_error accumulator "unterminated-string" (dart_unterminated_string raw triple)
        start index;
      index
    end else if starts source index delimiter then index + width
    else if carries index then loop (index + 2)
    else if (not raw) && starts source index "${" then
      loop (scan_dart_interpolation source language options accumulator (index + 2) (depth + 1))
    else if (not triple) && (Bytes.get source index = '\r' || Bytes.get source index = '\n')
    then begin
      add_error accumulator "unterminated-string" (dart_unterminated_string raw triple)
        start index;
      index
    end else loop (index + 1)
  in loop (quote + width)

(* NOTE: One "${ ... }" interpolation, beginning past its "${".  The braces of
   the expression are counted rather than searched for, because the expression
   is code: a nested string, a map literal, and a comment may all stand inside
   one.  A comment written there is a comment -- the Dart scanner attaches it to
   the token that follows, exactly as it does outside a string -- and a "//" one
   runs to the end of its line while the string it sits inside carries on
   below. *)
and scan_dart_interpolation source language options accumulator index depth =
  if depth > 256 then begin
    add_error accumulator "nesting-limit"
      "Dart string interpolation nesting limit exceeded" index index;
    Bytes.length source
  end else
  let length = Bytes.length source in
  let rec loop index braces =
    if index >= length then begin
      add_error accumulator "unterminated-template-expression"
        "unterminated Dart string interpolation" index index;
      index
    end else if starts source index "//" then begin
      let finish = line_end source (index + 2) in
      add_comment accumulator source language options (dart_line_kind source index) index finish;
      loop finish braces
    end else if starts source index "/*" then begin
      let finish, closed = block_end source index true in
      add_comment accumulator source language options (dart_block_kind source index) index finish;
      if not closed then add_error accumulator "unterminated-comment"
        "unterminated Dart block comment" index finish;
      loop finish braces
    end else match Bytes.get source index with
      | '"' | '\'' ->
        loop (scan_dart_string source language options accumulator index (depth + 1)) braces
      | '{' -> loop (index + 1) (braces + 1)
      | '}' -> let remaining = braces - 1 in
        if remaining = 0 then index + 1 else loop (index + 1) remaining
      | _ -> loop (index + 1) braces
  in loop index 1

(* NOTE: One Dart compilation unit (Dart Language Specification, 17.1 Comments).
   Dart is a C-family syntax with three departures that decide the shape of this
   scanner rather than of scan_slash: its block comment nests, so "/* /* */ */"
   is one comment; "//!" and "/*!" document nothing while "////" still does; and
   "#!" at the very first byte is the script tag, which tokenizeTag reads only
   when scanOffset = 0 -- a comment on the line above one is enough to take that
   away.  "#" everywhere else is the operator that opens a symbol literal,
   "#foo" or "#+", and needs no rule of its own.

   Ground truth for every rule here is scanString of package:_fe_analyzer_shared
   as the Dart SDK 3.13.2 ships it, read for token kinds and offsets, with
   dart analyze for acceptance.  Measured against that scanner over the 3,143
   .dart files of the SDK's own lib/ and of the packages fetched beside it: all
   147,988 comments it reports come back with the same byte span, and no file is
   called invalid. *)
let scan_dart source language options accumulator =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then ()
    else if index = 0 && starts source index "#!" then begin
      let finish = line_end source (index + 2) in
      add_comment accumulator source language options Line index finish;
      loop finish
    end
    else if starts source index "//" then begin
      let finish = line_end source (index + 2) in
      add_comment accumulator source language options (dart_line_kind source index) index finish;
      loop finish
    end
    else if starts source index "/*" then begin
      let finish, closed = block_end source index true in
      add_comment accumulator source language options (dart_block_kind source index) index finish;
      if not closed then add_error accumulator "unterminated-comment"
        "unterminated Dart block comment" index finish;
      loop finish
    end
    else match Bytes.get source index with
      | '"' | '\'' -> loop (scan_dart_string source language options accumulator index 0)
      | _ -> loop (index + 1)
  in loop 0

type heredoc = { operator : int; delimiter : bytes; strip_tabs : bool }

let consume_newline source index =
  if starts source index "\r\n" then index + 2 else index + 1

(* NOTE: A quote opens a scalar only where a scalar may begin: at the start of a
   line, behind white space, or behind one of the flow indicators "," "[" "{"
   (YAML 1.2.2, 7.4).  Anywhere else it is content of the plain scalar it sits
   in, which is what keeps the apostrophe of "it's" from opening a literal that
   would swallow the rest of the file. *)
let yaml_flow_opener source index =
  index > 0 && (match Bytes.get source (index - 1) with
    | ',' | '[' | '{' -> true
    | _ -> false)

(* NOTE: Which trailing line breaks a block scalar keeps (YAML 1.2.2, 8.1.1.2):
   "-" drops the final break and every empty line behind it, no indicator keeps
   the final break alone, and "+" makes both content -- which is what lets a
   blank line under such a body change its value. *)
type chomping = Chomp_strip | Chomp_clip | Chomp_keep

(* NOTE: A block scalar header is its "|" or ">", then its indicators, then
   white space, then at most a comment, and then the end of the line (YAML
   1.2.2, 8.1.1).  Anything else leaves the indicator a byte of a plain scalar,
   which is the whole of what tells "key: >" from "key: a > b".  The comment
   needs that white space in front of it like any other (6.6), so "key: |#c" is
   no header either.  The answer carries the explicit indentation indicator --
   None where the header spells none out and the body detects its own -- the
   chomping indicator, where a comment begins, and where the line ends.  The two
   readings of a missing indicator are not one answer: the floor a body line has
   to clear is the owner's column either way, and an absent indicator behaves as
   1 for that, but the depth the body's content sits at is written on the header
   only when the indicator is. *)
let yaml_block_header source index =
  let length = Bytes.length source in
  let rec indicators cursor indentation chomping =
    if cursor >= length then (cursor, indentation, chomping)
    else match Bytes.get source cursor with
      | '1' .. '9' when indentation = None ->
        indicators (cursor + 1)
          (Some (Char.code (Bytes.get source cursor) - Char.code '0')) chomping
      | '+' when chomping = None -> indicators (cursor + 1) indentation (Some Chomp_keep)
      | '-' when chomping = None -> indicators (cursor + 1) indentation (Some Chomp_strip)
      | _ -> (cursor, indentation, chomping) in
  let cursor, indentation, chomping = indicators (index + 1) None None in
  let rec white cursor spaced =
    if cursor < length && (Bytes.get source cursor = ' ' || Bytes.get source cursor = '\t')
    then white (cursor + 1) true else (cursor, spaced) in
  let cursor, spaced = white cursor false in
  let comment =
    if spaced && cursor < length && Bytes.get source cursor = '#' then Some cursor else None in
  let cursor = match comment with Some start -> line_end source start | None -> cursor in
  if cursor >= length || Bytes.get source cursor = '\r' || Bytes.get source cursor = '\n'
  then Some (indentation,
             (match chomping with Some value -> value | None -> Chomp_clip),
             comment, cursor)
  else None

(* NOTE: Where the node property beginning at `index` ends.  An anchor "&name"
   and a tag "!tag" run to the white space, the line, or the flow indicator that
   ends them (YAML 1.2.2, 6.9 and 7.4); nothing else may close one, which is
   what keeps "!!str" a single token. *)
let yaml_property_end source index =
  let length = Bytes.length source in
  let rec loop cursor =
    if cursor >= length then cursor
    else match Bytes.get source cursor with
      | ' ' | '\t' | '\r' | '\n' | ',' | '[' | ']' | '{' | '}' -> cursor
      | _ -> loop (cursor + 1)
  in loop (index + 1)

(* NOTE: Indentation is spaces alone: a tab may not indent a line (YAML 1.2.2,
   6.1), so the first one ends the indentation and is content of whatever
   follows it.  A line of nothing but white space is empty even so, which is
   what keeps a blank line inside a block scalar body from ending it. *)
let yaml_line_shape source start =
  let length = Bytes.length source in
  let rec spaces index =
    if index < length && Bytes.get source index = ' ' then spaces (index + 1) else index in
  let content = spaces start in
  let rec white index =
    if index < length && (Bytes.get source index = ' ' || Bytes.get source index = '\t')
    then white (index + 1) else index in
  let cursor = white content in
  let blank =
    cursor >= length || Bytes.get source cursor = '\r' || Bytes.get source cursor = '\n' in
  (content - start, blank, line_end source cursor)

(* NOTE: "---" and "..." are read in column zero alone, which is what the line
   start carries here: a line with any indentation at all begins with a space
   and matches neither (YAML 1.2.2, 9.1.2 and 9.1.3). *)
let yaml_document_marker source line_start =
  (starts source line_start "---" || starts source line_start "...") &&
  (line_start + 3 >= Bytes.length source ||
   match Bytes.get source (line_start + 3) with
   | ' ' | '\t' | '\r' | '\n' -> true
   | _ -> false)

(* NOTE: A line belongs to the body of a block scalar while it is empty -- an
   empty line is content of the scalar whatever its indentation (YAML 1.2.2,
   8.1.2) -- or indented to at least the content indentation.  The first line
   that is neither ends it, and so does a document marker in column zero, which
   is what ends the body of a scalar that is the whole document and therefore
   has no parent indentation to fall short of.

   The second half of the answer is the detected content indentation (8.1.1.1):
   the indentation of the first non-empty line, which is what a parser measures
   every later line against, and `body_min` when the body holds no non-empty
   line to measure.  It is never less than `body_min`, because a line shallower
   than that would have ended the body instead of opening it. *)
let yaml_block_body_end source header_end body_min =
  let length = Bytes.length source in
  if header_end >= length then (length, body_min)
  else
    let rec loop index content =
      if index >= length then (index, content)
      else
        let indent, blank, finish = yaml_line_shape source index in
        if (not blank) && (indent < body_min || yaml_document_marker source index) then (index, content)
        else
          let content = if blank || content <> None then content else Some indent in
          if finish >= length then (length, content)
          else loop (consume_newline source finish) content
    in
    let finish, content = loop (consume_newline source header_end) None in
    (finish, match content with Some value -> value | None -> body_min)

let rec only_blanks source index finish =
  index >= finish ||
  ((match Bytes.get source index with ' ' | '\t' -> true | _ -> false) &&
   only_blanks source (index + 1) finish)

(* NOTE: The one comment that is all its line holds, as an index into the array.
   None when the line holds none, holds one with something else on it, or holds
   a comment that does not run to the end of the line -- in each of those the
   line survives a removal whatever else is decided about it. *)
let comment_alone_on_line source comments line_start line_finish =
  let rec loop index =
    if index >= Array.length comments then None
    else
      let span = (comments.(index) : comment).span in
      if span.start >= line_finish then None
      else if span.start < line_start then loop (index + 1)
      else if span.finish = line_finish && only_blanks source line_start span.start
      then Some index else None
  in loop 0

(* NOTE: Where a line under a block scalar ends once its terminator is taken with
   it. *)
let past_terminator source line_finish =
  if line_finish >= Bytes.length source then line_finish
  else consume_newline source line_finish

(* NOTE: The keep reason the scanner writes for a comment a YAML block scalar
   leans on, and the one keep no option can overrule.  Frozen: the differential
   protocol compares this string byte for byte. *)
let yaml_structural_trail = "structural in a YAML block scalar trail"

(* NOTE: Which comments in the trails of `blocks` no removal may take, as indices
   into `comments`.

   A block scalar body ends at the first line under it that is shallower than its
   content (YAML 1.2.2, 8.1.1), and in a trail of whole-line comments that line
   is a comment.  Removing it -- and a removal there takes the whole line, which
   is the least a removal can leave -- hands the lines under it back to the body,
   and a line that reaches the content depth is content again.  When what comes
   back up is a comment the run keeps, no removal preserves the value: the
   comment above it is not commentary but the thing that closes the scalar, and
   it is kept.

   Only the first comment of a trail can do that work, and it always can.  The
   line a body ended at is shallower than the floor and so shallower than the
   content, so keeping it closes the scalar there and leaves everything below
   outside -- which is why one keep per block is both necessary and enough, and
   why the deeper comments of the trail stay removable.  Keeping a deeper one
   instead would be no fix at all: it reaches the content depth itself, so the
   body would swallow the survivor.

   A trail whose every comment is removable needs none of this: with nothing left
   standing under the body there is nothing for it to take back. *)
let yaml_structural_trail_keeps source blocks comments =
  let length = Bytes.length source in
  let keeps = ref [] in
  List.iter (fun block ->
    (* INVARIANT: `shield` is the trail's first removable comment shallower than
       the content -- the one keep that would close the body -- and is set before
       any deeper line can be reached, because the line a body ends at is
       shallower than the content by construction. *)
    let rec loop probe shield =
      if probe >= length then ()
      else
        let indent, blank, finish = yaml_line_shape source probe in
        if blank then
          (* NOTE: An empty line is content of the body above whatever its
             indentation (8.1.1.2), so it neither ends the trail nor shields
             anything under it. *)
          loop (past_terminator source finish) shield
        else
          match comment_alone_on_line source comments probe finish with
          (* NOTE: The first line with anything else on it is the next node, and
             it is not a line any removal here can move. *)
          | None -> ()
          | Some found ->
            if (comments.(found) : comment).disposition = Remove then
              let shield =
                if shield = None && indent < block.content_indent then Some found else shield in
              loop (past_terminator source finish) shield
            else if indent < block.content_indent then
              (* NOTE: A surviving line shallower than the content closes the
                 body on its own, so nothing above it is load-bearing. *)
              ()
            else match shield with
              | Some index -> keeps := index :: !keeps
              | None -> ()
    in loop block.body_end None) blocks;
  !keeps

(* NOTE: A double-quoted scalar takes backslash escapes (YAML 1.2.2, 7.3.1) and
   a single-quoted one takes none, where "''" is the one way to write a quote of
   its own (7.3.2), so a backslash inside the second is a byte of it.  Both fold
   over a line break, which leaves the end of the file the only thing that can
   leave one unterminated. *)
let scan_yaml_quoted source accumulator start =
  let length = Bytes.length source in
  let quote = Bytes.get source start in
  let rec loop index =
    if index >= length then begin
      add_error accumulator "unterminated-string"
        (if quote = '"' then "unterminated YAML double-quoted scalar"
          else "unterminated YAML single-quoted scalar") start index;
      index
    end
    else if quote = '"' && Bytes.get source index = '\\' then loop (min length (index + 2))
    else if Bytes.get source index <> quote then loop (index + 1)
    else if quote = '\'' && index + 1 < length && Bytes.get source (index + 1) = '\''
    then loop (index + 2)
    else index + 1
  in loop (start + 1)

(* NOTE: One YAML stream (YAML 1.2.2).  The scanner is line-local: everything it
   needs to decide what a byte is comes from the line that byte sits on.  "#"
   opens a comment only where white space separates it from the token in front
   of it (6.6), the two quoted styles may run over a line break and carry every
   "#" inside them as content, and a block scalar (8.1) swallows every following
   line more indented than the node it hangs off.

   "separated" is whether a "#" here would be separated from what precedes it;
   "node_start" is whether a node may begin here, which is what tells the block
   scalar indicator of "key: >" from the ">" inside the plain scalar
   "key: a > b"; "token_column" is where the token being read began, so that a
   ": " behind it can name the column its value hangs off; and "owner_column" is
   that column once one is known.  The first three are reset by the line break;
   "owner_column" survives it while the node it names is still owed one, because
   the "|" of a block scalar may sit on the line under the "key:" or the "-"
   that owns it, or behind node properties (6.9).  The Rust scanner carries two
   more answers -- whether a block scalar body ended at the start of a line, and
   that a line under a live carry offers no restart point -- which is
   bookkeeping for the incremental checkpoints this reference has no engine for.

   "valid" is a lexical answer here and nothing more.  YAML has shapes a lexer
   cannot rule out and a parser rejects, and removing a comment can walk a file
   from one to the other: a comment line inside a multi-line plain scalar is a
   parse error while it is there, and taking it away leaves a scalar that parses
   and folds the two halves into one value. *)
let scan_yaml source language options accumulator =
  let length = Bytes.length source in
  let boundary index =
    index >= length ||
    (match Bytes.get source index with ' ' | '\t' | '\r' | '\n' -> true | _ -> false) in
  let rec loop index line_start separated node_start token_column owner_column =
    if index >= length then ()
    else
      let byte = Bytes.get source index in
      if byte = '#' && separated then begin
        let finish = line_end source (index + 1) in
        add_comment accumulator source language options Line index finish;
        loop finish line_start separated node_start token_column owner_column
      end
      else if byte = '\r' || byte = '\n' then
        (* NOTE: A line that ends while a node is still owed -- "key:", a bare
           "-", a property whose node has not come yet, and the blank and comment
           lines a separation may hold (6.9, 8.2.2) -- hands the owner's
           indentation to the line below, because the "|" of the block scalar it
           introduces may be down there.  A line that put a node on itself hands
           over nothing. *)
        let next = consume_newline source index in
        loop next next true true None (if node_start then owner_column else None)
      else if byte = ' ' || byte = '\t' then
        loop (index + 1) line_start true node_start token_column owner_column
      else if (byte = '|' || byte = '>') && node_start && separated then
        (match yaml_block_header source index with
        | Some (indicator, chomping, comment, header_end) ->
          (match comment with
          | Some start -> add_comment accumulator source language options Line start header_end
          | None -> ());
          (* NOTE: The body is indented past the node the scalar hangs off
             (8.1.1.1).  For "key: |" that node is the mapping, whose indentation
             is the column of the key; for "- |" it is the sequence, whose
             indentation is the column of the "-".  The header itself may sit
             anywhere past that owner -- on a line of its own, or behind an
             anchor or a tag -- so its own column says nothing about how deep a
             body line has to be, and reading it as the floor would take a body
             indented less than the header for the end of the scalar and its "#"
             lines for comments.  With no owner at all the scalar is the whole
             document, whose indentation is one short of column zero, which
             leaves every line under it body.  An explicit indentation indicator
             counts from that same owner, which is why it replaces the detected
             depth rather than adding to it.  Detection proper reads the first
             non-empty line instead, and a line shallower than that but still
             past the owner is content of neither reading; taking it for body is
             the one that leaves bytes alone. *)
          let base = match owner_column with Some column -> column + 1 | None -> 0 in
          let floor = base + (match indicator with Some value -> value | None -> 1) - 1 in
          let finish, detected = yaml_block_body_end source header_end floor in
          (* NOTE: Where this body stopped, on what terms it keeps its trailing
             empty lines, and how deep its content is are the whole of what
             `lines_a_removal_must_swallow` and `yaml_structural_trail_keeps`
             need from a scan: the lines under a body are the only place in YAML
             where the hole a removal leaves carries meaning.  Recorded here
             rather than re-derived, because only the scan knows the column of
             the node the header hangs off.  An explicit indicator is the content
             depth (8.1.1.1); without one the depth is detected from the first
             non-empty line, and a body with no non-empty line at all has none to
             detect, so the floor stands in for it -- which is the depth the next
             line the scalar could take would set. *)
          accumulator.yaml_blocks_rev <-
            { body_end = finish;
              content_indent = (match indicator with Some _ -> floor | None -> detected);
              keeps_empties = chomping = Chomp_keep }
            :: accumulator.yaml_blocks_rev;
          loop finish finish true true None None
        | None ->
          let token_column =
            match token_column with Some _ -> token_column | None -> Some (index - line_start) in
          loop (index + 1) line_start false false token_column owner_column)
      (* NOTE: An anchor "&name" and a tag "!tag" are node properties (6.9): they
         stand in front of the node they decorate rather than being one, so a
         node may still begin after them.  That is what leaves the "|" of
         "key: !!str |" a block scalar header instead of a byte of a plain
         scalar.  A property belongs to the node it decorates, so it is that
         node's first token and names the column a ": " behind it hangs off. *)
      else if (byte = '!' || byte = '&') && node_start && separated then
        let token_column =
          match token_column with Some _ -> token_column | None -> Some (index - line_start) in
        loop (yaml_property_end source index) line_start false node_start token_column owner_column
      else if (byte = '"' || byte = '\'') && (separated || yaml_flow_opener source index) then
        let token_column =
          match token_column with
          | Some _ -> token_column
          | None -> if node_start then Some (index - line_start) else None in
        let finish = scan_yaml_quoted source accumulator index in
        loop finish line_start false false token_column owner_column
      (* NOTE: "-" is a sequence entry and "?" an explicit key only when white
         space or the line ends them (6.9 and 8.2): "-x" is a plain scalar, and
         so is "?x".  Either leaves the position a node may begin at, one column
         further in. *)
      else if (byte = '-' || byte = '?') && node_start && boundary (index + 1) then
        loop (index + 1) line_start false true None (Some (index - line_start))
      (* NOTE: A ":" ends a key only where white space or the line follows it
         (7.2), which is what leaves the ":" of "http://x" inside the plain
         scalar it belongs to. *)
      else if byte = ':' && boundary (index + 1) then
        let owner = match token_column with Some _ -> token_column | None -> owner_column in
        loop (index + 1) line_start false true None owner
      else
        let token_column =
          match token_column with
          | Some _ -> token_column
          | None -> if node_start then Some (index - line_start) else None in
        loop (index + 1) line_start false false token_column owner_column
  in
  loop 0 0 true true None None;
  (* NOTE: One rule needs the whole file rather than the byte in front of it, so
     it runs once the trails are all there to read: a comment that is the only
     thing holding a block scalar out of the kept comment under it is not
     commentary and is kept. *)
  if accumulator.yaml_blocks_rev <> [] then begin
    let comments = Array.of_list (List.rev accumulator.comments_rev) in
    let blocks = List.rev accumulator.yaml_blocks_rev in
    List.iter (fun index ->
      comments.(index) <- { (comments.(index)) with disposition = Keep yaml_structural_trail })
      (yaml_structural_trail_keeps source blocks comments);
    accumulator.comments_rev <- List.rev (Array.to_list comments)
  end

let parse_heredoc source index =
  let strip_tabs = index + 2 < Bytes.length source && Bytes.get source (index + 2) = '-' in
  let cursor = ref (index + if strip_tabs then 3 else 2) in
  while !cursor < Bytes.length source && ascii_whitespace (Bytes.get source !cursor) &&
    Bytes.get source !cursor <> '\r' && Bytes.get source !cursor <> '\n' do incr cursor done;
  let delimiter = Buffer.create 16 in
  let quote = ref None and saw_word = ref false and invalid = ref false in
  let escaped in_double =
    if !cursor + 1 >= Bytes.length source then invalid := true
    else begin
      let value = Bytes.get source (!cursor + 1) in
      if value = '\r' && !cursor + 2 < Bytes.length source &&
        Bytes.get source (!cursor + 2) = '\n'
      then cursor := !cursor + 3
      else begin
        if value <> '\r' && value <> '\n' then begin
          if in_double && not (String.contains "$`\"\\" value)
          then Buffer.add_char delimiter '\\';
          Buffer.add_char delimiter value
        end;
        cursor := !cursor + 2
      end
    end in
  while !cursor < Bytes.length source && not !invalid &&
    (match !quote with
    | Some _ -> true
    (* NOTE: The delimiter is a word (POSIX Shell Command Language, 2.7.4), and
       a word ends at a blank or at an unquoted operator character. *)
    | None -> let byte = Bytes.get source !cursor in
      not (ascii_whitespace byte) && not (String.contains ";|&()<>" byte))
  do
    let byte = Bytes.get source !cursor in
    match !quote with
    | Some active when byte = active -> quote := None; incr cursor
    | Some '"' when byte = '\\' -> escaped true
    | Some _ -> Buffer.add_char delimiter byte; incr cursor
    | None when byte = '\'' || byte = '"' ->
      saw_word := true; quote := Some byte; incr cursor
    | None when byte = '\\' -> saw_word := true; escaped false
    | None -> saw_word := true; Buffer.add_char delimiter byte; incr cursor
  done;
  if not !saw_word || !invalid || !quote <> None then None
  else Some ({ operator = index; delimiter = Buffer.contents delimiter |> Bytes.of_string;
    strip_tabs }, !cursor)

let heredoc_body_end source start heredoc =
  let rec loop index =
    if index > Bytes.length source then None else
    let finish = line_end source index in
    let line_start = if heredoc.strip_tabs then
      let cursor = ref index in
      while !cursor < finish && Bytes.get source !cursor = '\t' do incr cursor done;
      !cursor else index in
    let line = Bytes.sub source line_start (finish - line_start) in
    if line = heredoc.delimiter then
      Some (if finish < Bytes.length source then consume_newline source finish else finish)
    else if finish = Bytes.length source then None
    else loop (consume_newline source finish)
  in loop start

let shell_quote_end source start closing escapes =
  let rec loop index =
    if index >= Bytes.length source then (index, false)
    else if escapes && Bytes.get source index = '\\' then
      loop (min (Bytes.length source) (index + 2))
    else if Bytes.get source index = closing then (index + 1, true)
    else loop (index + 1)
  in loop (start + 1)

type shell_terminator = ShellParenthesis of int | ShellBacktick of int
type shell_case_state = CaseAwaitIn | CasePattern | CaseBody

let scan_shell source language options accumulator =
  let rec consume_bodies index = function
    | [] -> Some index
    | heredoc :: tail -> (match heredoc_body_end source index heredoc with
      | Some finish -> consume_bodies finish tail
      | None ->
        add_error accumulator "unterminated-heredoc" "unterminated shell heredoc"
          heredoc.operator (Bytes.length source);
        None)
  in
  let rec region index terminator depth =
    if depth > 256 then begin
      add_error accumulator "nesting-limit" "shell lexical nesting limit exceeded" index index;
      Bytes.length source
    end else
    let initial_parentheses = match terminator with Some (ShellParenthesis _) -> 1 | _ -> 0 in
    let case_states = ref [] and command_position = ref true in
    let pattern_expected () = match !case_states with
      | CasePattern :: _ -> true | _ -> false in
    let case_body () = match !case_states with
      | CaseBody :: _ -> true | _ -> false in
    let rec loop index heredocs parentheses word_open =
      if index >= Bytes.length source then begin
        (match terminator with
        | Some (ShellParenthesis start) -> add_error accumulator
            "unterminated-command-substitution" "unterminated shell command substitution"
            start index
        | Some (ShellBacktick start) -> add_error accumulator
            "unterminated-string" "unterminated shell command substitution" start index
        | None -> ());
        index
      end else if (match terminator with Some (ShellBacktick _) -> true | _ -> false) &&
        Bytes.get source index = '`' then index + 1
      else if Bytes.get source index = '#' && not word_open then begin
      let finish = line_end source (index + 1) in
      add_comment accumulator source language options Line index finish;
      loop finish heredocs parentheses false
    end else if Bytes.get source index = '\'' then begin
      let finish, closed = shell_quote_end source index '\'' false in
      if not closed then add_error accumulator "unterminated-string"
        "unterminated shell single quote" index finish;
      command_position := false;
      loop finish heredocs parentheses true
    end else if Bytes.get source index = '"' then begin
      command_position := false;
      loop (double_quote index (depth + 1)) heredocs parentheses true
    end else if Bytes.get source index = '`' then begin
      command_position := false;
      loop (region (index + 1) (Some (ShellBacktick index)) (depth + 1))
        heredocs parentheses true
    end else if starts source index "$(" then begin
      command_position := false;
      loop (region (index + 2) (Some (ShellParenthesis index)) (depth + 1))
        heredocs parentheses true
    end else if starts source index "$'" &&
      (options.dialect = Bash53 || options.dialect = Zsh) then begin
      let finish, closed = shell_quote_end source (index + 1) '\'' true in
      if not closed then add_error accumulator "unterminated-string"
        "unterminated shell ANSI-C quoted string" (index + 1) finish;
      command_position := false;
      loop finish heredocs parentheses true
    end else if starts source index "<<<" then
      loop (index + 3) heredocs parentheses false
    else if starts source index "<<" then (match parse_heredoc source index with
      | Some (heredoc, finish) ->
        loop finish (heredoc :: heredocs) parentheses true
      | None -> loop (index + 1) heredocs parentheses false)
    else if (Bytes.get source index = '\r' || Bytes.get source index = '\n') && heredocs <> [] then
      (command_position := not (pattern_expected ());
      match consume_bodies (consume_newline source index) (List.rev heredocs) with
      | Some finish -> loop finish [] parentheses false | None -> Bytes.length source)
    else if Bytes.get source index = '\r' || Bytes.get source index = '\n' then begin
      command_position := not (pattern_expected ());
      loop (consume_newline source index) heredocs parentheses false
    end
    else if (match terminator with Some (ShellParenthesis _) -> true | _ -> false) &&
      Bytes.get source index = '(' then begin
      command_position := not (pattern_expected ());
      loop (index + 1) heredocs (parentheses + 1) false
    end
    else if (match terminator with Some (ShellParenthesis _) -> true | _ -> false) &&
      Bytes.get source index = ')' then begin
      if parentheses = 1 && pattern_expected () then begin
        (match !case_states with
        | CasePattern :: tail -> case_states := CaseBody :: tail
        | _ -> ());
        command_position := true;
        loop (index + 1) heredocs parentheses false
      end else
        let remaining = max 0 (parentheses - 1) in
        if remaining = 0 then index + 1
        else begin
          command_position := true;
          loop (index + 1) heredocs remaining false
        end
    end
    else if Bytes.get source index = ')' && pattern_expected () then begin
      (match !case_states with
      | CasePattern :: tail -> case_states := CaseBody :: tail
      | _ -> ());
      command_position := true;
      loop (index + 1) heredocs parentheses false
    end
    else if Bytes.get source index = ';' && case_body () &&
      (starts source index ";;" || starts source index ";&") then begin
      let width = if starts source index ";;&" then 3 else 2 in
      (match !case_states with
      | CaseBody :: tail -> case_states := CasePattern :: tail
      | _ -> ());
      command_position := false;
      loop (index + width) heredocs parentheses false
    end
    else if String.contains ";&|()" (Bytes.get source index) then begin
      command_position :=
        Bytes.get source index <> '|' || not (pattern_expected ());
      loop (index + 1) heredocs parentheses false
    end
    else if String.contains "<>" (Bytes.get source index) then
      loop (index + 1) heredocs parentheses false
    else if ascii_whitespace (Bytes.get source index) then
      loop (index + 1) heredocs parentheses false
    else if Bytes.get source index = '\\' then
      if starts source index "\\\r\n" then
        loop (index + 3) heredocs parentheses word_open
      else if index + 1 < Bytes.length source &&
        (Bytes.get source (index + 1) = '\r' || Bytes.get source (index + 1) = '\n')
      then loop (index + 2) heredocs parentheses word_open
      else begin
        command_position := false;
        loop (min (Bytes.length source) (index + 2)) heredocs parentheses true
      end
    else if not word_open &&
      let character = Bytes.get source index in
      (character >= 'a' && character <= 'z') ||
      (character >= 'A' && character <= 'Z') || character = '_'
    then begin
      let rec word_end cursor =
        if cursor < Bytes.length source then
          let character = Bytes.get source cursor in
          if (character >= 'a' && character <= 'z') ||
            (character >= 'A' && character <= 'Z') ||
            (character >= '0' && character <= '9') || character = '_'
          then word_end (cursor + 1) else cursor
        else cursor in
      let finish = word_end (index + 1) in
      let boundary = finish = Bytes.length source ||
        let character = Bytes.get source finish in
        ascii_whitespace character || String.contains ";&|()<>" character in
      let token = Bytes.sub_string source index (finish - index) in
      if boundary && token = "case" && !command_position then begin
        case_states := CaseAwaitIn :: !case_states;
        command_position := false
      end else if boundary && token = "in" &&
        (match !case_states with CaseAwaitIn :: _ -> true | _ -> false)
      then begin
        (match !case_states with
        | CaseAwaitIn :: tail -> case_states := CasePattern :: tail
        | _ -> ());
        command_position := false
      end else if boundary && token = "esac" &&
        (!command_position || pattern_expected ())
      then begin
        (match !case_states with _ :: tail -> case_states := tail | [] -> ());
        command_position := false
      end else
        command_position := !command_position &&
          finish < Bytes.length source && Bytes.get source finish = '=';
      loop finish heredocs parentheses true
    end
    else begin
      command_position := false;
      loop (index + 1) heredocs parentheses true
    end
    in loop index [] initial_parentheses false
  and double_quote start depth =
    let rec loop index =
      if index >= Bytes.length source then begin
        add_error accumulator "unterminated-string" "unterminated shell double quote"
          start index;
        index
      end else if Bytes.get source index = '\\' then
        loop (min (Bytes.length source) (index + 2))
      else if Bytes.get source index = '"' then index + 1
      else if starts source index "$(" then
        loop (region (index + 2) (Some (ShellParenthesis index)) (depth + 1))
      else if Bytes.get source index = '`' then
        loop (region (index + 1) (Some (ShellBacktick index)) (depth + 1))
      else loop (index + 1)
    in loop (start + 1)
  in ignore (region 0 None 0)

let scan_sql source language options accumulator =
  let rec quoted_end_sql quote backslash_escapes index =
    if index >= Bytes.length source then (index, false)
    else if Bytes.get source index = quote && index + 1 < Bytes.length source &&
      Bytes.get source (index + 1) = quote then
      quoted_end_sql quote backslash_escapes (index + 2)
    else if Bytes.get source index = quote then (index + 1, true)
    else if backslash_escapes && Bytes.get source index = '\\' then
      quoted_end_sql quote backslash_escapes (min (Bytes.length source) (index + 2))
    else quoted_end_sql quote backslash_escapes (index + 1) in
  let identifier_end start closing =
    let close = if Bytes.get source start = '[' then ']' else closing in
    let rec loop index =
      if index >= Bytes.length source then (index, false)
      else if Bytes.get source index = close && index + 1 < Bytes.length source &&
        Bytes.get source (index + 1) = close then loop (index + 2)
      else if Bytes.get source index = close then (index + 1, true)
      else loop (index + 1) in
    loop (start + 1) in
  let dollar_quote_end start =
    match find_from source (start + 1) "$" with
    | None -> None
    | Some second ->
      let valid =
        let result = ref true in
        if second > start + 1 then begin
          let first = Bytes.get source (start + 1) in
          if not ((first >= 'a' && first <= 'z') || (first >= 'A' && first <= 'Z') ||
            first = '_') then result := false
        end;
        for index = start + 2 to second - 1 do
          let character = Bytes.get source index in
          if not ((character >= 'a' && character <= 'z') ||
            (character >= 'A' && character <= 'Z') ||
            (character >= '0' && character <= '9') || character = '_') then result := false
        done;
        !result in
      if not valid then None else
      let delimiter = Bytes.sub_string source start (second - start + 1) in
      Some (match find_from source (second + 1) delimiter with
        | Some finish -> (finish + String.length delimiter, true)
        | None -> (Bytes.length source, false)) in
  let oracle_quote_end start =
    if start + 2 >= Bytes.length source || Bytes.get source (start + 1) <> '\'' then None
    else let opening = Bytes.get source (start + 2) in
      let closing = match opening with '[' -> ']' | '{' -> '}' | '(' -> ')' | '<' -> '>'
        | other -> other in
      let token = String.make 1 closing ^ "'" in
      Some (match find_from source (start + 3) token with
        | Some finish -> (finish + 2, true)
        | None -> (Bytes.length source, false)) in
  let mysql_dash_boundary index =
    index + 2 >= Bytes.length source ||
    let code = Char.code (Bytes.get source (index + 2)) in
    code <= 32 || code = 127 in
  let postgres_escape_string_start quote =
    quote > 0 && (Bytes.get source (quote - 1) = 'e' || Bytes.get source (quote - 1) = 'E') &&
    (quote = 1 || let previous = Bytes.get source (quote - 2) in
      not ((previous >= 'a' && previous <= 'z') || (previous >= 'A' && previous <= 'Z') ||
        (previous >= '0' && previous <= '9') || previous = '_' || previous = '$')) in
  let rec loop index =
    if index >= Bytes.length source then ()
    else if starts source index "--" &&
      (options.dialect <> MySql || mysql_dash_boundary index) then
      let finish = line_end source (index + 2) in
      add_comment accumulator source language options Line index finish; loop finish
    else if options.dialect = MySql && Bytes.get source index = '#' then let finish = line_end source (index + 1) in add_comment accumulator source language options Line index finish; loop finish
    else if starts source index "/*" then let finish, closed = block_end source index
      (options.dialect = PostgreSql || options.dialect = TSql) in
      add_comment accumulator source language options Block index finish;
      if not closed then add_error accumulator "unterminated-comment" "unterminated SQL block comment" index finish; loop finish
    else if Bytes.get source index = '\'' then
      let backslash_escapes = options.dialect = MySql ||
        (options.dialect = PostgreSql && postgres_escape_string_start index) in
      let finish, closed = quoted_end_sql '\'' backslash_escapes (index + 1) in
      if not closed then add_error accumulator "unterminated-string" "unterminated SQL string" index finish; loop finish
    else if Bytes.get source index = '"' || Bytes.get source index = '`' then
      let mysql_string = Bytes.get source index = '"' && options.dialect = MySql in
      let finish, closed = if mysql_string then quoted_end_sql '"' true (index + 1)
        else identifier_end index (Bytes.get source index) in
      if not closed then add_error accumulator
        (if mysql_string then "unterminated-string" else "unterminated-identifier")
        (if mysql_string then "unterminated MySQL quoted string"
          else "unterminated SQL quoted identifier") index finish;
      loop finish
    else if Bytes.get source index = '[' && options.dialect = TSql then
      let finish, closed = identifier_end index ']' in
      if not closed then add_error accumulator "unterminated-identifier"
        "unterminated T-SQL bracket identifier" index finish;
      loop finish
    else if Bytes.get source index = '$' && options.dialect = PostgreSql then
      (match dollar_quote_end index with
      | Some (finish, closed) ->
        if not closed then add_error accumulator "unterminated-string"
          "unterminated PostgreSQL dollar-quoted string" index finish;
        loop finish
      | None -> loop (index + 1))
    else if (Bytes.get source index = 'q' || Bytes.get source index = 'Q') &&
      options.dialect = Oracle then
      (match oracle_quote_end index with
      | Some (finish, closed) ->
        if not closed then add_error accumulator "unterminated-string"
          "unterminated Oracle q-quoted string" index finish;
        loop finish
      | None -> loop (index + 1))
    else loop (index + 1)
  in loop 0

let ascii_case_starts source index token =
  index + String.length token <= Bytes.length source &&
  lowercase (Bytes.sub_string source index (String.length token)) = lowercase token

let ascii_case_find source index token =
  let rec loop cursor =
    if cursor + String.length token > Bytes.length source then None
    else if ascii_case_starts source cursor token then Some cursor else loop (cursor + 1)
  in loop index

(* NOTE: An opening tag is "<?php" without regard to case, followed by white
   space or the end of the file (zend_language_scanner.l:
   "<?php"([ \t]|{NEWLINE})), so "<?phpinfo()" is inline text; "<?=" is the
   short echo tag and needs nothing behind it.  A bare "<?" opens nothing at
   all, because short_open_tag is off by default, which is what leaves "<?xml"
   an XML declaration in the output rather than the start of a program. *)
let php_open_tag source index =
  let length = Bytes.length source in
  if starts source index "<?=" then Some (index + 3)
  else if ascii_case_starts source index "<?php" &&
    (index + 5 >= length ||
     match Bytes.get source (index + 5) with
     | ' ' | '\t' | '\r' | '\n' -> true
     | _ -> false)
  then Some (index + 5) else None

(* NOTE: A PHP "//" or "#" comment ends at the line break or at a closing tag,
   whichever comes first (PHP manual, Comments -- "the closing tag breaks out of
   PHP mode").  The "?>" is not part of the comment. *)
let php_line_comment_end source index =
  let length = Bytes.length source in
  let rec loop cursor =
    if cursor >= length then cursor
    else match Bytes.get source cursor with
      | '\r' | '\n' -> cursor
      | _ -> if starts source cursor "?>" then cursor else loop (cursor + 1)
  in loop index

(* NOTE: The tokenizer makes a documentation comment of "/**" only when white
   space follows it -- its rule is "/*"|"/**"{WHITESPACE}, and the longer
   alternative is what sets T_DOC_COMMENT -- so "/**/" and "/**text*/" are
   ordinary block comments.  "/*!" is Doxygen's marker and means nothing to
   PHP's own tooling, so it is an ordinary comment too. *)
let php_block_kind source index =
  if starts source index "/**" && index + 3 < Bytes.length source &&
    (match Bytes.get source (index + 3) with
     | ' ' | '\t' | '\r' | '\n' -> true
     | _ -> false)
  then DocBlock else Block

(* NOTE: The complex syntax "{$...}" holds a PHP expression, which the engine
   lexes as ordinary code.  This balances its braces instead, skipping over the
   two things inside one that can carry a brace of their own -- a nested string
   and a comment -- so "{$a['}']}" ends where PHP ends it.  Nothing else in an
   expression can.  What it does not do is report the comment it skipped:
   reading one out of a string would mean running the whole lexer inside one,
   and v1 leaves those bytes alone instead. *)
let php_interpolation_end source brace =
  let length = Bytes.length source in
  let rec literal index quote =
    if index >= length || Bytes.get source index = quote then index
    else if Bytes.get source index = '\\' then literal (min length (index + 2)) quote
    else literal (index + 1) quote in
  let rec loop index depth =
    if index >= length then length
    else match Bytes.get source index with
      | '{' -> loop (index + 1) (depth + 1)
      | '}' -> if depth = 1 then index + 1 else loop (index + 1) (depth - 1)
      | ('\'' | '"' | '`') as quote -> loop (min length (literal (index + 1) quote + 1)) depth
      | '/' when starts source index "/*" ->
        let finish, _ = block_end source index false in loop finish depth
      | '/' when starts source index "//" ->
        loop (php_line_comment_end source (index + 2)) depth
      | '#' when not (starts source index "#[") ->
        loop (php_line_comment_end source (index + 1)) depth
      | _ -> loop (index + 1) depth
  in loop (brace + 1) 1

(* NOTE: A PHP label opens with a letter, "_", or any byte from 0x80 up, and
   continues with those and the digits (PHP manual, Variables). *)
let php_label_start character =
  (character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
  character = '_' || Char.code character >= 0x80

let php_label_continue character =
  php_label_start character || (character >= '0' && character <= '9')

(* NOTE: The header is "<<<", blanks, the label -- bare, or quoted with "'" for
   a nowdoc or with a double quote for a heredoc -- and then the line break,
   with nothing else allowed in between (zend_language_scanner.l).  The body
   begins on the next line.  Anything the grammar refuses opened nothing, which
   is the conservative reading of "$a <<< 1". *)
let php_heredoc_header source start =
  let length = Bytes.length source in
  let rec blanks cursor =
    if cursor < length && (Bytes.get source cursor = ' ' || Bytes.get source cursor = '\t')
    then blanks (cursor + 1) else cursor in
  let opened = blanks (start + 3) in
  let quote =
    if opened < length && (Bytes.get source opened = '\'' || Bytes.get source opened = '"')
    then Some (Bytes.get source opened) else None in
  let label_start = match quote with Some _ -> opened + 1 | None -> opened in
  if label_start >= length || not (php_label_start (Bytes.get source label_start)) then None
  else
    let rec label cursor =
      if cursor < length && php_label_continue (Bytes.get source cursor)
      then label (cursor + 1) else cursor in
    let label_end = label label_start in
    let name = Bytes.sub_string source label_start (label_end - label_start) in
    let after = match quote with
      | None -> Some label_end
      | Some quote ->
        if label_end < length && Bytes.get source label_end = quote
        then Some (label_end + 1) else None in
    match after with
    | None -> None
    | Some after ->
      if after < length && (Bytes.get source after = '\r' || Bytes.get source after = '\n')
      then Some (name, consume_newline source after, quote = Some '\'')
      else None

(* NOTE: Since PHP 7.3 the closing label may be indented by blanks and may be
   followed by anything that cannot continue a label -- ";", ",", ")", an
   operator, the line break, or the end of the file (PHP manual, Heredoc text).
   A byte that can continue one leaves the line ordinary body, which is what
   keeps "EOTX" from ending an "EOT". *)
let php_heredoc_end source body name =
  let length = Bytes.length source in
  let width = String.length name in
  let rec line index =
    let rec blanks cursor =
      if cursor < length && (Bytes.get source cursor = ' ' || Bytes.get source cursor = '\t')
      then blanks (cursor + 1) else cursor in
    let cursor = blanks index in
    if starts source cursor name &&
      (cursor + width >= length || not (php_label_continue (Bytes.get source (cursor + width))))
    then Some (cursor + width)
    else
      let finish = line_end source index in
      if finish >= length then None else line (consume_newline source finish)
  in line body

(* NOTE: A single-quoted string escapes only "\'" and "\\", and every other
   backslash is a byte of it -- but the byte after a backslash can never be the
   closing quote unless the pair is that escape, so skipping two finds the same
   closer either way.  A double-quoted or backtick string takes the full escape
   set and interpolates.  None of the three ends at a line break, so only the
   end of the file leaves one unterminated. *)
let scan_php_quoted source accumulator start =
  let length = Bytes.length source in
  let quote = Bytes.get source start in
  let interpolates = quote <> '\'' in
  let rec loop index =
    if index >= length then begin
      add_error accumulator "unterminated-string"
        (match quote with
         | '\'' -> "unterminated PHP single-quoted string"
         | '"' -> "unterminated PHP double-quoted string"
         | _ -> "unterminated PHP backtick string") start index;
      index
    end
    else if Bytes.get source index = '\\' then loop (min length (index + 2))
    else if Bytes.get source index = quote then index + 1
    else if interpolates && Bytes.get source index = '{' &&
      index + 1 < length && Bytes.get source (index + 1) = '$'
    then loop (php_interpolation_end source index)
    else if interpolates && Bytes.get source index = '$' &&
      index + 1 < length && Bytes.get source (index + 1) = '{'
    then loop (php_interpolation_end source (index + 1))
    else loop (index + 1)
  in loop (start + 1)

let scan_php_heredoc source accumulator start =
  match php_heredoc_header source start with
  | None -> start + 1
  | Some (name, body, nowdoc) ->
    (match php_heredoc_end source body name with
     | Some finish -> finish
     | None ->
       add_error accumulator "unterminated-string"
         (if nowdoc then "unterminated PHP nowdoc" else "unterminated PHP heredoc")
         start (Bytes.length source);
       Bytes.length source)

(* NOTE: PHP mode, from the byte after the opening tag that entered it.  The
   answer is where inline HTML resumes: past a "?>" and the one line break it
   carries away with it (zend_language_scanner.l: "?>"{NEWLINE}?), which is what
   keeps a template from emitting a blank line for every block of code it holds,
   or the end of the file.  PHP 8.0 gave "#[" to attributes, so a "#" with a
   bracket behind it opens no comment. *)
let scan_php_code source language options accumulator start =
  let length = Bytes.length source in
  let rec loop index =
    if index >= length then index
    else if starts source index "?>" then
      let finish = index + 2 in
      if finish < length && (Bytes.get source finish = '\r' || Bytes.get source finish = '\n')
      then consume_newline source finish else finish
    else if starts source index "//" then
      let finish = php_line_comment_end source (index + 2) in
      add_comment accumulator source language options Line index finish;
      loop finish
    else if starts source index "/*" then begin
      let finish, closed = block_end source index false in
      add_comment accumulator source language options (php_block_kind source index) index finish;
      if not closed then
        add_error accumulator "unterminated-comment" "unterminated PHP block comment" index finish;
      loop finish
    end
    else if starts source index "#[" then loop (index + 1)
    else if Bytes.get source index = '#' then
      let finish = php_line_comment_end source (index + 1) in
      add_comment accumulator source language options Line index finish;
      loop finish
    else match Bytes.get source index with
      | '\'' | '"' | '`' -> loop (scan_php_quoted source accumulator index)
      | '<' when starts source index "<<<" ->
        loop (scan_php_heredoc source accumulator index)
      | _ -> loop (index + 1)
  in loop start

(* NOTE: One PHP file (PHP manual, Basic syntax, Comments, Strings, Heredoc
   text).  A file opens in inline-HTML mode, where every byte is output verbatim
   and nothing is a comment; an opening tag enters PHP mode and "?>" returns.
   Inline HTML is opaque in v1, so an HTML comment in a PHP file is not
   reported: reading it would mean scanning the inline halves as HTML, which is
   a change of what the language is rather than a missing arm here.

   The CLI strips a "#!" line from the very first line of a script before the
   engine sees it (php_cli.c, which tests the first two bytes).  Unlike CPython
   and Lua, PHP skips no byte order mark first, and neither does the kernel, so
   a mark in front of the "#!" leaves it ordinary inline HTML.  The Rust scanner
   carries one more answer out of the closing tag -- whether it ended at the
   start of a line -- which is bookkeeping for the incremental checkpoints this
   reference has no engine for. *)
let scan_php source language options accumulator =
  let length = Bytes.length source in
  let rec html index =
    if index >= length then ()
    else match Bytes.get source index with
      | '<' -> (match php_open_tag source index with
        | Some code -> html (scan_php_code source language options accumulator code)
        | None -> html (index + 1))
      | '\r' | '\n' -> html (consume_newline source index)
      | _ -> html (index + 1)
  in
  if starts source 0 "#!" then begin
    let finish = line_end source 2 in
    add_comment accumulator source language options Line 0 finish;
    html finish
  end else html 0

(* NOTE: Where a Ruby token may begin, which is what decides whether "/", "%",
   "?" and "<<" open a literal or are the operator spelled with the same byte.
   This is Ruby's own lex_state folded onto the three answers those four
   questions read out of it: IS_BEG(), IS_END(), and the IS_ARG() in between,
   where a bare word may be a method about to take a command argument and only
   the spacing around the byte says which.  Ruby's lexer tells a local variable
   from a method name by the symbol table it is building, which a scanner has
   not got, so every bare word lands in RubyArgument. *)
(* NOTE: RubyFname is EXPR_FNAME|EXPR_FITEM, where "alias" and "undef" leave
   Ruby.  It answers every question RubyEnd answers, and one differently:
   parse_percent opens a symbol literal on "%s" there, spacing or none. *)
type ruby_state = RubyBegin | RubyArgument | RubyEnd | RubyFname

type ruby_percent = {
  percent_form : char;
  percent_open : char;
  percent_close : char;
  percent_content : int;
  percent_interpolates : bool;
}

type ruby_heredoc = {
  heredoc_operator : int;
  heredoc_label : string;
  heredoc_indented : bool;
  heredoc_interpolates : bool;
}

(* NOTE: Ruby's is_identchar: a letter, "_", or the lead byte of a character
   outside ASCII, which Ruby takes as a name byte wholesale. *)
let ruby_identifier_start character =
  (character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
  character = '_' || Char.code character land 0x80 <> 0

let ruby_identifier_continue character =
  ruby_identifier_start character || (character >= '0' && character <= '9')

let ruby_digit character = character >= '0' && character <= '9'

let ruby_alphanumeric character =
  (character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
  ruby_digit character

(* NOTE: White space that separates Ruby tokens without ending a line.  The
   vertical tab and the form feed are in it, as rb_isspace has them; the two
   line terminators are handled on their own, because they finish a statement. *)
let ruby_is_space = function ' ' | '\t' | '\011' | '\012' -> true | _ -> false

let ruby_identifier_end source index =
  let rec loop cursor =
    if cursor < Bytes.length source && ruby_identifier_continue (Bytes.get source cursor)
    then loop (cursor + 1) else cursor
  in loop index

(* NOTE: Ruby's lexer takes a trailing "?" or "!" into a name unless a "="
   follows it, which is what tells "x.empty?" from the ternary "x ? y : z" --
   and, in the other direction, keeps "a != b" a comparison. *)
let ruby_word_end source index =
  let finish = ruby_identifier_end source (index + 1) in
  if finish < Bytes.length source &&
    (Bytes.get source finish = '?' || Bytes.get source finish = '!') &&
    not (finish + 1 < Bytes.length source && Bytes.get source (finish + 1) = '=')
  then finish + 1 else finish

(* NOTE: The digits, the "_" separators, the radix letters and the "r" and "i"
   suffixes are one run of name bytes; a "." joins the run only when a digit
   follows it, which is what keeps "1.times" a method call. *)
let ruby_number_end source index =
  let rec loop cursor =
    let finish = ruby_identifier_end source cursor in
    if finish + 1 < Bytes.length source && Bytes.get source finish = '.' &&
      ruby_digit (Bytes.get source (finish + 1))
    then loop (finish + 1) else finish
  in loop index

(* NOTE: Ruby's keyword table folded onto ruby_state.  "def", "alias" and
   "undef" refuse a literal because the name that follows one may be spelled "/"
   or "%" -- "def /(other)" defines division; "class" and "module" are in the
   first list for the mirror-image reason, that "class <<self" is a singleton
   class and never a here document.  "alias" and "undef" take RubyFname rather
   than RubyEnd because they leave Ruby in EXPR_FNAME|EXPR_FITEM, which is one
   answer wider.  "super", "yield", "not" and "defined?" are in none of the
   three, which leaves them where Ruby has them: a command that may take an
   argument.

   RubyEnd is a coarser answer than either reason asked for, and it is worth
   naming what the coarseness costs.  What is measured is this table against
   Ripper.lex, keyword by keyword: RubyEnd answers all four of the state
   machine's questions at once, so it also decides "/", "%" and "?", and two
   readings are where the answer it gives is not Ruby's.  After "def" it is
   Ruby's own answer for "/" and for every percent literal, which EXPR_FNAME
   reads as the method names they are, and "def" has no exception: "def%s(foo)"
   is on_op "%" then on_ident "s" to Ruby 3.3.12.  After "class" and "module" it
   is not Ruby's answer either: EXPR_CLASS expects a value, so Ruby opens a
   literal there -- "class /x # c/" is one regular expression to Ruby 3.3.12
   (Ripper.lex gives on_regexp_beg) and a division with a comment behind it
   here.  And "?" diverges after all five: "class ?# x" and "def ?# x" are
   on_CHAR "?#" to Ruby and a comment opener here.  Both of those are spellings
   Ruby itself refuses -- "class" and "module" take a constant, a "::" or a "<<"
   in any program that parses, and "def ?# x" is a syntax error -- so both are
   reachable only where the scan is already reading a broken file, and buying
   them back with a state of their own, one that keeps the literal readings and
   refuses only the here document, would add a state to the machine for no
   program that runs.

   RubyFname is the state that does earn its keep, and "%s" after "alias" or
   "undef" is what buys it: "alias%s(baz # x) %s(bar)" is a file Ruby runs, and
   Ripper.lex under Ruby 3.3.12 gives on_symbeg "%s(" in state FNAME|FITEM for
   both names with "baz # x" an on_tstring_content.  Reading that "#" as a
   comment would remove bytes Ruby has inside a symbol, which is the one
   direction this scanner may not take.  The rule is about the delimiter and not
   only the state -- "alias" goes on refusing "/", "<<", "%w" and "%q" in the
   same breath it accepts "%s" ("alias%w[a]" is on_op "%" to Ripper) -- so
   ruby_percent_opens asks it rather than ruby_literal_opens alone.

   "<<" itself is not an entry on that list, and what keeps it off is not
   this function.  A header RubyEnd refuses is a shift, which reads no fewer
   bytes into a literal than Ruby does; a header it allows queues a body, and
   scan_ruby queues it for the physical line the header stands on wherever that
   header was written -- before an interpolation, inside one, inside a nested
   one, or inside an interpolation on another here document's body line.  A
   queue that stopped at an interpolation boundary would read a whole here
   document body as code, which would be one more reading that takes fewer bytes
   into a literal than Ruby does, and it is the one the corpus cases named
   "ruby-heredoc-*-interpolation" hold shut.

   NOTE: RubyEnd is as far as that first half reaches, and "def", "alias" and
   "undef" are where it stops.  MRI's parser_yylex tries heredoc_identifier on
   "<<" unless the lexer state is EXPR_DOT|EXPR_CLASS, unless IS_END(), or
   unless it is an IS_ARG() with no white space in front -- and EXPR_FNAME is in
   none of those three, so the state "def" leaves Ruby in, and the
   EXPR_FNAME|EXPR_FITEM that "alias" and "undef" do, still reach a here
   document.  This table answers RubyEnd for "def" and RubyFname for the other
   two, and both refuse the header: "def <<EOS" is a shift here and a
   here-document header to MRI, which is the direction -- fewer bytes into a
   literal than Ruby takes -- that the rest of this file refuses.  What bounds
   it is what bounds the "class" and "module" readings above: no program that
   runs is written that way, because "def" is followed by a method name and
   "<<EOS" is not one.  "class" and "module" are not part of this exception at
   all -- EXPR_CLASS is named in that guard, which is what makes "class <<self"
   a singleton class rather than a here document.  Unlike every other reading in
   this comment, the "def <<EOS" one is argued from MRI's parse.y alone and has
   not been put to Ripper.lex: no Ruby 3.3 was available where it was
   written. *)
let ruby_state_after_word token =
  match token with
  | "end" | "self" | "nil" | "true" | "false" | "redo" | "retry" | "__FILE__"
  | "__LINE__" | "__ENCODING__" | "def" | "class" | "module" -> RubyEnd
  | "alias" | "undef" -> RubyFname
  | "if" | "unless" | "while" | "until" | "case" | "when" | "in" | "and" | "or"
  | "return" | "break" | "next" | "then" | "do" | "else" | "elsif" | "begin"
  | "ensure" | "rescue" | "for" -> RubyBegin
  | _ -> RubyArgument

(* NOTE: Ruby's rule for both "/" and "%" (parse_slash, parse_percent) is one
   rule: where a value is expected the byte always opens a literal; after an
   operand it never does; and in between it opens one exactly when white space
   stands before it and none behind it, which tells the command argument of
   "puts /x/" from the division in "a / b".  "/=" and "%=" are recognised before
   that last test, so an assignment operator is never read as a literal outside
   value position. *)
let ruby_literal_opens state space_seen source index =
  match state with
  | RubyBegin -> true
  | RubyEnd | RubyFname -> false
  | RubyArgument ->
    space_seen && index + 1 < Bytes.length source &&
    (let byte = Bytes.get source (index + 1) in
     byte <> '=' && not (ruby_is_space byte) && byte <> '\r' && byte <> '\n')

(* NOTE: Never after an operand, and after a bare word only when white space
   stands in front of it -- which is why "a << b" is a shift and "a <<b" is the
   here document that spacing exists to avoid. *)
let ruby_heredoc_may_open state space_seen =
  match state with
  | RubyBegin -> true | RubyArgument -> space_seen | RubyEnd | RubyFname -> false

(* NOTE: ruby_literal_opens answers the "%" question everywhere but one:
   parse_percent tests IS_lex_state(EXPR_FNAME | EXPR_FITEM) before it reaches
   the spacing rule and opens a symbol literal on "%s" there, so "alias%s(a)"
   and "alias %s(a)" open one alike.  Only "s" does; "%w", "%q" and the rest
   fall through to the ordinary answer, which is false in that state. *)
let ruby_percent_opens state space_seen source index form =
  (state = RubyFname && form = 's') || ruby_literal_opens state space_seen source index

(* NOTE: Where Ruby's two column-zero markers -- "=begin" and "__END__" -- are
   recognised.  A byte order mark is consumed before the first line is read, so
   the byte behind one still opens the first line. *)
let ruby_at_line_start source index =
  if index = 0 || index = byte_order_mark_width source then true
  else match Bytes.get source (index - 1) with
    | '\n' -> true
    | '\r' -> not (index < Bytes.length source && Bytes.get source index = '\n')
    | _ -> false

let ruby_word_boundary source index =
  index >= Bytes.length source ||
  (let byte = Bytes.get source index in
   ruby_is_space byte || byte = '\r' || byte = '\n')

(* NOTE: Ruby's word_match_p: the word ends at white space or at the end of the
   file, so "=beginner" is the "=" operator and a name. *)
let ruby_embedded_document source index =
  starts source index "=begin" && ruby_word_boundary source (index + 6)

(* NOTE: The document runs to the end of the "=end" line, whose remaining bytes
   Ruby skips along with the rest of it, and both markers stand at column zero. *)
let ruby_embedded_document_end source start =
  let rec loop index =
    if index >= Bytes.length source then (Bytes.length source, false)
    else
      let next = consume_newline source index in
      if starts source next "=end" && ruby_word_boundary source (next + 4)
      then (line_end source (next + 4), true)
      else loop (line_end source next)
  in loop (line_end source start)

(* NOTE: Ruby's whole_match_p: the marker is the whole line, so "__END__ x" is
   an ordinary name and the source runs on past it. *)
let ruby_data_marker source index =
  starts source index "__END__" &&
  (index + 7 >= Bytes.length source ||
   (let byte = Bytes.get source (index + 7) in byte = '\r' || byte = '\n'))

(* NOTE: The characters an operator method is spelled with, which is how a
   symbol naming one -- ":<=>", ":[]=", ":+@" -- is written. *)
let ruby_symbol_operator = function
  | '+' | '-' | '*' | '/' | '%' | '<' | '>' | '=' | '!' | '~' | '^' | '&' | '|'
  | '[' | ']' | '@' -> true
  | _ -> false

let ruby_symbol_head character =
  ruby_identifier_start character || character = '@' || character = '$' ||
  ruby_symbol_operator character

(* NOTE: Ruby's parse_gvar: a name, a digit run, "-" and one character, or one
   of the punctuation names.  "$\"" and "$'" are two of those names, which keeps
   the quote in either from opening a string, and "$/" and "$\\" two more.  "#"
   is not one of them -- the reference refuses that spelling outright -- so "$#"
   is a "$" on its own and the byte behind it opens the comment it opens
   everywhere else in the language. *)
let ruby_global_end source index =
  if index + 1 >= Bytes.length source then index + 1 else
  let byte = Bytes.get source (index + 1) in
  if ruby_identifier_start byte then ruby_identifier_end source (index + 2)
  else if ruby_digit byte then
    (let rec loop cursor =
       if cursor < Bytes.length source && ruby_digit (Bytes.get source cursor)
       then loop (cursor + 1) else cursor
     in loop (index + 2))
  else if byte = '-' then min (Bytes.length source) (index + 3)
  else if String.contains "~*$?!@/\\;,.=:<>\"&`'+" byte then index + 2
  else index + 1

let ruby_at_variable_end source index =
  let cursor =
    if index + 1 < Bytes.length source && Bytes.get source (index + 1) = '@'
    then index + 2 else index + 1 in
  ruby_identifier_end source cursor

(* NOTE: A symbol is a name -- with the "@", "@@" or "$" of a variable in front
   of it where one is meant -- or one of the operator methods, which is read
   here as the run of characters those are spelled with rather than as a table
   of them: a run that names no method is a syntax error either way, and reading
   it as one symbol keeps the byte after it out of the literal path. *)
let ruby_symbol_end source index =
  let cursor = index + 1 in
  if cursor >= Bytes.length source then cursor
  else match Bytes.get source cursor with
    | '$' -> ruby_global_end source cursor
    | '@' -> ruby_at_variable_end source cursor
    | byte when ruby_identifier_start byte -> ruby_word_end source cursor
    | _ ->
      let rec loop probe =
        if probe < Bytes.length source && ruby_symbol_operator (Bytes.get source probe)
        then loop (probe + 1) else probe
      in loop cursor

let ruby_character_width byte =
  let code = Char.code byte in
  if code >= 0xf0 && code <= 0xf7 then 4
  else if code >= 0xe0 && code <= 0xef then 3
  else if code >= 0xc0 && code <= 0xdf then 2
  else 1

(* NOTE: Ruby's parse_qmark: white space behind the "?" makes it the operator; a
   character outside ASCII is a literal whole; an ASCII letter, digit or "_"
   with another name byte behind it is the operator again, which keeps
   "a ?bc : d" a ternary; and everything else -- an escape, or one punctuation
   byte -- is a literal. *)
let ruby_character_literal_end source question =
  let index = question + 1 in
  if index >= Bytes.length source then None else
  let byte = Bytes.get source index in
  if ruby_is_space byte || byte = '\r' || byte = '\n' then None
  else if Char.code byte land 0x80 <> 0 then
    Some (min (Bytes.length source) (index + ruby_character_width byte))
  else if byte = '\\' then
    (if index + 2 < Bytes.length source && Bytes.get source (index + 1) = 'u' &&
       Bytes.get source (index + 2) = '{'
     then
       let rec loop cursor =
         if cursor < Bytes.length source && Bytes.get source cursor <> '}'
         then loop (cursor + 1) else min (Bytes.length source) (cursor + 1)
       in Some (loop (index + 3))
     else Some (min (Bytes.length source) (index + 2)))
  else if ruby_identifier_continue byte && index + 1 < Bytes.length source &&
    ruby_identifier_continue (Bytes.get source (index + 1))
  then None
  else Some (index + 1)

(* NOTE: The option letters that may follow a regular expression are read as a
   run of ASCII letters rather than as the set "imxonesu": a letter that is not
   an option is a syntax error either way, and taking it here leaves the lexer
   where the letters ended rather than in the middle of them. *)
let ruby_regexp_flags_end source index =
  let rec loop cursor =
    if cursor < Bytes.length source &&
      (let byte = Bytes.get source cursor in
       (byte >= 'a' && byte <= 'z') || (byte >= 'A' && byte <= 'Z'))
    then loop (cursor + 1) else cursor
  in loop index

(* NOTE: Ruby's parse_percent: the byte after the "%" is the delimiter unless it
   is alphanumeric, in which case it names the form and the byte after that is
   the delimiter.  A delimiter is any ASCII byte that is not alphanumeric, the
   space of "% a " included.  "(", "[", "{" and "<" pair with their closer and
   nest; every other delimiter closes with itself. *)
let ruby_percent_header source start =
  if start + 1 >= Bytes.length source then None else
  let first = Bytes.get source (start + 1) in
  let header =
    if ruby_alphanumeric first then
      (if not (String.contains "qQwWiIsrx" first) then None
       else if start + 2 >= Bytes.length source then None
       else Some (first, Bytes.get source (start + 2), start + 3))
    else Some ('Q', first, start + 2) in
  match header with
  | None -> None
  | Some (form, delimiter, content) ->
    if ruby_alphanumeric delimiter || Char.code delimiter land 0x80 <> 0 then None
    else
      let close = match delimiter with
        | '(' -> ')' | '[' -> ']' | '{' -> '}' | '<' -> '>' | other -> other in
      Some { percent_form = form; percent_open = delimiter; percent_close = close;
             percent_content = content;
             percent_interpolates = String.contains "QWIrx" form }

(* NOTE: Ruby's heredoc_identifier: an optional "-" or "~", then a quoted
   terminator or a bare word.  The bare word is a run of is_identchar bytes from
   its very first one, which is a wider set than a name may start with: a digit
   is an identchar, so "<<2" is a here document terminated by a line reading "2"
   and "<<9x" one terminated by "9x".  Refusing digits would read the body as
   code, which is the one direction that invents a comment out of bytes Ruby has
   inside a string, so they are taken.  Whether the "<<" stands where one may
   open at all is ruby_heredoc_may_open's question, and it is what still leaves
   "a[0] <<2" and "p 1 <<2" the shift they are.  A quoted terminator that runs
   past the end of its line opens nothing. *)
let ruby_heredoc_header source index =
  let cursor = index + 2 in
  let indented = cursor < Bytes.length source &&
    (Bytes.get source cursor = '-' || Bytes.get source cursor = '~') in
  let cursor = if indented then cursor + 1 else cursor in
  let opener =
    if cursor >= Bytes.length source then `Absent
    else match Bytes.get source cursor with
      | ('\'' | '"' | '`') as quote -> `Quoted quote
      | byte when ruby_identifier_continue byte -> `Bare
      | _ -> `Absent in
  match opener with
  | `Absent -> None
  | `Quoted quote ->
    let start = cursor + 1 in
    let rec loop finish =
      if finish >= Bytes.length source then None
      else match Bytes.get source finish with
        | '\r' | '\n' -> None
        | byte when byte = quote ->
          Some (Bytes.sub_string source start (finish - start), finish + 1)
        | _ -> loop (finish + 1) in
    (match loop start with
     | None -> None
     | Some (label, finish) ->
       Some ({ heredoc_operator = index; heredoc_label = label;
               heredoc_indented = indented; heredoc_interpolates = quote <> '\'' }, finish))
  | `Bare ->
    let finish = ruby_identifier_end source (cursor + 1) in
    Some ({ heredoc_operator = index;
            heredoc_label = Bytes.sub_string source cursor (finish - cursor);
            heredoc_indented = indented; heredoc_interpolates = true }, finish)

(* NOTE: Ruby's whole_match_p: the terminator is the whole line, with leading
   white space skipped only for the "<<-" and "<<~" forms. *)
let ruby_heredoc_terminates source index heredoc =
  let rec skip probe =
    if heredoc.heredoc_indented && probe < Bytes.length source &&
      ruby_is_space (Bytes.get source probe)
    then skip (probe + 1) else probe in
  let probe = skip index in
  let width = String.length heredoc.heredoc_label in
  starts source probe heredoc.heredoc_label &&
  (probe + width >= Bytes.length source ||
   (let byte = Bytes.get source (probe + width) in byte = '\r' || byte = '\n'))

(* NOTE: The first "count" elements of "items", which is how the shared here
   document queue is cut back to what an enclosing scan had put in it. *)
let rec ruby_take count items =
  if count <= 0 then []
  else match items with [] -> [] | head :: tail -> head :: ruby_take (count - 1) tail

let scan_ruby source language options accumulator =
  let length = Bytes.length source in
  (* NOTE: The here documents opened on the physical line being read and not yet
     given a body, in the order Ruby will consume them.  It is one queue for the
     whole line rather than one per nested scan because a header may stand
     inside an interpolation -- "puts \"#{ <<EOS }\"" opens a here document whose
     body is the line under that one -- and because Ruby takes the bodies in
     header order across the whole line, so an opener written before an
     interpolation and one written inside it queue together.  The queue is
     drained by whichever scan reaches the line break first, which is why it has
     to outlive the "}" a nested scan returns from. *)
  let pending = ref [] in
  let pending_push heredoc = pending := !pending @ [heredoc] in
  let pending_take () = let opened = !pending in pending := []; opened in
  let rec code index interpolation depth =
    if depth > 256 then begin
      add_error accumulator "nesting-limit" "Ruby lexical nesting limit exceeded" index index;
      length
    end else
    (* NOTE: Where this call's own openers begin in the shared queue.  An
       enclosing scan's entries sit in front of them and are that scan's to
       report, which is what keeps one unterminated here document to one
       diagnostic. *)
    let base = List.length !pending in
    let rec loop index state space_seen braces =
      if index >= length then begin
        (* NOTE: A here document opened on a last line that has no break of its
           own never reaches "bodies", which is driven from the break.  It is
           unterminated all the same, and is reported from its own "<<" with the
           span that call would have given it.  Only this call's own openers are
           reported here -- the ones from "base" on -- because an enclosing scan
           reports its own, and the queue is cut back to "base" so that it
           reports them once. *)
        (match List.nth_opt !pending base with
         | Some heredoc ->
           add_error accumulator "unterminated-heredoc"
             "unterminated Ruby here document" heredoc.heredoc_operator length;
           pending := ruby_take base !pending
         | None -> ());
        if interpolation then
          add_error accumulator "unterminated-interpolation"
            "unterminated Ruby interpolation" index index;
        index
      end else
      match Bytes.get source index with
      | '#' ->
        let finish = line_end source (index + 1) in
        add_comment accumulator source language options Line index finish;
        loop finish state space_seen braces
      | '=' when ruby_at_line_start source index && ruby_embedded_document source index ->
        let finish, closed = ruby_embedded_document_end source index in
        add_comment accumulator source language options Block index finish;
        if not closed then add_error accumulator "unterminated-comment"
          "unterminated Ruby embedded document" index finish;
        loop finish RubyBegin false braces
      (* NOTE: Everything past the marker is the DATA section, which is not
         source and holds no comments. *)
      | '_' when ruby_at_line_start source index && ruby_data_marker source index ->
        loop length state space_seen braces
      | '\r' | '\n' ->
        let next = consume_newline source index in
        (match pending_take () with
         | [] -> loop next RubyBegin false braces
         | opened ->
           (match bodies next opened (depth + 1) with
            | Some finish -> loop finish RubyBegin false braces
            | None -> loop length state space_seen braces))
      | '\'' -> loop (string index false depth) RubyEnd false braces
      | '"' | '`' -> loop (string index true depth) RubyEnd false braces
      | ':' when starts source index "::" -> loop (index + 2) RubyEnd false braces
      | ':' when index + 1 < length &&
          (Bytes.get source (index + 1) = '\'' || Bytes.get source (index + 1) = '"') ->
        loop (string (index + 1) (Bytes.get source (index + 1) = '"') depth)
          RubyEnd false braces
      | ':' when index + 1 < length && ruby_symbol_head (Bytes.get source (index + 1)) ->
        loop (ruby_symbol_end source index) RubyEnd false braces
      | '?' ->
        (match ruby_character_literal_end source index with
         | Some finish when state <> RubyEnd && state <> RubyFname ->
           loop finish RubyEnd false braces
         | _ -> loop (index + 1) RubyBegin false braces)
      | '%' ->
        (match ruby_percent_header source index with
         | Some literal
           when ruby_percent_opens state space_seen source index literal.percent_form ->
           (* NOTE: "alias" and "undef" hold EXPR_FNAME|EXPR_FITEM across the whole
              statement rather than only up to the first name: Ripper.lex under
              Ruby 3.3.12 reports that state again after the ")" of the first
              symbol, which is what makes "alias%s(a)%s(b # c)" two symbols and
              not one symbol and a modulo. *)
           let next_state =
             if state = RubyFname && literal.percent_form = 's' then RubyFname else RubyEnd in
           loop (percent index literal depth) next_state false braces
         | _ -> loop (index + 1) RubyBegin false braces)
      | '/' ->
        if ruby_literal_opens state space_seen source index
        then loop (regexp index depth) RubyEnd false braces
        else loop (index + 1) RubyBegin false braces
      | '<' when starts source index "<<" ->
        (match ruby_heredoc_header source index with
         | Some (heredoc, finish) when ruby_heredoc_may_open state space_seen ->
           pending_push heredoc; loop finish RubyEnd false braces
         | _ -> loop (index + 2) RubyBegin false braces)
      | '$' -> loop (ruby_global_end source index) RubyEnd false braces
      | '@' -> loop (ruby_at_variable_end source index) RubyEnd false braces
      | '{' -> loop (index + 1) RubyBegin false (braces + 1)
      | '}' ->
        if interpolation && braces = 1 then index + 1
        else loop (index + 1) RubyEnd false
          (if interpolation then braces - 1 else braces)
      | '(' | '[' -> loop (index + 1) RubyBegin false braces
      | ')' | ']' -> loop (index + 1) RubyEnd false braces
      (* NOTE: The method-call dot, which is also the two range operators.  All
         three want the byte after them read as a name rather than as a literal
         delimiter, which is what RubyEnd says. *)
      | '.' -> loop (index + 1) RubyEnd false braces
      (* NOTE: Outside a literal a backslash only continues the line, which is
         white space to the grammar. *)
      | '\\' when index + 1 < length &&
          (Bytes.get source (index + 1) = '\r' || Bytes.get source (index + 1) = '\n') ->
        loop (consume_newline source (index + 1)) state true braces
      | '\\' -> loop (min length (index + 2)) RubyEnd false braces
      | byte when ruby_digit byte ->
        loop (ruby_number_end source index) RubyEnd false braces
      | byte when ruby_identifier_start byte ->
        let finish = ruby_word_end source index in
        loop finish (ruby_state_after_word (Bytes.sub_string source index (finish - index)))
          false braces
      | byte when ruby_is_space byte -> loop (index + 1) state true braces
      | _ -> loop (index + 1) RubyBegin false braces
    in loop index RubyBegin false (if interpolation then 1 else 0)

  (* NOTE: A single-quoted string escapes only "\'" and "\\", and every other
     backslash is a byte of it -- but the byte after a backslash can never be
     the closing quote unless the pair is the "\'" escape, so skipping two finds
     the same closer either way.  The other two take the full escape set and
     interpolate, and none of the three ends at a line break. *)
  and string start interpolates depth =
    let quote = Bytes.get source start in
    let rec loop index =
      if index >= length then begin
        add_error accumulator "unterminated-string"
          (match quote with
           | '\'' -> "unterminated Ruby single-quoted string"
           | '"' -> "unterminated Ruby double-quoted string"
           | _ -> "unterminated Ruby backtick string")
          start index;
        index
      end
      else if Bytes.get source index = '\\' then loop (min length (index + 2))
      else if Bytes.get source index = quote then index + 1
      else if interpolates && starts source index "#{" then
        loop (code (index + 2) true (depth + 1))
      else loop (index + 1)
    in loop (start + 1)

  (* NOTE: A "[" opens a character class, where the delimiter is one of the
     pattern's own bytes.  That is deliberately more forgiving than Ruby's own
     tokadd_string, which ends the literal at the first unescaped "/" wherever
     it stands: reading "/[/]/" as one literal keeps the rest of the line inside
     it, which hides bytes from a removal rather than exposing them. *)
  and regexp start depth =
    let rec loop index in_class =
      if index >= length then begin
        add_error accumulator "unterminated-string"
          "unterminated Ruby regular expression" start index;
        index
      end
      else match Bytes.get source index with
        | '\\' -> loop (min length (index + 2)) in_class
        | '[' -> loop (index + 1) true
        | ']' -> loop (index + 1) false
        | '/' when not in_class -> ruby_regexp_flags_end source (index + 1)
        | '#' when starts source index "#{" -> loop (code (index + 2) true (depth + 1)) in_class
        | _ -> loop (index + 1) in_class
    in loop (start + 1) false

  (* NOTE: A paired delimiter nests, which is what lets "%w[a [b] c]" hold a
     bracket; every other delimiter closes with itself and cannot.  The
     interpolating forms read "#{ ... }" as an expression before either
     delimiter is considered, so the braces of one never count towards a
     "%Q{...}" nesting depth. *)
  and percent start literal depth =
    let rec loop index nesting =
      if index >= length then begin
        add_error accumulator "unterminated-string"
          "unterminated Ruby percent literal" start index;
        index
      end
      else if Bytes.get source index = '\\' then loop (min length (index + 2)) nesting
      else if literal.percent_interpolates && starts source index "#{" then
        loop (code (index + 2) true (depth + 1)) nesting
      else if literal.percent_open <> literal.percent_close &&
        Bytes.get source index = literal.percent_open
      then loop (index + 1) (nesting + 1)
      else if Bytes.get source index = literal.percent_close then
        (if nesting = 1 then
           (if literal.percent_form = 'r' then ruby_regexp_flags_end source (index + 1)
            else index + 1)
         else loop (index + 1) (nesting - 1))
      else loop (index + 1) nesting
    in loop literal.percent_content 1

  (* NOTE: Every here document opened on the line that has just ended, in the
     order they were opened.  None once one of them runs out of file, which is
     reported from the "<<" that opened it rather than from the line it
     swallowed. *)
  and bodies index heredocs depth =
    match heredocs with
    | [] -> Some index
    | heredoc :: tail ->
      (match body index heredoc depth with
       | Some finish -> bodies finish tail depth
       | None ->
         add_error accumulator "unterminated-heredoc" "unterminated Ruby here document"
           heredoc.heredoc_operator length;
         None)

  (* NOTE: The body is opaque: only "#{ ... }" in an interpolating form is read
     as code, and the terminator is looked for at the start of each of the
     body's own lines, so one written inside an interpolation is content like
     the rest of it. *)
  and body index heredoc depth =
    let rec line index =
      if index >= length then None
      else if ruby_heredoc_terminates source index heredoc then
        Some (min length (consume_newline source (line_end source index)))
      else
        let rec content cursor =
          if cursor >= length then cursor
          else match Bytes.get source cursor with
            | '\r' | '\n' -> cursor
            | '\\' when heredoc.heredoc_interpolates ->
              content (if starts source cursor "\\\r\n" then cursor + 3
                       else min length (cursor + 2))
            | '#' when heredoc.heredoc_interpolates && starts source cursor "#{" ->
              content (code (cursor + 2) true (depth + 1))
            | _ -> content (cursor + 1) in
        let finish = content index in
        if finish >= length then None
        else
          let next = consume_newline source finish in
          (* NOTE: A body line is a physical line like any other, so a header
             reached through an interpolation on it queues for the line under it
             and is read there -- before this body resumes. *)
          (match pending_take () with
           | [] -> line next
           | opened ->
             (match bodies next opened (depth + 1) with
              | Some resume -> line resume
              | None -> None))
    in line index
  in ignore (code 0 false 0)

let rec scan_html source language options accumulator =
  let tag_boundary = function None -> true | Some character ->
    ascii_whitespace character || character = '>' || character = '/' in
  let rec tag_end index quote =
    if index >= Bytes.length source then None else
    let character = Bytes.get source index in
    match quote with Some active when character = active -> tag_end (index + 1) None
    | Some _ -> tag_end (index + 1) quote
    | None when character = '\'' || character = '"' -> tag_end (index + 1) (Some character)
    | None when character = '>' -> Some (index + 1) | None -> tag_end (index + 1) None in
  let embedded_start index =
    if ascii_case_starts source index "<script" &&
      tag_boundary (if index + 7 < Bytes.length source then Some (Bytes.get source (index + 7)) else None)
    then Some ("script", JavaScript)
    else if ascii_case_starts source index "<style" &&
      tag_boundary (if index + 6 < Bytes.length source then Some (Bytes.get source (index + 6)) else None)
    then Some ("style", Css)
    else None in
  let find_close start name =
    let token = "</" ^ name in
    let rec loop cursor = match ascii_case_find source cursor token with
      | None -> None
      | Some candidate ->
        let after = candidate + String.length token in
        if tag_boundary (if after < Bytes.length source then Some (Bytes.get source after) else None)
        then Some candidate else loop after in
    loop start in
  let tag_candidate index =
    if index + 1 >= Bytes.length source || Bytes.get source index <> '<' then false else
    let next = Bytes.get source (index + 1) in
    (next >= 'a' && next <= 'z') || (next >= 'A' && next <= 'Z') ||
    next = '!' || next = '?' ||
    (next = '/' && index + 2 < Bytes.length source &&
      let after = Bytes.get source (index + 2) in
      (after >= 'a' && after <= 'z') || (after >= 'A' && after <= 'Z')) in
  let merge offset report =
    List.iter (fun (comment : comment) -> accumulator.comments_rev <- { comment with span = { start = comment.span.start + offset; finish = comment.span.finish + offset } } :: accumulator.comments_rev) report.comments;
    List.iter (fun (diagnostic : diagnostic) -> accumulator.diagnostics_rev <- { diagnostic with span = { start = diagnostic.span.start + offset; finish = diagnostic.span.finish + offset } } :: accumulator.diagnostics_rev) report.diagnostics in
  let rec loop index =
    if index >= Bytes.length source then ()
    else if starts source index "<!--" then begin
      let finish, closed = match find_from source (index + 4) "-->" with Some close -> (close + 3, true) | None -> (Bytes.length source, false) in
      add_comment accumulator source language options HtmlComment index finish;
      if not closed then add_error accumulator "unterminated-comment" "unterminated HTML comment" index finish; loop finish
    end else match embedded_start index with
    | Some (name, embedded) -> (match tag_end (index + 1) None with
      | None -> add_error accumulator "unterminated-html-tag"
          "unterminated HTML raw-text start tag" index (Bytes.length source)
      | Some content_start ->
        let closing = find_close content_start name in
        let content_finish = match closing with Some value -> value | None -> Bytes.length source in
        let child_source = Bytes.sub source content_start (content_finish - content_start) in
        let child = { comments_rev = []; diagnostics_rev = []; yaml_blocks_rev = [] } in
        if embedded = JavaScript
        then scan_javascript ~offset:content_start child_source embedded options child
        else scan_slash child_source embedded options child;
        merge content_start { language = embedded; comments = List.rev child.comments_rev;
          diagnostics = List.rev child.diagnostics_rev; valid = true };
        (match closing with
        | None -> add_error accumulator "unterminated-embedded-language"
            "unterminated HTML script or style element" index (Bytes.length source)
        | Some closing -> (match tag_end closing None with
          | Some finish -> loop finish
          | None -> add_error accumulator "unterminated-html-tag"
              "unterminated HTML raw-text end tag" closing (Bytes.length source))))
    | None ->
      if not (tag_candidate index) then loop (index + 1)
      else match tag_end (index + 1) None with
        | Some finish -> loop finish
        | None -> add_error accumulator "unterminated-html-tag" "unterminated HTML tag"
            index (Bytes.length source)
  in loop 0

and scan source language options =
  let accumulator = { comments_rev = []; diagnostics_rev = []; yaml_blocks_rev = [] } in
  (match language with
  | Rust | C | Cpp | Go | Kotlin | Css | Jsonc -> scan_slash source language options accumulator
  | Java -> scan_java source language options accumulator
  | JavaScript | TypeScript -> scan_javascript source language options accumulator
  | Ocaml -> scan_ocaml source language options accumulator
  | Python -> scan_python source language options accumulator
  | Shell -> scan_shell source language options accumulator
  | Sql -> scan_sql source language options accumulator
  | Toml -> scan_toml source language options accumulator
  | Lua -> scan_lua source language options accumulator
  | Yaml -> scan_yaml source language options accumulator
  | Php -> scan_php source language options accumulator
  | Ruby -> scan_ruby source language options accumulator
  | Zig -> scan_zig source language options accumulator
  | Dart -> scan_dart source language options accumulator
  | R -> scan_r source language options accumulator
  | Html -> scan_html source language options accumulator
  | Unknown -> add_error accumulator "unknown-language" "a language is required" 0 0);
  let comments = List.rev accumulator.comments_rev in
  let diagnostics = List.rev accumulator.diagnostics_rev in
  { language; comments; diagnostics; valid = not (List.exists (fun diagnostic -> diagnostic.severity = Error) diagnostics) }

let validate_profile profile =
  let token name value =
    if value = "" then Result.Error (name ^ " delimiter must not be empty")
    else if String.contains value '\r' || String.contains value '\n' then
      Result.Error "delimiter contains a newline"
    else Result.Ok () in
  let rec all = function
    | [] -> Result.Ok ()
    | result :: tail -> (match result with Result.Ok () -> all tail | Result.Error _ as error -> error) in
  if String.trim profile.name = "" then Result.Error "profile name must not be empty"
  else if profile.line_comments = [] && profile.block_comments = [] then
    Result.Error "profile must define at least one comment delimiter"
  else
  match all (List.concat [
    List.map (fun delimiter -> token "line start" delimiter.line_start) profile.line_comments;
    List.concat_map (fun delimiter -> [token "block start" delimiter.block_start;
      token "block end" delimiter.block_end_token]) profile.block_comments;
    List.concat_map (fun delimiter -> [token "string start" delimiter.string_start;
      token "string end" delimiter.string_end] @
      (match delimiter.escape with Some value -> [token "string escape" value] | None -> []))
      profile.strings]) with
  | Result.Error _ as error -> error
  | Result.Ok () ->
    let comment_starts = List.map (fun item -> item.line_start) profile.line_comments @
      List.map (fun item -> item.block_start) profile.block_comments in
    let string_starts = List.map (fun item -> item.string_start) profile.strings in
    let rec prefixes = function
      | [] -> Result.Ok ()
      | first :: tail ->
        (match List.find_opt (fun second -> String.starts_with ~prefix:first second ||
          String.starts_with ~prefix:second first) tail with
        | Some second -> Result.Error (Printf.sprintf "ambiguous delimiter prefix: `%s` and `%s`"
            first second)
        | None -> prefixes tail) in
    (match prefixes comment_starts with
    | Result.Error _ as error -> error
    | Result.Ok () -> match List.find_map (fun left ->
        List.find_opt (fun right -> String.starts_with ~prefix:left right ||
          String.starts_with ~prefix:right left) string_starts |>
        Option.map (fun right -> (left, right))) comment_starts with
      | Some (left, right) -> Result.Error (Printf.sprintf
          "ambiguous comment/string delimiter prefix: `%s` and `%s`" left right)
      | None -> match prefixes string_starts with
        | Result.Error message -> Result.Error
            (String.concat "" ["ambiguous string delimiter prefix";
              String.sub message (String.length "ambiguous delimiter prefix")
                (String.length message - String.length "ambiguous delimiter prefix")])
        | Result.Ok () -> match List.find_opt (fun delimiter -> delimiter.nested &&
          (delimiter.block_start = delimiter.block_end_token ||
           contains delimiter.block_start delimiter.block_end_token ||
           contains delimiter.block_end_token delimiter.block_start)) profile.block_comments with
        | Some _ -> Result.Error "nested block delimiters require distinct non-overlapping start and end tokens"
        | None -> if List.exists (fun item -> item.pattern = "" || String.trim item.reason = "")
            profile.protected_patterns
          then Result.Error "protected patterns need non-empty `contains` and `reason` values"
          else Result.Ok ())

let profile_comment source profile options start finish kind =
  let raw = Bytes.sub_string source start (finish - start) in
  match List.find_opt (fun item -> contains raw item.pattern) profile.protected_patterns with
  | None -> { span = { start; finish }; kind; disposition = disposition options kind raw }
  | Some protected ->
    let kind = Directive in
    let selected = disposition options kind raw in
    let disposition = match selected with Keep _ -> Keep protected.reason | Remove -> Remove in
    { span = { start; finish }; kind; disposition }

let scan_profile source profile options =
  match validate_profile profile with
  | Result.Error _ as error -> error
  | Result.Ok () ->
    let accumulator = { comments_rev = []; diagnostics_rev = []; yaml_blocks_rev = [] } in
    let rec scan_string token_start delimiter index =
      if index >= Bytes.length source then (index, false)
      else if starts source index delimiter.string_end then
        (index + String.length delimiter.string_end, true)
      else match delimiter.escape with
      | Some escape when starts source index escape ->
        scan_string token_start delimiter
          (min (Bytes.length source) (index + String.length escape + 1))
      | _ when not delimiter.multiline &&
          (Bytes.get source index = '\r' || Bytes.get source index = '\n') -> (index, false)
      | _ -> scan_string token_start delimiter (index + 1) in
    let rec scan_block delimiter index depth =
      if index >= Bytes.length source then (index, depth)
      else if delimiter.nested && starts source index delimiter.block_start then
        scan_block delimiter (index + String.length delimiter.block_start) (depth + 1)
      else if starts source index delimiter.block_end_token then
        let remaining = depth - 1 in
        if remaining = 0 then (index + String.length delimiter.block_end_token, 0)
        else scan_block delimiter (index + String.length delimiter.block_end_token) remaining
      else scan_block delimiter (index + 1) depth in
    let rec loop index =
      if index >= Bytes.length source then () else
      match List.find_opt (fun delimiter -> starts source index delimiter.string_start)
        profile.strings with
      | Some delimiter ->
        let finish, closed = scan_string index delimiter
          (index + String.length delimiter.string_start) in
        if not closed then add_error accumulator "unterminated-profile-string"
          (Printf.sprintf "unterminated string in profile `%s`" profile.name) index finish;
        loop finish
      | None -> match List.find_opt (fun delimiter -> starts source index delimiter.line_start &&
          (not delimiter.requires_boundary || index = 0 ||
            ascii_whitespace (Bytes.get source (index - 1)))) profile.line_comments with
        | Some delimiter ->
          let finish = line_end source (index + String.length delimiter.line_start) in
          accumulator.comments_rev <- profile_comment source profile options index finish
            delimiter.line_kind :: accumulator.comments_rev;
          loop finish
        | None -> match List.find_opt (fun delimiter -> starts source index delimiter.block_start)
            profile.block_comments with
          | Some delimiter ->
            let finish, depth = scan_block delimiter
              (index + String.length delimiter.block_start) 1 in
            accumulator.comments_rev <- profile_comment source profile options index finish
              delimiter.block_kind :: accumulator.comments_rev;
            if depth <> 0 then add_error accumulator "unterminated-profile-comment"
              (Printf.sprintf "unterminated block comment in profile `%s`" profile.name)
              index finish;
            loop finish
          | None -> loop (index + 1)
    in
    loop 0;
    let comments = List.rev accumulator.comments_rev and diagnostics = List.rev accumulator.diagnostics_rev in
    Result.Ok { language = Unknown; comments; diagnostics;
      valid = not (List.exists (fun diagnostic -> diagnostic.severity = Error) diagnostics) }

let newline_bytes source span =
  let buffer = Buffer.create (span.finish - span.start) in
  let rec loop index =
    if index < span.finish then
      match unicode_line_terminator_width source index with
      | Some width when index + width <= span.finish ->
        Buffer.add_string buffer (Bytes.sub_string source index width);
        loop (index + width)
      | _ -> loop (index + 1) in
  loop span.start;
  Bytes.of_string (Buffer.contents buffer)

let utf8_character source index finish =
  let byte offset = Char.code (Bytes.get source (index + offset)) in
  let continuation value = value land 0xc0 = 0x80 in
  if index >= finish then None else
  let first = byte 0 in
  if first < 0x80 then Some (first, 1)
  else if first >= 0xc2 && first <= 0xdf && index + 1 < finish && continuation (byte 1)
  then Some (((first land 0x1f) lsl 6) lor (byte 1 land 0x3f), 2)
  else if first >= 0xe0 && first <= 0xef && index + 2 < finish &&
    continuation (byte 1) && continuation (byte 2) &&
    (first <> 0xe0 || byte 1 >= 0xa0) && (first <> 0xed || byte 1 < 0xa0)
  then Some (((first land 0x0f) lsl 12) lor ((byte 1 land 0x3f) lsl 6) lor
    (byte 2 land 0x3f), 3)
  else if first >= 0xf0 && first <= 0xf4 && index + 3 < finish &&
    continuation (byte 1) && continuation (byte 2) && continuation (byte 3) &&
    (first <> 0xf0 || byte 1 >= 0x90) && (first <> 0xf4 || byte 1 < 0x90)
  then Some (((first land 0x07) lsl 18) lor ((byte 1 land 0x3f) lsl 12) lor
    ((byte 2 land 0x3f) lsl 6) lor (byte 3 land 0x3f), 4)
  else None

let prepended_concatenation_mark value =
  (value >= 0x0600 && value <= 0x0605) || value = 0x06dd || value = 0x070f ||
  value = 0x0890 || value = 0x0891 || value = 0x08e2 || value = 0x110bd ||
  value = 0x110cd

(* INVARIANT: Keep this character-level policy aligned with unicode-width's
   documented Unicode 17 rules.  Uucp's tty_width_hint deliberately follows the
   much simpler historical wcwidth heuristic and, in particular, gives
   decomposed Hangul vowel/trailing jamo a width of one. *)
let unicode_width value =
  let character = Uchar.of_int value in
  if value < 0x20 || (value >= 0x7f && value < 0xa0) then 0
  else if value = 0x17d8 then 3
  else if value = 0x2d7f then 1
  else if value = 0x115f || value = 0x17a4 then 2
  else if Uucp.Gen.is_default_ignorable character ||
    Uucp.Func.is_grapheme_extend character ||
    (match Uucp.Hangul.syllable_type character with `V | `T -> true | _ -> false) ||
    value = 0x0605 || value = 0x070f || value = 0x0890 || value = 0x0891 ||
    value = 0x08e2 || value = 0xa8fa ||
    (Uucp.Break.grapheme_cluster character = `PP &&
      not (prepended_concatenation_mark value))
  then 0
  else match Uucp.Break.east_asian_width character with
  | `F | `W -> 2
  | _ -> 1

let advance_display_column source start finish initial_column =
  let rec loop index column =
    if index >= finish then column
    else match unicode_line_terminator_width source index with
    | Some width when index + width <= finish -> loop (index + width) 0
    | _ ->
    if Bytes.get source index = '\t' then
      loop (index + 1) (column + 8 - column mod 8)
    else if Char.code (Bytes.get source index) < 0x80 then loop (index + 1) (column + 1)
    else match utf8_character source index finish with
      | Some (value, length) -> loop (index + length) (column + unicode_width value)
      | None -> loop (index + 1) (column + 1)
  in loop start initial_column

let column_replacement source span initial_column =
  let output = Buffer.create (span.finish - span.start) in
  let rec loop index column =
    if index >= span.finish then (Bytes.of_string (Buffer.contents output), column)
    else match unicode_line_terminator_width source index with
    | Some width when index + width <= span.finish ->
      Buffer.add_string output (Bytes.sub_string source index width);
      loop (index + width) 0
    | _ -> match Bytes.get source index with
    | '\t' ->
      let width = 8 - column mod 8 in
      Buffer.add_string output (String.make width ' ');
      loop (index + 1) (column + width)
    | character when Char.code character < 0x80 ->
      Buffer.add_char output ' '; loop (index + 1) (column + 1)
    | _ -> (match utf8_character source index span.finish with
      | Some (value, length) ->
        let width = unicode_width value in
        Buffer.add_string output (String.make width ' ');
        loop (index + length) (column + width)
      | None -> Buffer.add_char output ' '; loop (index + 1) (column + 1))
  in loop span.start initial_column

(* NOTE: The layout arithmetic calls a byte blank when `ascii_whitespace` does,
   which leaves the vertical tab out, so a line carrying one is not blank and
   survives `compact`.  Lua's own lexer disagrees -- see `lua_is_space` -- and
   that is a different question: one asks what the chunk means, this one asks
   what the file looks like. *)
let has_non_whitespace_neighbors source span =
  span.start > 0 && span.finish < Bytes.length source &&
  not (ascii_whitespace (Bytes.get source (span.start - 1))) &&
  not (ascii_whitespace (Bytes.get source span.finish))

(* NOTE: What layout `lines` leaves in place of a removed comment: the line
   terminators the comment spanned, so every following line keeps its number,
   and a single space when the comment was all that kept two tokens apart.  A
   comment that spanned a terminator needs no space of its own, because a
   newline is a lexical separator already. *)
let line_replacement source kind span =
  if kind = HtmlComment then Bytes.empty else
  let output = newline_bytes source span in
  if Bytes.length output > 0 then output
  else if has_non_whitespace_neighbors source span then Bytes.of_string " "
  else Bytes.empty

(* NOTE: The first line terminator inside a comment, as the bytes that wrote
   it, so a CRLF file keeps its CRLF.  A terminator that would reach past the
   end of the comment is not one: the same rule newline_bytes applies. *)
let first_line_terminator source span =
  let rec loop index =
    if index >= span.finish then None
    else match unicode_line_terminator_width source index with
    | Some width when index + width <= span.finish -> Some (Bytes.sub source index width)
    | _ -> loop (index + 1)
  in loop span.start

(* NOTE: How the line a comment ended on runs out: where the blanks after the
   comment stop, and how wide the line terminator there is - 0 at the end of the
   source.  None when something other than blanks follows on that line, which is
   what makes the comment an interior one rather than the last thing on its
   line. *)
let line_tail source from =
  let rec loop index =
    match unicode_line_terminator_width source index with
    | Some width -> Some (index, width)
    | None ->
      if index >= Bytes.length source then Some (index, 0)
      else if ascii_whitespace (Bytes.get source index) then loop (index + 1)
      else None
  in loop from

(* NOTE: Where the run of blanks that ends at `at` begins.  It never reaches
   before `floor` and never crosses a line terminator, so trimming what a
   removal left at the end of a line can never touch the line before it. *)
let blank_start source at floor =
  let rec loop index =
    if index > floor && ascii_whitespace (Bytes.get source (index - 1)) &&
      unicode_line_terminator_width source (index - 1) = None
    then loop (index - 1) else index
  in loop at

let rec has_code source index finish =
  index < finish &&
  (not (ascii_whitespace (Bytes.get source index)) || has_code source (index + 1) finish)

(* NOTE: One layout `compact` edit.  `line_start` is where the line holding the
   comment begins, `floor` is the end of the previous edit and `ceiling` the
   start of the next comment, so the span that comes back is sorted and
   non-overlapping with its neighbours however a scanner laid the comments out.

   An HTML comment closes up completely under every layout, the newlines it
   spanned included, so it never counts as ending a line by spanning one and
   never puts a terminator back. *)
let compact_edit source (comment : comment) line_start floor ceiling =
  let span = comment.span in
  let html = comment.kind = HtmlComment in
  let interior = first_line_terminator source span in
  let tail = line_tail source span.finish in
  let head_code = has_code source line_start span.start in
  let ends_the_line = tail <> None || (interior <> None && not html) in
  let start =
    if ends_the_line then blank_start source span.start (max floor line_start)
    else span.start in
  let eats_the_terminator =
    if html then not head_code else interior <> None || not head_code in
  let finish = match tail with
    | Some (blanks, terminator) ->
      blanks + (if eats_the_terminator then terminator else 0)
    | None -> span.finish in
  let replacement =
    if html then Bytes.empty
    (* NOTE: An interior comment: the line goes on after it, so keeping the two
       tokens either side apart is the whole story, exactly as under `lines`. *)
    else if not ends_the_line then line_replacement source comment.kind span
    else match interior with
    (* NOTE: The code before the comment keeps its own line, and the terminator
       that ended that line was inside the comment. *)
    | Some terminator when head_code -> terminator
    (* NOTE: Nothing that survives on this line follows the comment, so the
       line terminator - the one kept after it or the one that ended the code
       line - is separator enough. *)
    | _ -> Bytes.empty in
  { span = { start; finish = min finish ceiling }; replacement }

(* NOTE: The edits layout `compact` makes: layout `lines`, plus the promise
   that a line which held nothing but a removed comment goes away instead of
   staying behind as a blank one.

   Whether a comment was alone on its line is judged from the bytes of the
   original source, so a line holding two comments and nothing else keeps its
   terminator: neither of them was alone on it.

   The start of the current line is tracked forward through the whole source,
   comment bodies included, so a comment beginning on a line that an earlier
   comment ended is still measured from that line's real beginning. *)
(* NOTE: `swallowed` names the lines whose hole would carry meaning, and it
   reaches further than a line: under a "|+" body it takes the empty lines the
   comment was sheltering too (see `lines_a_removal_must_swallow`).  Taking the
   line is what `compact` does anyway, so this only ever widens what it takes,
   and it is what keeps all three layouts writing the same bytes there. *)
let compact_edits source comments swallowed =
  let rec loop index scan line_start floor edits = function
    | [] -> List.rev edits
    | (comment : comment) :: tail -> match comment.disposition with
      | Keep _ -> loop (index + 1) scan line_start floor edits tail
      | Remove -> match swallowed index with
      | Some (line : byte_span) ->
        let span = { start = max line.start floor; finish = max line.finish floor } in
        loop (index + 1) span.finish span.finish span.finish
          ({ span; replacement = Bytes.empty } :: edits) tail
      | None ->
        let rec advance scan line_start =
          if scan >= comment.span.start then (scan, line_start)
          else match unicode_line_terminator_width source scan with
          | Some width when scan + width <= comment.span.start ->
            advance (scan + width) (scan + width)
          | _ -> advance (scan + 1) line_start in
        let scan, line_start = advance scan line_start in
        (* NOTE: The next comment of any disposition, kept ones included: the
           blanks an edit swallows must never reach into one. *)
        let ceiling = max comment.span.finish
          (match tail with next :: _ -> next.span.start | [] -> Bytes.length source) in
        let edit = compact_edit source comment line_start floor ceiling in
        loop (index + 1) scan line_start edit.span.finish (edit :: edits) tail
  in loop 0 0 0 0 [] comments

(* PERF:
   Column state is threaded between edits so every source byte is inspected at
   most once.  This also reflects an explicitly removed HTML comment: because
   that edit emits no bytes, its original newlines do not affect later edits.
*)
let column_edit source cursor column (comment : comment) =
  let column = advance_display_column source cursor comment.span.start column in
  if comment.kind = HtmlComment then
    ({ span = comment.span; replacement = Bytes.empty }, column)
  else
    let replacement, column = column_replacement source comment.span column in
    ({ span = comment.span; replacement }, column)

let apply_edits source edits =
  let output_length = List.fold_left (fun length edit -> length - (edit.span.finish - edit.span.start) + Bytes.length edit.replacement) (Bytes.length source) edits in
  let output = Bytes.create output_length in
  let source_cursor = ref 0 and output_cursor = ref 0 in
  List.iter (fun edit ->
    if edit.span.start < !source_cursor || edit.span.finish < edit.span.start || edit.span.finish > Bytes.length source then invalid_arg "invalid edit contract";
    let unchanged = edit.span.start - !source_cursor in
    Bytes.blit source !source_cursor output !output_cursor unchanged; output_cursor := !output_cursor + unchanged;
    Bytes.blit edit.replacement 0 output !output_cursor (Bytes.length edit.replacement);
    output_cursor := !output_cursor + Bytes.length edit.replacement; source_cursor := edit.span.finish
  ) edits;
  Bytes.blit source !source_cursor output !output_cursor (Bytes.length source - !source_cursor); output

let source_map source_length edits =
  let rec loop original output segments = function
    | [] ->
      if original < source_length || segments = [] then
        List.rev ({ original = { start = original; finish = source_length };
          output = { start = output; finish = output + source_length - original }; exact = true } :: segments)
      else List.rev segments
    | edit :: tail ->
      let segments, output = if original < edit.span.start then
        ({ original = { start = original; finish = edit.span.start };
           output = { start = output; finish = output + edit.span.start - original }; exact = true } :: segments,
         output + edit.span.start - original) else (segments, output) in
      let replacement_finish = output + Bytes.length edit.replacement in
      loop edit.span.finish replacement_finish
        ({ original = edit.span; output = { start = output; finish = replacement_finish }; exact = false } :: segments) tail
  in loop 0 0 [] edits

(* NOTE: Every block scalar in a YAML source, in order.

   A scan of its own, so that the answer stays a function of the bytes alone and
   an incremental rescan or an external hand-off reaches the same one with no
   state to carry.  It is the scanner's reading of a header, not a loose one:
   "key: a |+" ends a plain scalar with two characters that look like a header,
   and reading it as one would hang a phantom keep-chomped tail off a line that
   has no body at all. *)
let yaml_block_scalars source =
  let accumulator = { comments_rev = []; diagnostics_rev = []; yaml_blocks_rev = [] } in
  scan_yaml source Yaml default_scan_options accumulator;
  List.rev accumulator.yaml_blocks_rev

(* NOTE: Whether nothing but indentation stands between "start" and the beginning
   of its line, which is the whole of what makes a comment a candidate for being
   swallowed whole. *)
let starts_its_line source start =
  let rec loop index =
    if index <= 0 then true
    else match Bytes.get source (index - 1) with
      | ' ' | '\t' -> loop (index - 1)
      | '\r' | '\n' -> true
      | _ -> false
  in loop start

(* NOTE: Whether the source holds a byte that could head a block scalar at all. *)
let holds_a_block_indicator source =
  let length = Bytes.length source in
  let rec loop index =
    index < length &&
    (match Bytes.get source index with '|' | '>' -> true | _ -> loop (index + 1))
  in loop 0

(* NOTE: For each comment, the line a removal has to take whole -- its
   terminator included -- instead of leaving the ordinary hole on it, or None
   where the ordinary hole is right.  An empty answer stands for all-None, which
   is every language but YAML and nearly every YAML file.

   YAML is the one language where the hole itself carries meaning, and the reason
   is that a block scalar decides where its body ends from the lines below it
   (YAML 1.2.2, 8.1.1).  A whole-line comment under a body is
   "l-trail-comments" and is not part of the value, but the hole left in its
   place is read as one of two things: a line of spaces as wide as the comment,
   which `columns` writes, is indented at least as deep as the body whenever the
   comment was wide enough -- and a line indented that deep is body content, so
   the scalar silently grows a line; and an empty line, which `lines` writes, is
   content under "|+" and ">+", where every empty line trailing a body is kept
   (8.1.1.2).

   So every whole-line comment whose own line sits in the run of empty and
   comment lines under a body is removed by taking the line, terminator and all,
   under every layout -- which is the line `compact` takes already.  That costs
   those lines their numbering under `lines` and their columns under `columns`;
   the alternative costs the reader's value, and no indentation a padded line
   could be given is safe, because the depth that would put it outside one body
   is the depth that puts it inside the mapping the body belongs to.

   Under "|+" and ">+" the line is not enough on its own.  The empty lines
   between a removed comment and the next line are "l-comment" while the comment
   shelters them and "l-keep-empty" once it is gone (8.1.1.2), so the swallow
   runs on through them.  The empty lines above the first comment are already
   content and are left exactly where they are: a removal takes what the comment
   was sheltering and nothing else.

   The answer is a function of the source and the comments alone, so an
   incremental rescan and an external hand-off reach the same one with no state
   to carry. *)
let lines_a_removal_must_swallow source language comments =
  if language <> Yaml || comments = [] then [||]
  (* PERF: Two answers that cost almost nothing, in front of a scan of the whole
     source.  A file with no "|" and no ">" in it has no block scalar at all; and
     a comment a body could swallow is one that is alone on its line, which is a
     walk back over that line's indentation and no further -- the "# note" of
     "key: value # note" stops on the byte behind it.  A YAML file whose comments
     all trail something therefore never pays for the scan below. *)
  else if not (holds_a_block_indicator source) then [||]
  else if not (List.exists (fun (comment : comment) ->
    comment.disposition = Remove && starts_its_line source comment.span.start) comments)
  then [||]
  else
    let blocks = yaml_block_scalars source in
    if blocks = [] then [||]
    else begin
      let comments = Array.of_list comments in
      let answers = Array.make (Array.length comments) None in
      let length = Bytes.length source in
      List.iter (fun block ->
        let rec loop probe =
          if probe >= length then ()
          else
            let _, blank, finish = yaml_line_shape source probe in
            if blank then
              (* NOTE: An empty line neither ends the run nor is taken on its
                 own: it is content of the body above until a comment below it is
                 removed, and only that removal may take it. *)
              loop (past_terminator source finish)
            else
              match comment_alone_on_line source comments probe finish with
              (* NOTE: The first line with anything else on it is the next node,
                 and the comments under it are that node's. *)
              | None -> ()
              | Some found ->
                (if (comments.(found) : comment).disposition = Remove then begin
                   let rec run taken =
                     if not block.keeps_empties || taken >= length then taken
                     else
                       let _, blank, run_finish = yaml_line_shape source taken in
                       if blank then run (past_terminator source run_finish) else taken in
                   let taken = run (past_terminator source finish) in
                   answers.(found) <- Some { start = probe; finish = taken }
                 end);
                loop (past_terminator source finish)
        in loop block.body_end) blocks;
      answers
    end

(* NOTE: Apply `yaml_structural_trail_keeps` to comments that did not come from a
   scan of this reference's own, which is the external hand-off of
   `transform_spans`.  A scan reaches the same answer from the blocks it already
   walked over; this is that answer re-derived from the bytes, so the two paths
   cannot disagree about a value. *)
let keep_yaml_structural_trails source language comments =
  (* PERF: The same two answers `lines_a_removal_must_swallow` opens with: no "|"
     and no ">" is no block scalar, and a file whose comments all trail something
     has no whole-line comment to weigh. *)
  if language <> Yaml || comments = [] then comments
  else if not (holds_a_block_indicator source) then comments
  else if not (List.exists (fun (comment : comment) ->
    comment.disposition = Remove && starts_its_line source comment.span.start) comments)
  then comments
  else
    let comments = Array.of_list comments in
    List.iter (fun index ->
      comments.(index) <- { (comments.(index)) with disposition = Keep yaml_structural_trail })
      (yaml_structural_trail_keeps source (yaml_block_scalars source) comments);
    Array.to_list comments

let transform_report source report options =
  let edits =
    if not report.valid && not options.scan.force_invalid then []
    else
    (* NOTE: The one hole whose own bytes carry meaning, so the layouts that
       leave a line behind have to be told where not to.  `compact` takes the
       line already. *)
    let swallow = lines_a_removal_must_swallow source report.language report.comments in
    let swallowed index = if index < Array.length swallow then swallow.(index) else None in
    match options.layout with
    | Columns ->
      let rec loop index cursor column edits = function
        | [] -> List.rev edits
        | (comment : comment) :: tail -> (match comment.disposition with
          | Keep _ -> loop (index + 1) cursor column edits tail
          | Remove -> match swallowed index with
            (* NOTE: A swallowed line takes its terminator with it, so what
               follows starts a line of its own in the output as it did in the
               source and the column count begins again there. *)
            | Some line ->
              let span = { start = max line.start cursor; finish = line.finish } in
              loop (index + 1) span.finish 0 ({ span; replacement = Bytes.empty } :: edits) tail
            | None ->
              let edit, column = column_edit source cursor column comment in
              loop (index + 1) comment.span.finish column (edit :: edits) tail)
      in loop 0 0 0 [] report.comments
    | Lines ->
      let rec loop index floor edits = function
        | [] -> List.rev edits
        | (comment : comment) :: tail -> (match comment.disposition with
          | Keep _ -> loop (index + 1) floor edits tail
          | Remove ->
            let edit = match swallowed index with
              | Some line ->
                { span = { start = max line.start floor; finish = line.finish };
                  replacement = Bytes.empty }
              | None ->
                { span = comment.span;
                  replacement = line_replacement source comment.kind comment.span } in
            loop (index + 1) edit.span.finish (edit :: edits) tail)
      in loop 0 0 [] report.comments
    | Compact -> compact_edits source report.comments swallowed
  in
  { output = apply_edits source edits; edits; report; source_map = source_map (Bytes.length source) edits }

let transform source language options =
  transform_report source (scan source language options.scan) options

let transform_profile source profile options =
  match scan_profile source profile options.scan with
  | Result.Error _ as error -> error
  | Result.Ok report -> Result.Ok (transform_report source report options)

let transform_spans source language spans options =
  let rec validate cursor index comments = function
    | [] -> Result.Ok (List.rev comments)
    | (span, kind) :: tail ->
      if span.start > span.finish || span.finish > Bytes.length source then
        Result.Error (Printf.sprintf "external comment #%d is outside the %d-byte source"
          index (Bytes.length source))
      else if span.start = span.finish then
        Result.Error (Printf.sprintf "external comment #%d has an empty span" index)
      else if index > 0 && span.start < cursor then
        Result.Error (Printf.sprintf "external comment #%d is out of order or overlaps its predecessor" index)
      else let raw = Bytes.sub_string source span.start (span.finish - span.start) in
        let comment = { span; kind; disposition = disposition options.scan kind raw } in
        validate span.finish (index + 1) (comment :: comments) tail
  in
  match validate 0 0 [] spans with
  | Result.Error _ as error -> error
  | Result.Ok comments ->
    (* NOTE: The one verdict a comment's own bytes cannot reach, so it is applied
       to the hand-off as a built-in scan applies it: a YAML block scalar leaning
       on the comment that ends it keeps that comment, whoever found it.  Without
       this the report would promise a removal that
       `lines_a_removal_must_swallow` cannot make safe. *)
    let comments = keep_yaml_structural_trails source language comments in
    Result.Ok (transform_report source
      { language; comments; diagnostics = []; valid = true } options)
