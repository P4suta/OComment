use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
use tower_lsp::lsp_types::Url;

struct LspClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl LspClient {
    fn start(directory: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ocomment"))
            .arg("lsp")
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start OComment LSP");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin: Some(stdin),
            stdout,
        }
    }

    fn send(&mut self, message: Value) {
        let body = serde_json::to_vec(&message).unwrap();
        let stdin = self.stdin.as_mut().unwrap();
        write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        stdin.write_all(&body).unwrap();
        stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut length = None;
        loop {
            let mut header = String::new();
            assert_ne!(self.stdout.read_line(&mut header).unwrap(), 0, "LSP EOF");
            if header == "\r\n" || header == "\n" {
                break;
            }
            if let Some(value) = header
                .strip_prefix("Content-Length:")
                .or_else(|| header.strip_prefix("content-length:"))
            {
                length = Some(value.trim().parse::<usize>().unwrap());
            }
        }
        let mut body = vec![0; length.expect("Content-Length header")];
        self.stdout.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn response(&mut self, id: i64) -> Value {
        for _ in 0..64 {
            let message = self.read();
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return message;
            }
        }
        panic!("no response for request {id}");
    }

    fn response_with_notifications(&mut self, id: i64, method: &str) -> (Value, Vec<Value>) {
        let mut notifications = Vec::new();
        for _ in 0..128 {
            let message = self.read();
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return (message, notifications);
            }
            if message.get("method").and_then(Value::as_str) == Some(method) {
                notifications.push(message);
            }
        }
        panic!("no response for request {id}");
    }

    fn notification(&mut self, method: &str) -> Value {
        for _ in 0..64 {
            let message = self.read();
            if message.get("method").and_then(Value::as_str) == Some(method) {
                return message;
            }
        }
        panic!("no `{method}` notification");
    }

    fn initialize(&mut self, root: &Path, encodings: &[&str]) -> Value {
        self.initialize_with_folders(Some(root), encodings, Some(&[root]))
    }

    fn initialize_with_folders(
        &mut self,
        root: Option<&Path>,
        encodings: &[&str],
        folders: Option<&[&Path]>,
    ) -> Value {
        let root_uri = root.map(|root| Url::from_directory_path(root).unwrap());
        let workspace_folders = folders.map(|folders| {
            folders
                .iter()
                .map(|root| {
                    let uri = Url::from_directory_path(root).unwrap();
                    json!({
                        "uri": uri,
                        "name": root.file_name().unwrap_or_default().to_string_lossy()
                    })
                })
                .collect::<Vec<_>>()
        });
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {
                    "general": { "positionEncodings": encodings },
                    "workspace": { "diagnostics": { "refreshSupport": true } }
                },
                "workspaceFolders": workspace_folders
            }
        }));
        let response = self.response(1);
        self.send(json!({
            "jsonrpc": "2.0", "method": "initialized", "params": {}
        }));
        response
    }

    fn stop(mut self) {
        self.send(json!({
            "jsonrpc": "2.0", "id": 99, "method": "shutdown"
        }));
        let response = self.response(99);
        assert!(
            response.get("error").is_none_or(Value::is_null),
            "shutdown failed: {response}"
        );
        self.send(json!({
            "jsonrpc": "2.0", "method": "exit"
        }));
        drop(self.stdin.take());
        assert!(self.child.wait().unwrap().success());
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn protocol_supports_utf8_pull_workspace_actions_and_stale_versions() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("sample.rs");
    let uri = Url::from_file_path(&path).unwrap();
    let closed_path = workspace.path().join("closed.rs");
    std::fs::write(&closed_path, "let closed = true; // remove\n").unwrap();
    let closed_uri = Url::from_file_path(&closed_path).unwrap();
    let mut client = LspClient::start(workspace.path());
    let initialized = client.initialize(workspace.path(), &["utf-8", "utf-16"]);
    assert_eq!(
        initialized["result"]["capabilities"]["positionEncoding"],
        "utf-8"
    );
    assert_eq!(
        initialized["result"]["capabilities"]["diagnosticProvider"]["workspaceDiagnostics"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["diagnosticProvider"]["workDoneProgress"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["codeActionProvider"]["workDoneProgress"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["executeCommandProvider"]["workDoneProgress"],
        true
    );
    assert_eq!(
        initialized["result"]["capabilities"]["textDocumentSync"]["willSaveWaitUntil"], true,
        "on-save must remain callable after a live configuration reload"
    );

    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "rust",
                "version": 1,
                "text": "😀 // remove\n"
            }
        }
    }));
    let pushed = client.notification("textDocument/publishDiagnostics");
    assert_eq!(pushed["params"]["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(
        pushed["params"]["diagnostics"][0]["range"]["start"]["character"],
        5
    );

    client.send(json!({
        "jsonrpc": "2.0", "id": 2, "method": "textDocument/diagnostic",
        "params": { "textDocument": { "uri": uri } }
    }));
    let pulled = client.response(2);
    assert_eq!(pulled["result"]["kind"], "full");
    let result_id = pulled["result"]["resultId"].as_str().unwrap().to_owned();
    assert_eq!(pulled["result"]["items"].as_array().unwrap().len(), 1);

    client.send(json!({
        "jsonrpc": "2.0", "id": 3, "method": "textDocument/diagnostic",
        "params": {
            "textDocument": { "uri": uri },
            "previousResultId": result_id
        }
    }));
    assert_eq!(client.response(3)["result"]["kind"], "unchanged");

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{
                "range": {
                    "start": { "line": 99, "character": 0 },
                    "end": { "line": 99, "character": 0 }
                },
                "text": "invalid"
            }]
        }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "id": 30, "method": "textDocument/diagnostic",
        "params": {
            "textDocument": { "uri": uri },
            "previousResultId": result_id
        }
    }));
    assert_eq!(client.response(30)["result"]["kind"], "unchanged");

    client.send(json!({
        "jsonrpc": "2.0", "id": 4, "method": "workspace/diagnostic",
        "params": { "previousResultIds": [], "workDoneToken": "diagnostics-4" }
    }));
    let (workspace_report, progress) = client.response_with_notifications(4, "$/progress");
    assert_eq!(
        progress.first().unwrap()["params"]["value"]["kind"],
        "begin"
    );
    assert_eq!(
        progress.first().unwrap()["params"]["value"]["cancellable"],
        true
    );
    assert_eq!(progress.last().unwrap()["params"]["value"]["kind"], "end");
    assert!(
        progress
            .iter()
            .all(|notification| { notification["params"]["token"] == "diagnostics-4" })
    );
    assert_eq!(
        workspace_report["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let closed_report = workspace_report["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|report| report["uri"] == closed_uri.as_str())
        .unwrap();
    assert!(closed_report["version"].is_null());
    assert_eq!(closed_report["items"].as_array().unwrap().len(), 1);

    client.send(json!({
        "jsonrpc": "2.0", "id": 5, "method": "textDocument/codeAction",
        "params": {
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 6 },
                "end": { "line": 0, "character": 6 }
            },
            "context": { "diagnostics": [] }
        }
    }));
    let actions = client.response(5);
    assert!(actions["result"].as_array().unwrap().iter().any(|action| {
        action["title"] == "Remove this comment"
            && action["edit"]["documentChanges"][0]["textDocument"]["version"] == 1
    }));
    let workspace_action = actions["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["title"] == "Remove comments in workspace")
        .unwrap();
    let workspace_edits = workspace_action["edit"]["documentChanges"]
        .as_array()
        .unwrap();
    assert_eq!(workspace_edits.len(), 2);
    assert!(workspace_edits.iter().any(|edit| {
        edit["textDocument"]["uri"] == closed_uri.as_str()
            && edit["textDocument"]["version"].is_null()
    }));

    client.send(json!({
        "jsonrpc": "2.0", "id": 6, "method": "textDocument/willSaveWaitUntil",
        "params": { "textDocument": { "uri": uri }, "reason": 1 }
    }));
    assert!(client.response(6)["result"].is_null());

    // NOTE: A stale update must not replace the current document snapshot.
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": 1 },
            "contentChanges": [{ "text": "let clean = true;\n" }]
        }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "id": 7, "method": "textDocument/diagnostic",
        "params": { "textDocument": { "uri": uri } }
    }));
    assert_eq!(
        client.response(7)["result"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    client.stop();
}

#[test]
fn sequential_incremental_changes_keep_cached_scan_in_sync() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = Url::from_file_path(workspace.path().join("incremental.rs")).unwrap();
    let mut client = LspClient::start(workspace.path());
    let _ = client.initialize(workspace.path(), &["utf-8"]);
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri, "languageId": "rust", "version": 1,
            "text": "let a = 1; // one\nlet b = 2; // two\n"
        }}
    }));
    assert_eq!(
        client.notification("textDocument/publishDiagnostics")["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didChange",
        "params": {
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [
                {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 0 }
                    },
                    "text": "let prefix = 0;\n"
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 14 },
                        "end": { "line": 2, "character": 17 }
                    },
                    "text": "changed"
                }
            ]
        }
    }));
    let pushed = client.notification("textDocument/publishDiagnostics");
    let diagnostics = pushed["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 1);
    assert_eq!(diagnostics[1]["range"]["start"]["line"], 2);

    client.send(json!({
        "jsonrpc": "2.0", "id": 40, "method": "textDocument/codeAction",
        "params": {
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 2, "character": 14 },
                "end": { "line": 2, "character": 14 }
            },
            "context": { "diagnostics": [] }
        }
    }));
    let actions = client.response(40);
    assert!(actions["result"].as_array().unwrap().iter().any(|action| {
        action["title"] == "Remove this comment"
            && action["edit"]["documentChanges"][0]["textDocument"]["version"] == 2
    }));
    client.stop();
}

