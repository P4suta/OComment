# Reading a report

Three readers ask three different questions of the same run, and there is a format for each.

| | who | what it answers |
|---|---|---|
| `review` | a person, deciding | what to do about these, and how many of each |
| `human` | a pipeline, counting | where each one is, one line at a time |
| `agent` | a program, acting | the edit, and the command to run next |

`review` is the default, in a pipe as much as on a terminal.
That is deliberate and it is not what most tools do: a person on a screen and an agent reading the same run through a pipe are in one conversation about one report, and a format that changes shape between them leaves each arguing from something the other cannot see.
Colour still follows the terminal, because colour is the one thing here that carries no meaning of its own.

## `review`

```console
$ ocomment check
  NO  5 comments in 1 file · 1 file scanned · policy conservative

  DECIDE  make it a documentation comment                 2 comments
    src/budget.rs:3-4
      - // The retry budget is per connection, not per request, because the
      - // server counts attempts against the socket it sees.
      + /// The retry budget is per connection, not per request, because the
      + /// server counts attempts against the socket it sees.
        pub struct Budget {
    or keep them  [policy.allow]
                  tags = ["NOTE"]

  DECIDE  do what it promises, or delete it                1 comment
    src/budget.rs:9
      - // TODO: make this configurable
    or keep them  [policy.allow]
                  tags = ["TODO"]

  ALLOWED 1 comment this run did not report; `--explain` names the rule that kept each
```

Findings are grouped by the decision they ask for rather than by the rule that produced them, because five comments under one rule are not five questions.
They are one question asked five times, and the answer to each is decided by the code the comment sits on — which is why the code is there.

Adjacent comment lines are one finding.
A comment beside code never is: forty trailing notes on forty assignments are forty decisions, one per statement.

**`or keep them` is the other half of every decision.** A gate that can only say "delete it" is one somebody turns off the first time it is wrong about a single comment.
What it offers is a *setting* rather than a flag — a flag makes one run pass and a setting is a decision the repository keeps.
It will not offer you the shortest path to a green run; a gate that names the flag which silences it, at the moment it fires, is arguing against its own finding.

**`ALLOWED` is what the run did not report.** A tool that is silent when it is green leaves nobody able to check that the green is right.
`--explain` turns the count into the list, with the rule that kept each:

```console
$ ocomment check --explain
  ALLOWED 1 comment this run did not report
    src/budget.rs:13  /// A fresh budget.
      kept: policy conservative protects documentation comments, and this is a `doc-line`
```

`--explain` also puts the engine's own verdict under each finding.
The decision above it says *what to do*, read from where the comment sits; the line `--explain` adds says *why it is being asked*, which is the rule and the setting behind it.

### When there are thousands

Above twenty findings a group shows its shape instead of its contents: where its comments are, most first, and two of them as an example.

```console
  DECIDE  make it a documentation comment              4679 comments
    rust/ocomment-core/src/scanner.rs                       1973
    rust/ocomment/src/output.rs                              499
    … and 52 more files
    editors/vscode/esbuild.mjs:6
      - /** @type {import("esbuild").BuildOptions} */
        const options = {

   ocomment check rust/ocomment-core/src/scanner.rs   the 2259 in one file, in full
```

A count with no location cannot set an order.
"1,973 of these are in one file" is the difference between a project-wide problem and an afternoon, and the last line is the one thing a count never gives you: somewhere to start.
It is withheld when the busiest file holds under a twentieth of the total, because that is not a place to start — it is a place that happens to be first.

## `human`

One line per finding, in the `path:line:column:` stream a pipeline greps.

```console
$ ocomment check --format human
src/budget.rs:3:1: removable line comment: // The retry budget is per connection, not per
src/budget.rs:9:1: removable line comment: // TODO: make this configurable
```

Kept because a pipeline written against it should not have to be rewritten, and because one line per finding is the right shape for counting even when it is the wrong shape for deciding.
The end-of-run summary on standard error is the same whichever format wrote the report.

## `agent`

The same report for a reader that is going to act on it, carrying its own schema — a machine format whose reader has to go and learn it first spends a round trip doing that.

```console
$ ocomment check --format agent
# ocomment: 5 comments to answer for in 1 of 1 file scanned, policy conservative.
# Every line starts with a marker. DECIDE opens one question, asked of each
# FINDING under it. A FINDING names a path and the first and last line of one
# comment, which may span several, and the column when the comment does not
# open its line. `-` is what is there now, `+` what would replace it, `=` the
# code the comment is about. KEEP names a file and `|` the setting that would
# stop the question being asked. BROKEN is a file that did not parse. The
# argv lines are commands, ready to run.

DECIDE make it a documentation comment | 2 comments
FINDING src/budget.rs:3-4
- // The retry budget is per connection, not per request, because the
+ /// The retry budget is per connection, not per request, because the
= pub struct Budget {
KEEP .ocomment.toml
| [policy.allow]
| tags = ["NOTE"]

RECHECK ["ocomment","check"]
REMOVE-ALL ["ocomment","fix"] removes 5 comments, including any above that were worth keeping
```

