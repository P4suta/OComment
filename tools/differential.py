#!/usr/bin/env python3
"""Run normalized JSONL fixtures against Rust and OCaml implementations."""

import base64
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUST = ROOT / "rust/target/debug/examples/ref_driver"
OCAML = ROOT / "ocaml/_build/default/bin/main.exe"

CORPUS = json.loads((ROOT / "spec/fixtures/v1/builtins.json").read_text(encoding="utf-8"))
assert CORPUS["version"] == 1
# INVARIANT: `builtins.json` carries one case per language and is loaded into a
# INVARIANT: dict keyed by that language, so a second case for a language the
# INVARIANT: corpus already covers would silently replace the first rather than
# INVARIANT: be run. The assertion below is what says so out loud; anything past
# INVARIANT: the first case for a language belongs in SPECIAL_FIXTURES.
FIXTURES = {
    case["language"]: case["source_utf8"].encode("utf-8")
    for case in CORPUS["cases"]
}
assert len(FIXTURES) == len(CORPUS["cases"]), "builtins.json holds one case per language"

UNICODE_COLUMN_SAMPLE = "".join(
    chr(value)
    for value in range(0x80, 0x110000, 97)
    if not 0xD800 <= value <= 0xDFFF
).encode("utf-8")


SPECIAL_FIXTURES = [
    ("rust-nested-raw", "rust", br'r#"// opaque"# /* outer /* inner */ end */\n// rustfmt::skip\n', {}),
    ("rust-raw-c-string", "rust", b'cr#"inner " // opaque"#; // remove\n', {}),
    ("rust-multiline-string", "rust", b'const A: &str = "a\n// opaque\nb"; // remove\n', {}),
    ("ocaml-nested-quoted", "ocaml", br'{tag| (* opaque *) |tag} (* outer "*)" (* inner *) *)', {}),
    ("ocaml-comment-quoted", "ocaml", br'(* outer {tag| *) opaque |tag} end *)', {}),
    ("ocaml-long-quoted-id", "ocaml", b"{" + b"a" * 80 + b"|(* opaque *)|" + b"a" * 80 + b"} (* remove *)", {}),
    ("invalid-ocaml-quoted", "ocaml", br'{tag| unterminated (* opaque *)', {}),
    ("c-line-splice", "c", b"int x; /\\\n/ comment\\\ncontinued\nint y;", {}),
    ("cpp-raw", "cpp", br'R"tag(/* opaque */ // opaque)tag" // remove', {}),
    ("go-directives", "go", b"//go:build linux\n// +build linux\n//line generated.go:1\n// remove\n", {}),
    ("java-unicode", "java", br"int x; \u002f\u002f comment\u000aint y;", {}),
    ("java-unicode-surrogates", "java", br'String s = "\uD83D\uDE00 // opaque"; // remove', {}),
    ("invalid-java-unicode", "java", br"int x = 1; \u00G0 // known", {}),
    ("forced-invalid-java-unicode", "java", br"int x = 1; \u00G0 // known", {"force_invalid": True}),
    ("java-text-block-escape", "java", b'String s = """\n\\""" // opaque\nend\n"""; // remove\n', {}),
    ("javascript-goals", "javascript", br'''#!/usr/bin/env node
const r = /\/\/* opaque/;
const t = `literal // opaque ${1 /* remove */}`;
// remove
''', {}),
    ("javascript-control-regex", "javascript", br"if (ready) /https?:\/\/example\.test/.test(value); // remove", {}),
    ("javascript-brace-goals", "javascript", b"const ratio = {} / 2; // remove\nif (ready) {} /[/*]/.test(value); // remove\n", {}),
    ("javascript-html-like-comments", "javascript", b"const x = 1; <!-- remove\n  --> remove\nconst text = '<!-- opaque';\n", {}),
    ("javascript-unicode-line-terminators", "javascript", "// remove\u2028const value = 1; /* first\u2029second */\n".encode(), {}),
    ("javascript-unicode-line-in-string", "javascript", "const text = 'first\u2028second'; // known\n".encode(), {}),
    ("jsx-goal", "javascript", br'const x=<div url="http://x">// opaque {1 /* remove */}</div>; // remove', {"dialect": "jsx"}),
    ("html-raw-text-close-boundary", "html", b'<script>const s = "</scripture>"; // remove\n</script>', {}),
    ("invalid-html-embedded", "html", b"<script>const x = 1; // known\n", {}),
    ("forced-invalid-html-embedded", "html", b"<script>const x = 1; // known\n", {"force_invalid": True}),
    ("typescript-directive", "typescript", b'/// <reference path="types.d.ts" />\n// remove\n', {}),
    ("javascript-tree-shaking", "javascript", b"const x = /*#__PURE__*/ factory();\n// @ts-expect-error\ncall();\n// remove\n", {}),
    ("python-fstring", "python", br"""#!/usr/bin/env python3
# coding: utf-8
value = f'''literal # opaque {(
  1 # remove
)}'''
# remove
""", {}),
    ("python-inline-encoding-text", "python", b"value = 1  # coding: utf-8\n", {}),
    ("python-template-string", "python", b"value = t'''literal # opaque {(\n  1 # remove\n)}'''\n# remove\n", {}),
    ("shell-heredoc", "shell", b"cat <<'EOF'\n# opaque\nEOF\ncat <<< '# opaque'\n# remove\n", {"dialect": "bash53"}),
    ("shell-quoted-heredoc-and-ansi", "shell", b"cat <<E\"OF\"\n# opaque\nEOF\ncat <<\\DONE\n# opaque\nDONE\nvalue=$'it\\'s # opaque'\n# remove\n", {"dialect": "bash53"}),
    ("shell-command-substitutions", "shell", b'value="$(printf ok # nested\n)"\nold=`printf ok # legacy\n`\ntext="# opaque"\n# remove\n', {"dialect": "bash53"}),
    ("shell-logical-word-boundaries", "shell", b"value=word\\\n#suffix\njoined=$(printf x)#suffix\nprintf ok \\\n# remove\n$(printf x);# remove\n", {}),
    ("dockerfile-directives", "shell", b"# syntax=docker/dockerfile:1\n# remove\n# hadolint ignore=DL3018\nRUN apk add --no-cache musl-dev\n", {}),
    ("shell-case-command-substitution", "shell", b"value=$(case x in\n  a) # remove\n    printf '%s' '# opaque' ;;\n  *) printf ok ;;\nesac\n)#suffix\n# remove\n", {}),
    ("sql-postgres", "sql", br'select $tag$-- opaque /* opaque */$tag$; /* outer /* nested */ end */ -- remove', {"dialect": "postgresql"}),
    ("sql-standard-backslash", "sql", b"select '\\'; -- remove\n", {}),
    ("sql-postgres-escape", "sql", b"select E'it\\'s -- opaque'; -- remove\n", {"dialect": "postgresql"}),
    ("sql-postgres-invalid-dollar-tag", "sql", b"select $1$; -- remove\n", {"dialect": "postgresql"}),
    ("sql-postgres-long-dollar-tag", "sql", b"select $" + b"a" * 65 + b"$-- opaque$" + b"a" * 65 + b"$; -- remove\n", {"dialect": "postgresql"}),
    ("sql-oracle", "sql", br"select q'[-- opaque /* opaque */]' from dual; /*+ index(t) */ -- remove", {"dialect": "oracle"}),
    ("sql-mysql", "sql", b"/*!40101 SET NAMES utf8 */ # remove\n", {"dialect": "mysql"}),
    ("sql-mysql-double-escape", "sql", b'select "it\\\"s -- opaque"; -- remove\n', {"dialect": "mysql"}),
    ("sql-mysql-dash-boundary", "sql", b"select 1--2; -- remove\n", {"dialect": "mysql"}),
    ("sql-tsql-nested", "sql", b"/* outer /* inner */ end */ select 1;", {"dialect": "t-sql"}),
    ("c-plus-comment", "c", b"int x; /*+ ordinary C comment */", {}),
    ("kotlin-string-templates", "kotlin", b"val regular = \"opaque // ${1 /* remove */}\"\nval raw = \"\"\"opaque ${run { // remove\n1 }} /* opaque */\"\"\"\n// remove\n", {}),
    ("legal-policy", "javascript", b"// SPDX-License-Identifier: MIT\n// remove\n", {"policy": "legal"}),
    ("force-preamble", "javascript", b"#!/usr/bin/env node\n// remove\n", {"policy": "all", "force_protected": True}),
    ("kind-and-regex-overrides", "c", b"/* KEEP */ /* REMOVE */ // ordinary\n", {
        "keep_regex": ["(?i)keep"], "remove_regex": ["(?i)remove"], "keep_kinds": ["line"]
    }),
    ("column-layout", "c", "x\t/*中😀*/y\r\n".encode(), {"layout": "columns"}),
    ("column-unicode-width", "c", "x/*‍ֿ⌚️🇯🇵ᆍᇮힸ\U000e0093៘⵿؀؅*/y\n".encode(), {"layout": "columns"}),
    ("column-unicode-width-scalar-sample", "c", b"x/*" + UNICODE_COLUMN_SAMPLE + b"*/y\n", {"layout": "columns"}),
    ("column-valid-then-invalid-utf8", "c", b"x/*\xe4\xb8\xad\xff*/y", {"layout": "columns"}),
    ("column-after-empty-html-removal", "html", b"ab<!-- drop\nline --><script>let x=1;/*\t*/y</script>", {"policy": "all", "layout": "columns"}),
    ("non-utf8-bytes", "c", b"\xff/* remove */\x80\r\n", {}),
    ("compact-layout", "c", b"left/* remove */right\n", {"layout": "compact"}),
    ("invalid-cpp-raw", "cpp", br'R"tag(unterminated /* opaque */', {}),
    ("invalid-shell-quote", "shell", b"echo 'unterminated", {}),
    ("invalid-shell-heredoc", "shell", b"cat <<EOF\nbody\n", {}),
    ("invalid-shell-command-substitution", "shell", b"value=$(echo ok # comment\n", {}),
]


