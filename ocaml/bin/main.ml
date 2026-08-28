open Ocomment_ref

let base64_table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

let base64_decode input =
  let value character =
    match String.index_opt base64_table character with Some index -> index | None -> -1 in
  let cleaned = String.to_seq input |> Seq.filter (fun character -> character <> '\r' && character <> '\n' && character <> ' ') |> String.of_seq in
  let output = Buffer.create (String.length cleaned * 3 / 4) in
  let rec loop index =
    if index >= String.length cleaned then () else
    let a = value cleaned.[index] and b = value cleaned.[index + 1] in
    let c = if index + 2 < String.length cleaned && cleaned.[index + 2] <> '=' then value cleaned.[index + 2] else -1 in
    let d = if index + 3 < String.length cleaned && cleaned.[index + 3] <> '=' then value cleaned.[index + 3] else -1 in
    if a < 0 || b < 0 then invalid_arg "invalid base64";
    Buffer.add_char output (Char.chr ((a lsl 2) lor (b lsr 4)));
    if c >= 0 then Buffer.add_char output (Char.chr (((b land 15) lsl 4) lor (c lsr 2)));
    if d >= 0 then Buffer.add_char output (Char.chr (((c land 3) lsl 6) lor d));
    loop (index + 4)
  in loop 0; Bytes.of_string (Buffer.contents output)

let base64_encode bytes =
  let output = Buffer.create ((Bytes.length bytes + 2) / 3 * 4) in
  let rec loop index =
    if index >= Bytes.length bytes then () else
    let a = Char.code (Bytes.get bytes index) in
    let b = if index + 1 < Bytes.length bytes then Char.code (Bytes.get bytes (index + 1)) else 0 in
    let c = if index + 2 < Bytes.length bytes then Char.code (Bytes.get bytes (index + 2)) else 0 in
    Buffer.add_char output base64_table.[a lsr 2];
    Buffer.add_char output base64_table.[((a land 3) lsl 4) lor (b lsr 4)];
    Buffer.add_char output (if index + 1 < Bytes.length bytes then base64_table.[((b land 15) lsl 2) lor (c lsr 6)] else '=');
    Buffer.add_char output (if index + 2 < Bytes.length bytes then base64_table.[c land 63] else '=');
    loop (index + 3)
  in loop 0; Buffer.contents output

let span_json (span : byte_span) = `Assoc ["start", `Int span.start; "end", `Int span.finish]
let disposition_json = function Remove -> `Assoc ["action", `String "remove"] | Keep reason -> `Assoc ["action", `String "keep"; "reason", `String reason]
let comment_json (comment : comment) = `Assoc ["span", span_json comment.span; "kind", `String (string_of_comment_kind comment.kind); "disposition", disposition_json comment.disposition]
let severity_string = function Error -> "error" | Warning -> "warning" | Info -> "info" | Hint -> "hint"
let diagnostic_json (diagnostic : diagnostic) = `Assoc ["code", `String diagnostic.code; "message", `String diagnostic.message;
  "severity", `String (severity_string diagnostic.severity); "span", span_json diagnostic.span]
let edit_json (edit : edit) = `Assoc ["span", span_json edit.span; "replacement_base64", `String (base64_encode edit.replacement)]
let source_map_json (segment : source_map_segment) = `Assoc ["original", span_json segment.original; "output", span_json segment.output; "exact", `Bool segment.exact]

