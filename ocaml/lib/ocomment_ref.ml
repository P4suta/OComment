type language =
  | Rust | Ocaml | C | Cpp | Go | Java | JavaScript | TypeScript | Python
  | Shell | Html | Css | Jsonc | Sql | Kotlin | Unknown

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
  | "kotlin" | "kt" | "kts" -> Ok Kotlin
  | other -> Error ("unsupported language `" ^ other ^ "`")

let string_of_language = function
  | Rust -> "rust" | Ocaml -> "ocaml" | C -> "c" | Cpp -> "cpp" | Go -> "go"
  | Java -> "java" | JavaScript -> "javascript" | TypeScript -> "typescript"
  | Python -> "python" | Shell -> "shell" | Html -> "html" | Css -> "css"
  | Jsonc -> "jsonc" | Sql -> "sql" | Kotlin -> "kotlin" | Unknown -> "unknown"

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

let trim_markers raw =
  let markers = ["<!--"; "///"; "//!"; "//"; "/**"; "/*"; "(*"; "--"; "#"] in
  let endings = ["-->"; "*/"; "*)"] in
  let start = match List.find_opt (fun marker -> String.starts_with ~prefix:marker raw) markers with
    | Some marker -> String.length marker | None -> 0 in
  let finish = match List.find_opt (fun marker -> String.ends_with ~suffix:marker raw) endings with
    | Some marker -> String.length raw - String.length marker | None -> String.length raw in
  String.sub raw start (max 0 (finish - start)) |> String.trim |> lowercase

let is_legal text =
  List.exists (contains text) ["spdx-license-identifier"; "copyright"; "licensed under";
    "permission is hereby granted"; "all rights reserved"]

(* NOTE: A directive named after the tool that reads it is followed by the
   argument that tool takes, and whitespace of the writer's choosing separates
   the two, so the keyword ends at a boundary rather than at one particular
   byte. Matching the bare prefix would read prose that merely opens with those
   letters -- "# shellcheckish note" -- as an instruction as well. *)
let opens_with_keyword compact keyword =
  let length = String.length keyword in
  String.starts_with ~prefix:keyword compact &&
  String.length compact > length &&
  (match compact.[length] with
   | ' ' | '\t' | '\n' | '\012' | '\r' -> true
   | _ -> false)

let is_directive language text raw =
  let compact = String.trim text |> String.to_seq |>
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

let python_encoding_declaration source start raw =
  if not (within_first_two_lines source start) || not (String.starts_with ~prefix:"#" raw)
  then false else
  let rec find_line_start index =
    if index = 0 then 0
    else if Bytes.get source (index - 1) = '\r' || Bytes.get source (index - 1) = '\n'
    then index else find_line_start (index - 1) in
  let line_start = find_line_start start in
  let prefix_start = if line_start = 0 && starts source 0 "\xef\xbb\xbf" then 3 else line_start in
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
  if start = 0 && String.starts_with ~prefix:"#!" raw then Shebang
  else if language = Python && python_encoding_declaration source start raw then Encoding
  else if language = Sql && String.starts_with ~prefix:"/*+" raw then OptimizerHint
  else if language = Sql && String.starts_with ~prefix:"/*!" raw then VersionComment
  else if is_legal text then License else if is_directive language text raw then Directive else lexical

type accumulator = { mutable comments_rev : comment list; mutable diagnostics_rev : diagnostic list }

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
      if String.length delimiter > 16 || String.exists
        (fun character -> Char.code character <= 32 || character = '\\' || character = ')') delimiter
      then None
      else let closing = ")" ^ delimiter ^ "\"" in
        match find_from source (opening + 1) closing with
        | Some finish -> Some (finish + String.length closing, true)
        | None -> Some (Bytes.length source, false))

let c_quote_start source index =
  let length = Bytes.length source in
  if index >= length then None
  else if Bytes.get source index = '"' || Bytes.get source index = '\'' then Some index
  else if String.contains "LuU" (Bytes.get source index) && index + 1 < length &&
    (Bytes.get source (index + 1) = '"' || Bytes.get source (index + 1) = '\'')
  then Some (index + 1)
  else if (starts source index "u8\"" || starts source index "u8'") then Some (index + 2)
  else None