#[test]
fn pending_workspace_request_can_be_cancelled() {
    let workspace = tempfile::tempdir().unwrap();
    let line = "let value = 1; // removable\n".repeat(1024);
    for index in 0..256 {
        std::fs::write(
            workspace.path().join(format!("source-{index:03}.rs")),
            &line,
        )
        .unwrap();
    }
    let mut client = LspClient::start(workspace.path());
    let _ = client.initialize(workspace.path(), &["utf-16"]);
    client.send(json!({
        "jsonrpc": "2.0", "id": 50, "method": "workspace/diagnostic",
        "params": { "previousResultIds": [], "workDoneToken": "cancel-50" }
    }));
    client.send(json!({
        "jsonrpc": "2.0", "method": "$/cancelRequest", "params": { "id": 50 }
    }));
    let response = client.response(50);
    assert_eq!(response["error"]["code"], -32800);
    client.stop();
}

#[test]
fn on_save_is_opt_in_and_returns_annotated_safe_edits() {
    let workspace = tempfile::tempdir().unwrap();
    std::fs::write(
        workspace.path().join(".ocomment.toml"),
        "version = 1\n[lsp]\non_save = true\n",
    )
    .unwrap();
    let uri = Url::from_file_path(workspace.path().join("save.rs")).unwrap();
    let mut client = LspClient::start(workspace.path());
    let initialized = client.initialize(workspace.path(), &["utf-16"]);
    assert_eq!(
        initialized["result"]["capabilities"]["textDocumentSync"]["willSaveWaitUntil"],
        true
    );
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri, "languageId": "rust", "version": 4,
            "text": "let x = 1; // remove\n"
        }}
    }));
    let _ = client.notification("textDocument/publishDiagnostics");
    client.send(json!({
        "jsonrpc": "2.0", "id": 8, "method": "textDocument/willSaveWaitUntil",
        "params": { "textDocument": { "uri": uri }, "reason": 1 }
    }));
    let response = client.response(8);
    assert_eq!(response["result"].as_array().unwrap().len(), 1);
    assert_eq!(response["result"][0]["range"]["start"]["character"], 11);
    client.stop();
}