def request(index, language, source, options):
    return {
        "id": index,
        "operation": "transform",
        "language": language,
        "source_base64": base64.b64encode(source).decode(),
        "options": {"policy": "safe", "layout": "lines", **options},
    }


def span_request(index):
    source = b"a/*x*/b//y\n"
    value = request(index, "c", source, {"keep_kinds": ["line"]})
    value["operation"] = "transform-spans"
    value["spans"] = [
        {"start": 1, "end": 6, "kind": "block"},
        {"start": 7, "end": 10, "kind": "line"},
    ]
    return value


def edits_request(index):
    source = b"\xffabcdef\r\n"
    value = request(index, "c", source, {})
    value["operation"] = "apply_edits"
    value["edits"] = [
        {
            "span": {"start": 1, "end": 3},
            "replacement_base64": base64.b64encode(b"\x80X").decode(),
        },
        {
            "span": {"start": 6, "end": 7},
            "replacement_base64": base64.b64encode(b"").decode(),
        },
    ]
    return value


def profile_request(index, policy):
    source = b'";; opaque" ;; remove\n#| outer #| nested |# |# ;; KEEP\n'
    value = request(index, "c", source, {"policy": policy})
    value["operation"] = "transform-profile"
    value["profile"] = {
        "name": "demo",
        "extensions": ["demo"],
        "line_comments": [{"start": ";;", "kind": "line"}],
        "block_comments": [{"start": "#|", "end": "|#", "nested": True, "kind": "block"}],
        "strings": [{"start": '"', "end": '"', "escape": "\\"}],
        "protected_patterns": [{"contains": "KEEP", "reason": "fixture protection"}],
    }
    return value