let rust_raw_end source start =
  let cursor = ref start in
  if !cursor < Bytes.length source &&
    (Bytes.get source !cursor = 'b' || Bytes.get source !cursor = 'c') then incr cursor;
  if !cursor >= Bytes.length source || Bytes.get source !cursor <> 'r' then None else begin
    incr cursor; let hashes = ref 0 in
    while !cursor < Bytes.length source && Bytes.get source !cursor = '#' do incr cursor; incr hashes done;
    if !cursor >= Bytes.length source || Bytes.get source !cursor <> '"' then None else
    let ending = "\"" ^ String.make !hashes '#' in
    match find_from source (!cursor + 1) ending with Some close -> Some (close + String.length ending) | None -> Some (Bytes.length source)
  end

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
    end else match if language = Rust then rust_raw_end source index else None with
      | Some finish -> loop finish
      | None ->
        let character = Bytes.get source index in
        if language = Cpp then begin
          match cpp_raw_end source index with
          | Some (finish, closed) ->
            if not closed then add_error accumulator "unterminated-string"
              "unterminated C++ raw string" index finish;
            loop finish
          | None -> (match c_quote_start source index with
            | Some quote ->
              let finish, closed = quoted_end source quote false in
              if not closed then add_error accumulator "unterminated-string"
                "unterminated string or character literal" quote finish;
              loop finish
            | None -> loop (index + 1))
        end else if language = C then begin
          match c_quote_start source index with
          | Some quote ->
            let finish, closed = quoted_end source quote false in
            if not closed then add_error accumulator "unterminated-string"
              "unterminated string or character literal" quote finish;
            loop finish
          | None -> loop (index + 1)
        end else if language = Go && character = '`' then begin
          match find_from source (index + 1) "`" with Some finish -> loop (finish + 1)
          | None -> add_error accumulator "unterminated-string" "unterminated raw string" index (Bytes.length source)
        end else if language = Kotlin && starts source index "\"\"\"" then begin
          loop (scan_kotlin_string source options accumulator index true 0)
        end else if language = Kotlin && character = '"' then begin
          loop (scan_kotlin_string source options accumulator index false 0)
        end else if character = '"' || character = '\'' then begin
          (* INVARIANT: a Rust string or byte-string literal carries a bare
             newline as content, so only its closing quote or the end of the
             file ends one; a Rust character literal still ends at the line. *)
          let multiline = language = Css || (language = Rust && character = '"') in
          let finish, closed = quoted_end source index multiline in
          if not closed then add_error accumulator "unterminated-string" "unterminated literal" index finish;
          loop finish
        end else loop (index + 1)
  in loop 0

let scan_slash source language options accumulator =
  if (language = C || language = Cpp) &&
    (find_from source 0 "\\\n" <> None || find_from source 0 "\\\r\n" <> None)
  then begin
    let mapping = without_c_line_splices source in
    let child = { comments_rev = []; diagnostics_rev = [] } in
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
  let mapped = mapping.mapped and child = { comments_rev = []; diagnostics_rev = [] } in
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

let js_identifier_start character =
  (character >= 'a' && character <= 'z') || (character >= 'A' && character <= 'Z') ||
  character = '_' || character = '$' || Char.code character land 0x80 <> 0

let js_identifier_continue character =
  js_identifier_start character || (character >= '0' && character <= '9')

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

let js_html_close_comment source index =
  starts source index "-->" &&
  let rec line_start cursor =
    if cursor = 0 then 0 else
    match Bytes.get source (cursor - 1) with
    | '\r' | '\n' -> cursor
    | _ -> line_start (cursor - 1) in
  let start = line_start index in
  let start = if start = 0 && starts source start "\xef\xbb\xbf" then 3 else start in
  let rec whitespace cursor =
    cursor = index ||
    (String.contains " \t\011\012" (Bytes.get source cursor) && whitespace (cursor + 1)) in
  whitespace start

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
    end else if index = 0 && starts source index "#!" then begin
      let finish = js_line_end source (index + 2) in
      add_comment accumulator source language options Line index finish;
      loop finish brace_depth regex_allowed control_parentheses pending_control
        brace_blocks statement_start pending_block
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
    | character when Char.code character <= 32 ->
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
                if value > index && Char.code (Bytes.get source (value - 1)) <= 32
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