#[test]
fn advertised_on_save_is_a_noop_while_live_configuration_disables_it() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = Url::from_file_path(workspace.path().join("save.rs")).unwrap();
    let mut client = LspClient::start(workspace.path());
    let initialized = client.initialize(workspace.path(), &["utf-16"]);
    assert_eq!(
        initialized["result"]["capabilities"]["textDocumentSync"]["willSaveWaitUntil"],
        true
    );
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri, "languageId": "rust", "version": 1,
            "text": "let x = 1; // remove\n"
        }}
    }));
    let _ = client.notification("textDocument/publishDiagnostics");
    client.send(json!({
        "jsonrpc": "2.0", "id": 9, "method": "textDocument/willSaveWaitUntil",
        "params": { "textDocument": { "uri": uri }, "reason": 1 }
    }));
    assert_eq!(client.response(9)["result"], Value::Null);
    client.stop();
}

#[test]
fn multi_root_routes_longest_ancestor_and_excludes_standalone_documents_from_workspace_fixes() {
    let server = tempfile::tempdir().unwrap();
    let outer = server.path().join("outer");
    let nested = outer.join("nested");
    let standalone = server.path().join("standalone");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir(&standalone).unwrap();
    std::fs::write(
        outer.join(".ocomment.toml"),
        "version = 1\n[policy]\nmode = \"safe\"\n",
    )
    .unwrap();
    std::fs::write(
        nested.join(".ocomment.toml"),
        "version = 1\n[policy]\nmode = \"all\"\n[files]\nexclude = [\"ignored.rs\"]\n",
    )
    .unwrap();
    let ignored_path = nested.join("ignored.rs");
    std::fs::write(&ignored_path, "let ignored = 1; // remove\n").unwrap();
    std::fs::write(
        standalone.join(".ocomment.toml"),
        "version = 1\n[policy]\nmode = \"all\"\n",
    )
    .unwrap();
    let outer_uri = Url::from_file_path(outer.join("outer.rs")).unwrap();
    let nested_uri = Url::from_file_path(nested.join("nested.rs")).unwrap();
    let standalone_uri = Url::from_file_path(standalone.join("outside.rs")).unwrap();
    let ignored_uri = Url::from_file_path(ignored_path).unwrap();

    let mut client = LspClient::start(server.path());
    let _ = client.initialize_with_folders(
        Some(&outer),
        &["utf-8"],
        Some(&[outer.as_path(), nested.as_path()]),
    );
    for (uri, version) in [(&outer_uri, 1), (&nested_uri, 2), (&standalone_uri, 3)] {
        client.send(json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "rust", "version": version,
                "text": "let value = 1; // rustfmt::skip\n"
            }}
        }));
        let published = client.notification("textDocument/publishDiagnostics");
        let count = published["params"]["diagnostics"].as_array().unwrap().len();
        let expected = usize::from(uri != &outer_uri);
        assert_eq!(
            count, expected,
            "wrong context routed for {uri}: {published}"
        );
    }

    client.send(json!({
        "jsonrpc": "2.0", "id": 70, "method": "workspace/diagnostic",
        "params": { "previousResultIds": [] }
    }));
    let report = client.response(70);
    let items = report["result"]["items"].as_array().unwrap();
    assert!(items.iter().any(|item| item["uri"] == outer_uri.as_str()));
    assert!(items.iter().any(|item| item["uri"] == nested_uri.as_str()));
    assert!(
        !items.iter().any(|item| item["uri"] == ignored_uri.as_str()),
        "an outer root bypassed the nested root's file exclusions: {report}"
    );
    assert!(
        !items
            .iter()
            .any(|item| item["uri"] == standalone_uri.as_str()),
        "a standalone document entered workspace diagnostics: {report}"
    );

    client.send(json!({
        "jsonrpc": "2.0", "id": 71, "method": "textDocument/codeAction",
        "params": {
            "textDocument": { "uri": nested_uri },
            "range": {
                "start": { "line": 0, "character": 18 },
                "end": { "line": 0, "character": 18 }
            },
            "context": { "diagnostics": [] }
        }
    }));
    let actions = client.response(71);
    let workspace = actions["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["title"] == "Remove comments in workspace")
        .unwrap();
    let edits = workspace["edit"]["documentChanges"].as_array().unwrap();
    assert!(
        edits
            .iter()
            .any(|edit| edit["textDocument"]["uri"] == nested_uri.as_str())
    );
    assert!(
        !edits
            .iter()
            .any(|edit| edit["textDocument"]["uri"] == standalone_uri.as_str()),
        "a standalone document entered the workspace fix: {workspace}"
    );
    client.stop();
}

