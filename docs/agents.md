# Agents

OComment has three readers, and one output shape for each.
A person reads `--format human`: colour, hyperlinks, a summary at the end.
A program reads `--format json`, `--format jsonl`, `--format sarif` or `--format github`: a schema, a fixed shape, no prose.
An agent about to edit the file reads `--format agent`, which is neither — it is an instruction.

This page is about the third.
`docs/library.md` is the library, `docs/ci.md` is the pipeline, and `AGENTS.md` at the repository root is for an agent working on OComment itself rather than with it.

## `--format agent`

```console
$ ocomment check --format agent src
ocomment: 3 comments to go in 2 files.

src/session.rs:12:5 shorten to 1 line: // The retry budget is per connection
src/session.rs:13:5 shorten to 1 line: // rather than per request, because a
src/pool.rs:44:23 move above the code: // NOTE: closed by the caller

rule: policy `conservative` removes line and block comments. Allowed: a comment tagged NOTE, SAFETY or INVARIANT, at most 1 line of adjacent comments, never beside code.
next: edit them, or run `ocomment fix src/session.rs src/pool.rs`.
```

Three parts, in the order they are needed.

**What has to change**, one line per comment, `path:line:column` first and the verb second.
The verb is the rule that decided the comment, not a description of the verdict: a comment that is only too long says `shorten to 1 line`, and one that only sits in the wrong place says `move above the code`.
Telling a reader to delete a comment that had to move is wrong advice however correct the verdict was.

**The rule**, once.
A report that lists three comments and never says what would have been acceptable teaches nothing: the reader fixes those three and writes the fourth the same way.
This line is written only when every file in the report was judged by the same rules — a `[[overrides]]` table covering part of the tree means there is no single sentence to write, and none is written rather than one that is true of only some of the findings.

**The way through**, split by who has to answer.
`TIDY-ALL` runs `ocomment fix --tidy`, which writes every rewrite in the report and takes no comment away; it is safe to run without reading the findings first, because nothing it does is a judgement.
`REMOVE-ALL` runs `ocomment fix`, which also applies every removal above — including the ones that were worth keeping, which is why its line says how many.
Both are named only when the bytes are on the disk; a proposal a hook is judging gets a plain instruction instead.

One verb is deliberately not a single action:

```
src/pool.rs:44:1 do it or drop it (61d old, 30d allowed): // TODO: retry on timeout
```

That is a tag with a deadline — see [`[policy.allow.expiry]`](configuration.md#tags-that-are-promises).
Deleting the line satisfies the rule and loses the promise; doing the work satisfies both.
Only the reader knows which, so the report does not pick.

A clean run writes **nothing at all**, on either stream, and exits 0. Silence is the pass.
That is what makes this format usable as the body of a hook decision: there is no "nothing to do" line to parse before finding out whether there is anything to do.

## Editing hooks

`ocomment hook <SURFACE>` reads an agent host's hook payload on standard input and answers in that host's protocol.
It decides nothing of its own: it works out which bytes are about to become which file, hands that pair to the same machinery `ocomment check` runs, and writes the answer in the shape the host reads.
Your `.ocomment.toml` is the whole of the policy, exactly as it is for the command line and for CI.

### Claude Code

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [{ "type": "command", "command": "ocomment hook claude-code" }]
      }
    ]
  }
}
```

In `.claude/settings.json` for a project, or `~/.claude/settings.json` for every project.
`PreToolUse` is the one worth having: the hook is asked *before* the edit lands, so a comment that would not survive the project's policy never enters the file.

- `Write` carries the whole file, and is judged as written.
- `Edit` and `MultiEdit` carry replacements.
  The hook applies them to the file as it stands and judges the result, so the line numbers it reports are the ones the agent will find when it looks.
  Nothing is written: the file on disk is untouched either way.
- A replacement that does not match the file is an edit the host will refuse on its own, and the hook says nothing about it rather than judging bytes that will never exist.

A clean edit gets no answer at all, and the edit proceeds under whatever permission rules its user set.
The hook never answers `allow`: waving an edit past those rules is not what it was asked about.

An unclean edit is denied, with the `--format agent` report as the reason.
The agent sees it, rewrites, and tries again.

`PostToolUse` works too, and is the one to use for a file the agent did not write through `Write` or `Edit` — a generator, a formatter, a shell command:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [{ "type": "command", "command": "ocomment hook claude-code" }]
      }
    ]
  }
}
```

The edit has already happened by then, so there is nothing left to refuse and the report is a correction: the hook exits 2 with the report on standard error,
which is how that host puts text in front of the model.

Either hook is silent about every event that is not an edit.
Reading a file with comments in it is not writing one.

### Another host

One file, `rust/ocomment/src/hook.rs`, holds every line of this that is specific to a host — the same arrangement as `editors/` and `action.yml`, which speak an editor's and a CI system's protocols without either reaching into the scanner.
Supporting another agent host is one more `Surface` and one more arm.
Nothing in `ocomment-core` knows that any of them exist.

## Exit status

The same three everywhere, and the hook surfaces are the documented exception:

| Status | Meaning |
| --- | --- |
| 0 | Nothing removable, and every requested change applied. |
| 1 | Removable comments were reported, or a diff was printed. |
| 2 | Invalid source, configuration, plugin, or I/O failure. |

`ocomment hook claude-code` answers in its host's vocabulary instead: 0 when it has nothing to say *or* when it is carrying a decision in its JSON output, 2 when a `PostToolUse` correction has to reach the model.

## Reading a report as data

`--format agent` is prose with a fixed shape, meant to be read rather than parsed.
When you want fields, ask for fields:

```console
$ ocomment scan --format json src/session.rs
```

`spec/result.schema.json` is the schema, published in the repository and embedded in the binary — `ocomment config schema` writes the configuration one.
`--format jsonl` is the same content one object per line, for a stream you do not want to buffer.

### A report can tell you it was guessing

`valid` is false when the source failed to lex, and a report like that still carries a verdict for every comment it found — because those are what the scanner concluded, not because you should act on all of them.
The ones it could not establish are marked, and only those:

```python
for comment in report["comments"]:
    if comment.get("established", True):
        act_on(comment)
```

A comment without the field is one the scan established, and its verdict is worth what any verdict in a clean report is worth.

A scanner that cannot find the end of a token does not know where the next one starts, so an unterminated block opener is reported as a comment running to the end of the file — and the code under it is not a comment.
Acting on that verdict deletes code.
`fix --force-invalid` skips exactly these comments for the same reason, so if you are shelling out rather than reading the report you already have this for free.

The mark is narrow on purpose.
A C# string that never closes ends at the newline, and the comment on the line below it is delimited by a scanner that knows exactly where it is — that comment is not marked, and a forced run still removes it.

## Finding out why

`--explain` names, under each reported comment, the rule that decided it and the table that rule came from:

```console
$ ocomment check --explain src/session.rs
src/session.rs:12:5: removable line comment: // The retry budget is per connection
    removed: it belongs to a run of 2 adjacent comment lines, and at most 1 is allowed ([policy.allow] in .ocomment.toml); cut the run to 1 line
```

`--trace json` records the whole path to that verdict — the language detection,
the scanner entered, each comment classified, each edit planned — as JSONL on standard error, against `spec/trace.schema.json`.
Standard output is unchanged,
so a trace can be collected from a run whose output is being consumed.

`ocomment doctor` reports what the environment resolved to: which configuration files were found, which Git repository, which plugins.
`ocomment selftest` runs the corpus embedded in the binary against that binary,
which is how an agent can tell a broken install from a disagreement about policy.