let scan_javascript source language options accumulator =
  ignore (scan_js_code source language options accumulator 0 None 0)

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
    else if Bytes.get source index = '\'' && index + 2 < Bytes.length source &&
      (Bytes.get source (index + 2) = '\'' || Bytes.get source (index + 1) = '\\') then
      let finish, _ = quoted_end source index false in comment_end finish depth
    else comment_end (index + 1) depth in
  let rec loop index =
    if index >= Bytes.length source then ()
    else if starts source index "(*" then begin
      let finish, closed = comment_end (index + 2) 1 in
      add_comment accumulator source language options (if starts source index "(**" then DocBlock else Block) index finish;
      if not closed then add_error accumulator "unterminated-comment" "unterminated OCaml comment" index finish;
      loop finish
    end else if Bytes.get source index = '"' then let finish, closed = quoted_end source index true in
      if closed then loop finish else add_error accumulator "unterminated-string" "unterminated OCaml string" index finish
    else if Bytes.get source index = '\'' && index + 2 < Bytes.length source &&
      (Bytes.get source (index + 2) = '\'' ||
        (Bytes.get source (index + 1) = '\\' &&
          let rec has_quote cursor remaining = remaining > 0 && cursor < Bytes.length source &&
            (Bytes.get source cursor = '\'' || has_quote (cursor + 1) (remaining - 1)) in
          has_quote (index + 2) 6))
    then let finish, closed = quoted_end source index false in
      if closed then loop finish else add_error accumulator "unterminated-string"
        "unterminated OCaml character literal" index finish
    else if Bytes.get source index = '{' then
      (match ocaml_quoted_end source index with
      | Some (finish, true) -> loop finish
      | Some (finish, false) -> add_error accumulator "unterminated-string"
          "unterminated OCaml quoted string" index finish
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

type heredoc = { operator : int; delimiter : bytes; strip_tabs : bool }

let consume_newline source index =
  if starts source index "\r\n" then index + 2 else index + 1

let parse_heredoc source index =
  let strip_tabs = index + 2 < Bytes.length source && Bytes.get source (index + 2) = '-' in
  let cursor = ref (index + if strip_tabs then 3 else 2) in
  while !cursor < Bytes.length source && Char.code (Bytes.get source !cursor) <= 32 &&
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
    | None -> let byte = Bytes.get source !cursor in
      Char.code byte > 32 && not (String.contains ";|&()<>" byte))
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
    else if Char.code (Bytes.get source index) <= 32 then
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
        Char.code character <= 32 || String.contains ";&|()<>" character in
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

let rec scan_html source language options accumulator =
  let tag_boundary = function None -> true | Some character ->
    Char.code character <= 32 || character = '>' || character = '/' in
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
        merge content_start (scan child_source embedded options);
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
  let accumulator = { comments_rev = []; diagnostics_rev = [] } in
  (match language with
  | Rust | C | Cpp | Go | Kotlin | Css | Jsonc -> scan_slash source language options accumulator
  | Java -> scan_java source language options accumulator
  | JavaScript | TypeScript -> scan_javascript source language options accumulator
  | Ocaml -> scan_ocaml source language options accumulator
  | Python -> scan_python source language options accumulator
  | Shell -> scan_shell source language options accumulator
  | Sql -> scan_sql source language options accumulator
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
    let accumulator = { comments_rev = []; diagnostics_rev = [] } in
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
            Char.code (Bytes.get source (index - 1)) <= 32)) profile.line_comments with
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

let ascii_whitespace = function
  | ' ' | '\t' | '\n' | '\r' | '\011' | '\012' -> true
  | _ -> false

let replacement source layout kind span =
  if kind = HtmlComment then Bytes.empty else match layout with
  | Columns -> invalid_arg "column replacement requires tracked display state"
  | Lines | Compact ->
    let output = newline_bytes source span in
    if Bytes.length output > 0 then output
    else if span.start > 0 && span.finish < Bytes.length source &&
      not (ascii_whitespace (Bytes.get source (span.start - 1))) &&
      not (ascii_whitespace (Bytes.get source span.finish))
    then Bytes.of_string " " else Bytes.empty

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

let transform_report source report options =
  let edits =
    if not report.valid && not options.scan.force_invalid then []
    else match options.layout with
    | Columns ->
      let rec loop cursor column edits = function
        | [] -> List.rev edits
        | comment :: tail -> (match comment.disposition with
          | Keep _ -> loop cursor column edits tail
          | Remove ->
            let edit, column = column_edit source cursor column comment in
            loop comment.span.finish column (edit :: edits) tail)
      in loop 0 0 [] report.comments
    | Lines | Compact ->
      List.filter_map (fun comment -> match comment.disposition with Keep _ -> None | Remove ->
        Some { span = comment.span;
          replacement = replacement source options.layout comment.kind comment.span }) report.comments
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
  | Result.Ok comments -> Result.Ok (transform_report source
      { language; comments; diagnostics = []; valid = true } options)