Every payload line carries a marker, so text that happens to contain a colon or a keyword cannot be mistaken for structure.
Commands are argv arrays rather than prose, because a copied array cannot be mistyped.

A clean run writes nothing at all, which is what makes this usable as the body of a hook decision.
See [Agents](agents.md).

## `json`, `jsonl`, `sarif`, `github`

`--format json` carries the whole report — every comment, its span, its line and column, its text and its verdict — against [`spec/result.schema.json`](https://github.com/P4suta/OComment/blob/main/spec/result.schema.json).

Each file also says what read it.
`language` is which built-in grammar applied,
and it is `unknown` for a file no built-in language claims; `read_by` is the reader that answered, which for such a file is a declarative profile or a plugin that read it from end to end.
One field without the other said `unknown` about a file the run had just read in full:

```json
{ "path": ".gitignore", "language": "unknown",
  "read_by": { "kind": "profile", "name": "hash-line" } }
```

It also carries `decisions`: the same grouping the other two formats show, with the lines as they are, what would replace them, and the settings to add.

```json
{
  "decision": "explains-the-item-below",
  "instruction": "make it a documentation comment",
  "comments": 2,
  "findings": [
    {
      "path": "src/budget.rs",
      "span": { "start": 26, "end": 147 },
      "line": 3,
      "column": 1,
      "end_line": 4,
      "old": ["// The retry budget is per connection, not per request, because the"],
      "new": ["/// The retry budget is per connection, not per request, because the"],
      "subject": "pub struct Budget {"
    }
  ],
  "keep_instead": { "file": ".ocomment.toml", "add": "[policy.allow]\ntags = [\"NOTE\"]" }
}
```

`span` is what identifies a finding; the path and the line do not.
Two removable comments share a line whenever one of them sits beside code —
`let x = 1; /* directive */ /* prose */` is two findings, asked two different questions — and named by line alone they arrive identical.
The text formats put the column after the line in that case for the same reason, and leave it off for a comment that only happens to be indented, which is the only one on its line.

A paragraph a style rule would write differently is a decision like any other, and it is in the same list.
The one difference is that the answer is already computed: `new` carries the bytes, and `keep_instead` names the setting that would stop the rule asking.

```json
{
  "decision": "wrap",
  "instruction": "run `ocomment fix --tidy` and it is written for you",
  "comments": 1,
  "findings": [
    {
      "path": "src/budget.rs",
      "span": { "start": 26, "end": 92 },
      "line": 3,
      "column": 5,
      "end_line": 3,
      "old": ["    /// One sentence. Another one."],
      "new": ["    /// One sentence.", "    /// Another one."]
    }
  ],
  "keep_instead": { "file": ".ocomment.toml", "add": "[style]\nwrap = \"preserve\"" }
}
```

The report itself carries them too, beside the comments rather than among them, because the bytes a reflow moves belong to no single comment:

```json
{ "runs": [ { "span": { "start": 26, "end": 92 },
              "line": 3, "column": 5, "end_line": 3, "end_column": 36,
              "origin": "comments", "rule": "wrap",
              "replacement": "/// One sentence.\n    /// Another one." } ] }
```

`origin` says what the paragraph was: `comments` for a run of adjacent comments, `document` for the prose of a Markdown page.
`runs` is absent where the run asked for no rule about how a paragraph is broken, which is every run that set none.

`--format jsonl` is the same content one object per line.
`--format sarif` and `--format github` are for the tools that read them; see [CI and hooks](ci.md).
Both carry rewrites as well as removals — a SARIF result for a rewrite carries the replacement as its `fixes[]`, so an editor or a review bot can apply it — and both name a rewrite as a rewrite: a format that called it a removal would be telling a reader their documentation is about to be deleted.

## After a fix

`fix` reports what it removed and then what it left.

```console
$ ocomment fix
  OK  5 comments removed from 1 file · 1 file scanned

  KEPT    1 comment, still in the files
    src/budget.rs:13  /// A fresh budget.
```

A run that says only what it removed is a run whose judgement nobody can audit:
you are told five went and have no way to check that the sixth was right to stay.

`diff` writes a patch under either person-facing format, because a patch is a product rather than a report about one and there is no grouped view of one.