#[test]
fn folderless_workspace_operations_cover_all_open_documents_only() {
    let server = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let first_uri = Url::from_file_path(first.path().join("one.rs")).unwrap();
    let second_uri = Url::from_file_path(second.path().join("two.rs")).unwrap();
    let mut client = LspClient::start(server.path());
    let _ = client.initialize_with_folders(None, &["utf-8"], None);
    for (uri, version) in [(&first_uri, 1), (&second_uri, 2)] {
        client.send(json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": {
                "uri": uri, "languageId": "rust", "version": version,
                "text": "let value = 1; // remove\n"
            }}
        }));
        let published = client.notification("textDocument/publishDiagnostics");
        assert_eq!(
            published["params"]["diagnostics"].as_array().unwrap().len(),
            1
        );
    }
    client.send(json!({
        "jsonrpc": "2.0", "id": 72, "method": "workspace/diagnostic",
        "params": { "previousResultIds": [] }
    }));
    let report = client.response(72);
    let items = report["result"]["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        2,
        "folder-less discovery read from disk: {report}"
    );
    assert!(items.iter().any(|item| item["uri"] == first_uri.as_str()));
    assert!(items.iter().any(|item| item["uri"] == second_uri.as_str()));
    client.stop();
}

#[test]
fn diagnostics_and_hover_name_comment_kinds_in_canonical_spelling() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = Url::from_file_path(workspace.path().join("doc.rs")).unwrap();
    let mut client = LspClient::start(workspace.path());
    let _ = client.initialize(workspace.path(), &["utf-8"]);
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri, "languageId": "rust", "version": 1,
            "text": "/** doc */\n// SPDX-License-Identifier: MIT\n"
        }}
    }));
    let pushed = client.notification("textDocument/publishDiagnostics");
    let diagnostics = pushed["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics[0]["message"], "removable doc-block comment");
    assert_eq!(diagnostics[0]["code"], "removable-comment");

    client.send(json!({
        "jsonrpc": "2.0", "id": 60, "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 3 }
        }
    }));
    assert_eq!(
        client.response(60)["result"]["contents"],
        "OComment: removable doc-block comment"
    );

    client.stop();
}