def run(executable, requests):
    payload = "".join(json.dumps(item, separators=(",", ":")) + "\n" for item in requests)
    completed = subprocess.run([str(executable)], input=payload, text=True, capture_output=True, check=True)
    return [json.loads(line) for line in completed.stdout.splitlines()]


def main():
    requests = []
    labels = []
    index = 0
    for language, source in FIXTURES.items():
        for policy in ("safe", "all"):
            requests.append(request(index, language, source, {"policy": policy}))
            labels.append(f"{language}-{policy}")
            index += 1
    for label, language, source, options in SPECIAL_FIXTURES:
        requests.append(request(index, language, source, options))
        labels.append(label)
        index += 1
    requests.append(span_request(index))
    labels.append("external-plugin-spans")
    index += 1
    requests.append(edits_request(index))
    labels.append("apply-edits-binary")
    index += 1
    for policy in ("safe", "all"):
        requests.append(profile_request(index, policy))
        labels.append(f"declarative-profile-{policy}")
        index += 1
    rust = run(RUST, requests)
    ocaml = run(OCAML, requests)
    failures = 0
    for label, request_value, left, right in zip(labels, requests, rust, ocaml):
        if left != right:
            failures += 1
            print(f"mismatch: {label} ({request_value['language']})", file=sys.stderr)
            print(json.dumps({"rust": left, "ocaml": right}, indent=2), file=sys.stderr)
    if failures:
        print(f"{failures} differential fixture(s) failed", file=sys.stderr)
        return 1
    print(f"{len(requests)} Rust/OCaml differential fixtures passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