let scan_json report = `Assoc ["language", `String (string_of_language report.language);
  "comments", `List (List.map comment_json report.comments); "diagnostics", `List (List.map diagnostic_json report.diagnostics); "valid", `Bool report.valid]

let transform_json result = `Assoc [
  "output_base64", `String (base64_encode result.output);
  "edits", `List (List.map edit_json result.edits);
  "report", scan_json result.report;
  "source_map", `List (List.map source_map_json result.source_map)]

let member_string name json = Yojson.Safe.Util.member name json |> Yojson.Safe.Util.to_string

let comment_kind_of_string = function
  | "line" -> Line | "block" -> Block | "doc-line" -> DocLine
  | "doc-block" -> DocBlock | "directive" -> Directive | "license" -> License
  | "html-comment" -> HtmlComment | "shebang" -> Shebang | "encoding" -> Encoding
  | "optimizer-hint" -> OptimizerHint | "version-comment" -> VersionComment
  | value -> failwith ("unknown comment kind `" ^ value ^ "`")

let dialect_of_string = function
  | "standard" -> Standard | "jsx" -> Jsx | "tsx" -> Tsx
  | "objective-c" -> ObjectiveC | "objective-cpp" -> ObjectiveCpp
  | "gnu-c" -> GnuC | "gnu-cpp" -> GnuCpp | "cuda" -> Cuda
  | "posix-sh" -> PosixSh | "bash53" -> Bash53 | "zsh" -> Zsh
  | "postgresql" -> PostgreSql | "mysql" -> MySql | "sqlite" -> Sqlite
  | "t-sql" -> TSql | "oracle" -> Oracle | "scss" -> Scss
  | value -> failwith ("unknown dialect `" ^ value ^ "`")

let strings name json =
  match Yojson.Safe.Util.member name json with
  | `Null -> []
  | value -> Yojson.Safe.Util.to_list value |> List.map Yojson.Safe.Util.to_string

let external_spans json =
  match Yojson.Safe.Util.member "spans" json with
  | `Null -> []
  | value -> Yojson.Safe.Util.to_list value |> List.map (fun item ->
      let start = Yojson.Safe.Util.member "start" item |> Yojson.Safe.Util.to_int in
      let finish = Yojson.Safe.Util.member "end" item |> Yojson.Safe.Util.to_int in
      let kind = Yojson.Safe.Util.member "kind" item |> Yojson.Safe.Util.to_string |>
        comment_kind_of_string in
      ({ start; finish }, kind))

let edits_of_json source_length json =
  let edits = match Yojson.Safe.Util.member "edits" json with
    | `Null -> failwith "missing edits"
    | value -> Yojson.Safe.Util.to_list value |> List.map (fun item ->
      let span = Yojson.Safe.Util.member "span" item in
      let start = Yojson.Safe.Util.member "start" span |> Yojson.Safe.Util.to_int in
      let finish = Yojson.Safe.Util.member "end" span |> Yojson.Safe.Util.to_int in
      let replacement = member_string "replacement_base64" item |> base64_decode in
      ({ span = { start; finish }; replacement } : edit)) in
  let rec validate cursor index = function
    | [] -> edits
    | edit :: tail ->
      if edit.span.start < cursor || edit.span.finish < edit.span.start ||
        edit.span.finish > source_length
      then failwith (Printf.sprintf "invalid edit contract at edit %d" index)
      else validate edit.span.finish (index + 1) tail in
  validate 0 0 edits

let bool_or default name json =
  match Yojson.Safe.Util.member name json with `Bool value -> value | _ -> default

let string_or default name json =
  match Yojson.Safe.Util.member name json with `String value -> value | _ -> default

let list_or_empty name json =
  match Yojson.Safe.Util.member name json with
  | `Null -> [] | value -> Yojson.Safe.Util.to_list value

let profile_of_json json =
  let line_comments = list_or_empty "line_comments" json |> List.map (fun item ->
    ({ line_start = member_string "start" item;
       requires_boundary = bool_or false "requires_boundary" item;
       line_kind = comment_kind_of_string (string_or "line" "kind" item) } : line_delimiter)) in
  let block_comments = list_or_empty "block_comments" json |> List.map (fun item ->
    ({ block_start = member_string "start" item;
       block_end_token = member_string "end" item;
       nested = bool_or false "nested" item;
       block_kind = comment_kind_of_string (string_or "line" "kind" item) } : block_delimiter)) in
  let string_delimiters = list_or_empty "strings" json |> List.map (fun item ->
    ({ string_start = member_string "start" item;
       string_end = member_string "end" item;
       escape = (match Yojson.Safe.Util.member "escape" item with
         | `String value -> Some value | _ -> None);
       multiline = bool_or false "multiline" item } : string_delimiter)) in
  let protected_patterns = list_or_empty "protected_patterns" json |> List.map (fun item ->
    ({ pattern = member_string "contains" item; reason = member_string "reason" item }
      : protected_pattern)) in
  ({ name = member_string "name" json; extensions = strings "extensions" json;
     line_comments; block_comments; strings = string_delimiters; protected_patterns }
    : declarative_profile)

let options json =
  let policy = match Yojson.Safe.Util.member "policy" json with `String "all" -> All | `String "legal" -> Legal | _ -> Safe in
  let layout = match Yojson.Safe.Util.member "layout" json with `String "columns" -> Columns | `String "compact" -> Compact | _ -> Lines in
  let dialect = match Yojson.Safe.Util.member "dialect" json with `String value -> dialect_of_string value | _ -> Standard in
  let force_invalid = match Yojson.Safe.Util.member "force_invalid" json with `Bool value -> value | _ -> false in
  let force_protected = match Yojson.Safe.Util.member "force_protected" json with `Bool value -> value | _ -> false in
  let keep_kinds = strings "keep_kinds" json |> List.map comment_kind_of_string in
  let remove_kinds = strings "remove_kinds" json |> List.map comment_kind_of_string in
  let keep_regex = strings "keep_regex" json in
  let remove_regex = strings "remove_regex" json in
  ({ scan = { policy; dialect; force_invalid; force_protected; keep_kinds;
      remove_kinds; keep_regex; remove_regex }; layout } : transform_options)

let handle json =
  let id = Yojson.Safe.Util.member "id" json in
  try
    let operation = member_string "operation" json in
    let language = match language_of_string (member_string "language" json) with Ok value -> value | Error message -> failwith message in
    let source = base64_decode (member_string "source_base64" json) in
    let options = options (Yojson.Safe.Util.member "options" json) in
    let ok = match operation with
      | "apply_edits" ->
        let edits = edits_of_json (Bytes.length source) json in
        `Assoc ["output_base64", `String (base64_encode (apply_edits source edits))]
      | "scan" -> scan_json (scan source language options.scan)
      | "transform" -> transform_json (transform source language options)
      | "transform-spans" -> (match transform_spans source language (external_spans json) options with
          | Ok result -> transform_json result | Error message -> failwith message)
      | "scan-profile" -> (match scan_profile source
          (profile_of_json (Yojson.Safe.Util.member "profile" json)) options.scan with
          | Ok report -> scan_json report | Error message -> failwith message)
      | "transform-profile" -> (match transform_profile source
          (profile_of_json (Yojson.Safe.Util.member "profile" json)) options with
          | Ok result -> transform_json result | Error message -> failwith message)
      | _ -> failwith ("unsupported operation `" ^ operation ^ "`")
    in `Assoc ["id", id; "ok", ok]
  with exn -> `Assoc ["id", id; "error", `String (Printexc.to_string exn)]

let () =
  try while true do
    let line = input_line stdin in
    if String.trim line <> "" then (Yojson.Safe.from_string line |> handle |> Yojson.Safe.to_channel stdout; output_char stdout '\n'; flush stdout)
  done with End_of_file -> ()