#[test]
fn hover_over_a_protected_comment_names_the_kind_and_reason() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = Url::from_file_path(workspace.path().join("preamble.py")).unwrap();
    let mut client = LspClient::start(workspace.path());
    let _ = client.initialize(workspace.path(), &["utf-8"]);
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri, "languageId": "python", "version": 1,
            "text": "#!/usr/bin/env python3\nx = 1\n"
        }}
    }));
    let _ = client.notification("textDocument/publishDiagnostics");
    client.send(json!({
        "jsonrpc": "2.0", "id": 61, "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 3 }
        }
    }));
    assert_eq!(
        client.response(61)["result"]["contents"],
        "OComment: kept shebang comment: required source preamble"
    );
    client.stop();
}

#[test]
fn editor_language_ids_name_languages_the_path_alone_would_not() {
    let workspace = tempfile::tempdir().unwrap();
    let mut client = LspClient::start(workspace.path());
    let _ = client.initialize(workspace.path(), &["utf-8"]);

    /* NOTE: Neither name carries an extension and neither buffer opens with a
     * shebang, so the client's `languageId` is the only thing that can say
     * what the bytes are. An id the server cannot place leaves the document
     * `unknown`, which is an error diagnostic rather than a comment, so both
     * assertions below check the comment and not merely the count. */
    let shell = Url::from_file_path(workspace.path().join("hook")).unwrap();
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": shell, "languageId": "shellscript", "version": 1,
            "text": "echo hi # remove\n"
        }}
    }));
    let pushed = client.notification("textDocument/publishDiagnostics");
    assert_eq!(pushed["params"]["uri"], shell.as_str());
    let diagnostics = pushed["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "unexpected diagnostics: {pushed}");
    assert_eq!(diagnostics[0]["code"], "removable-comment");
    assert_eq!(diagnostics[0]["range"]["start"]["character"], 8);

    let cuda = Url::from_file_path(workspace.path().join("kernel")).unwrap();
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": cuda, "languageId": "cuda-cpp", "version": 1,
            "text": "int x = 1; // remove\n"
        }}
    }));
    let pushed = client.notification("textDocument/publishDiagnostics");
    assert_eq!(pushed["params"]["uri"], cuda.as_str());
    let diagnostics = pushed["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "unexpected diagnostics: {pushed}");
    assert_eq!(diagnostics[0]["code"], "removable-comment");
    assert_eq!(diagnostics[0]["range"]["start"]["character"], 11);

    client.stop();
}

#[test]
fn shellscript_keeps_the_dialect_the_path_implies() {
    let workspace = tempfile::tempdir().unwrap();
    let uri = Url::from_file_path(workspace.path().join("script.bash")).unwrap();
    let mut client = LspClient::start(workspace.path());
    let _ = client.initialize(workspace.path(), &["utf-8"]);
    /* NOTE: `$'...'` is ANSI-C quoting in Bash and zsh only. Read as POSIX sh
     * the string ends at the escaped quote and the comment starts eleven
     * columns earlier, on `#1'`. One editor id, `shellscript`, covers all
     * three shells, so the dialect still has to come from the path. */
    client.send(json!({
        "jsonrpc": "2.0", "method": "textDocument/didOpen",
        "params": { "textDocument": {
            "uri": uri, "languageId": "shellscript", "version": 1,
            "text": "printf $'it\\'s #1' # remove\n"
        }}
    }));
    let pushed = client.notification("textDocument/publishDiagnostics");
    let diagnostics = pushed["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "unexpected diagnostics: {pushed}");
    assert_eq!(diagnostics[0]["code"], "removable-comment");
    assert_eq!(diagnostics[0]["range"]["start"]["character"], 19);
    client.stop();
}
