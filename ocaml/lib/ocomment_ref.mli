type language =
  | Rust | Ocaml | C | Cpp | Go | Java | JavaScript | TypeScript | Python
  | Shell | Html | Css | Jsonc | Sql | Kotlin | Toml | Lua | Yaml | Php | Ruby
  | Zig | R | Dart | Swift | CSharp | Scala | Vue | Svelte | Markdown | Perl | Unknown

(** Declared before `dialect` for the reason the implementation gives: both carry a `Standard`, and the dialect's is the one worth leaving unannotated. *)
(* NOTE: `RemoveNothing` rather than `None`, which is taken: a constructor by that name shadows `option`'s wherever this type is open.
   The name on the wire is still `none`. *)
type policy = RemoveNothing | Conservative | Standard | All

type dialect =
  | Standard | Jsx | Tsx | ObjectiveC | ObjectiveCpp | GnuC | GnuCpp | Cuda
  | PosixSh | Bash53 | Zsh | PostgreSql | MySql | Sqlite | TSql | Oracle | Scss
  | Sass

type byte_span = { start : int; finish : int }

type comment_kind =
  | Line | Block | DocLine | DocBlock | Directive | License | HtmlComment
  | Shebang | Encoding | OptimizerHint | VersionComment | LoadBearing

type protection = NoProtection | Preamble | LoadBearingTier

(** A rule about how a comment is written, as opposed to whether it stays. *)
type style_rule = Wrap | SpaceAfterMarker | TrailingWhitespace

val style_rule_name : style_rule -> string

(** Where the line breaks in a paragraph of comment prose go.
   The rule is about a run of comments rather than about one of them. *)
type wrap = Preserve | Unwrap | Sentence

val wrap_name : wrap -> string

(** What the run decided about one comment.
   Three-valued rather than two: a comment that stays and a comment that stays spelled differently are not the same outcome, and only one of them leaves the bytes alone.
   `Rewrite` carries its replacement, so what the report describes and what a fix writes cannot be computed twice and disagree. *)
type disposition = Remove | Keep of string | Rewrite of style_rule * bytes
type severity = Error | Warning | Info | Hint
type diagnostic = { code : string; message : string; severity : severity; span : byte_span }

(** A rule about a comment's shape rather than its kind, recorded because nothing can re-derive it from the comment's own bytes. *)
type shape_rule = Tagged of string | Trailing | TooLong of int * int

type comment =
  { span : byte_span; kind : comment_kind; disposition : disposition;
    shape : shape_rule option }
type layout = Lines | Columns | Compact

(** How a comment that survives is written.
   A sibling of `allow_rules` and not a field of it: a comment that fails one of those is removed, and a comment that fails one of these is rewritten. *)
type style_rules = {
  wrap : wrap;
  space_after_marker : bool option;
  trailing_whitespace : bool option;
}

(** What a comment has to be beyond being of a kind the policy keeps.
   The policy decides by kind, and a kind is a coarse thing to decide by: a one-line rationale and a forty-line essay are both Line.
   These are the other axes,
   and they cut across the policy rather than under it. *)
type allow_rules = {
  tags : string list;
  max_lines : int option;
  trailing : bool option;
  (* NOTE: Tags that carry a deadline.
     Allowed here exactly as `tags` are:
     measuring the age of a line means reading a repository, and neither this implementation nor the Rust scanner does any I/O, so the verdict that takes one back is reached by a caller with a clock.
     The names are still needed, because until the deadline passes these are ordinary allowed tags and the two implementations have to agree about that. *)
  expiring_tags : string list;
}

(** How strongly a protected pattern asks for its comment.
   The weaker tier records it as a directive that every policy but `all` keeps; the stronger one records it as a comment no policy reaches. *)
type protection_tier = Tool | ProfileLoadBearing

type protected_pattern =
  { pattern : string; reason : string; tier : protection_tier }

type scan_options = {
  policy : policy;
  dialect : dialect;
  force_invalid : bool;
  force_protected : bool;
  keep_kinds : comment_kind list;
  remove_kinds : comment_kind list;
  keep_regex : string list;
  remove_regex : string list;
  allow : allow_rules;
  style : style_rules;
  (* NOTE: Markers this project's own tools read.
     The catalogue this implementation ships knows the tools everybody uses and cannot know yours,
     and a `keep_regex` leaves the comment ordinary -- which `all` is entitled to remove.
     A pattern here decides what the comment is. *)
  protected : protected_pattern list;
}

type transform_options = { scan : scan_options; layout : layout }
(* NOTE: A run of comments on consecutive lines, and the bytes a style rule makes of it.
   Recorded against the run because the bytes it replaces are not any one comment's. *)
type comment_run = { run_span : byte_span; run_rule : style_rule; run_replacement : bytes }

type scan_report = { language : language; comments : comment list;
                     runs : comment_run list;
                     diagnostics : diagnostic list; valid : bool }
type edit = { span : byte_span; replacement : bytes }
type source_map_segment = { original : byte_span; output : byte_span; exact : bool }
type source_map = source_map_segment list
type transform_result = { output : bytes; edits : edit list; report : scan_report; source_map : source_map }

type line_delimiter = {
  line_start : string;
  requires_boundary : bool;
  (** Only open a comment when the token is the first byte of its line.
     A pattern list gives [#] that rule and only that rule: [file#name] names a file with one in it. *)
  requires_line_start : bool;
  (** Characters that, coming directly after the token, mean it does not open a comment after all.
     The mirror of [requires_boundary], which looks at the byte before.
     The token's final character may repeat before the test:
     Haskell's opener is a run of dashes, so [-- x] is a comment while [-->] is an operator and [---x] is a comment again. *)
  forbidden_after : string;
  line_kind : comment_kind;
}
type block_delimiter = { block_start : string; block_end_token : string; nested : bool; block_kind : comment_kind }
type string_delimiter = { string_start : string; string_end : string; escape : string option; multiline : bool }

(** `tier` is how strongly the pattern asks for the comment.
   A profile describes a syntax with no built-in scanner, and its author knows something the policy cannot: a marker their toolchain reads is not a marker their linter reads.
   Without it every profile protection was the weaker one and `all` took a marker a build depended on. *)
type declarative_profile = {
  name : string; extensions : string list; line_comments : line_delimiter list;
  block_comments : block_delimiter list; strings : string_delimiter list;
  protected_patterns : protected_pattern list;
  (** Whether an ordinary line comment directly below a documentation one continues it.
     Haddock marks only the first line and continues with the ordinary opener, so read one token at a time the rest is a remark. *)
  doc_continuation : bool;
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
