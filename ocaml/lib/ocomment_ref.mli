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

type line_delimiter = { line_start : string; requires_boundary : bool; line_kind : comment_kind }
type block_delimiter = { block_start : string; block_end_token : string; nested : bool; block_kind : comment_kind }
type string_delimiter = { string_start : string; string_end : string; escape : string option; multiline : bool }
type protected_pattern = { pattern : string; reason : string }
type declarative_profile = {
  name : string; extensions : string list; line_comments : line_delimiter list;
  block_comments : block_delimiter list; strings : string_delimiter list;
  protected_patterns : protected_pattern list;
}

val default_scan_options : scan_options
val default_transform_options : transform_options
val language_of_string : string -> (language, string) result
val string_of_language : language -> string
val string_of_comment_kind : comment_kind -> string
val scan : bytes -> language -> scan_options -> scan_report
val validate_profile : declarative_profile -> (unit, string) result
val scan_profile : bytes -> declarative_profile -> scan_options -> (scan_report, string) result
val transform : bytes -> language -> transform_options -> transform_result
val transform_profile : bytes -> declarative_profile -> transform_options ->
  (transform_result, string) result
val transform_spans : bytes -> language -> (byte_span * comment_kind) list ->
  transform_options -> (transform_result, string) result
val apply_edits : bytes -> edit list -> bytes
