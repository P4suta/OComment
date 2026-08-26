use crate::{
    config::{self, ResolvedConfig},
    files,
    output::{kept_label, removable_label},
    plugin::PluginHost,
};
use anyhow::Result as AnyResult;
use ocomment_core::{
    ByteSpan, Diagnostic as CoreDiagnostic, Dialect, Disposition, DocumentChange,
    IncrementalDocument, Language, ScanReport, Severity, SourceMap, TransformResult,
    detect_language, transform,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::RwLock;
use tower_lsp::{Client, LanguageServer, LspService, Server, jsonrpc, lsp_types::*};

#[derive(Clone)]
struct Document {
    text: String,
    language_name: String,
    language: Language,
    dialect: Dialect,
    version: i32,
    incremental: Option<IncrementalDocument>,
}

#[derive(Clone)]
struct WorkspaceSnapshot {
    uri: Url,
    document: Document,
    version: Option<i32>,
}

struct WorkspaceEditEntry {
    uri: Url,
    text: String,
    version: Option<i32>,
    edits: Vec<ocomment_core::Edit>,
}

struct Backend {
    client: Client,
    documents: RwLock<HashMap<Url, Document>>,
    configuration: RwLock<ResolvedConfig>,
    explicit_config: Option<PathBuf>,
    encoding: RwLock<PositionEncodingKind>,
    workspace_folders: RwLock<Vec<WorkspaceFolder>>,
    plugins: RwLock<PluginHost>,
    configuration_generation: AtomicU64,
    pull_diagnostics: AtomicBool,
    dynamic_file_watching: AtomicBool,
}

pub fn run(explicit_config: Option<&Path>) -> AnyResult<u8> {
    let configuration = config::load(explicit_config)?;
    let plugins = PluginHost::load(&configuration.root, &configuration.config.plugins)?;
    let explicit_config = explicit_config.map(Path::to_path_buf);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(|client| Backend {
            client,
            documents: RwLock::new(HashMap::new()),
            configuration: RwLock::new(configuration),
            explicit_config,
            encoding: RwLock::new(PositionEncodingKind::UTF16),
            workspace_folders: RwLock::new(Vec::new()),
            plugins: RwLock::new(plugins),
            configuration_generation: AtomicU64::new(0),
            pull_diagnostics: AtomicBool::new(false),
            dynamic_file_watching: AtomicBool::new(false),
        });
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    Ok(0)
}

impl Backend {
    async fn result(&self, uri: &Url, document: &Document) -> TransformResult {
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.path()));
        let configuration = self.configuration.read().await;
        if document.language != Language::Unknown
            && configuration
                .config
                .languages
                .get(document.language.as_str())
                .and_then(|language| language.enabled)
                == Some(false)
        {
            return unchanged_result(document.text.as_bytes(), document.language);
        }
        let (language, options) =
            configuration.for_path(&path, document.language, document.dialect);
        let profile = (document.language == Language::Unknown)
            .then(|| files::profile_for_path(&path, &configuration))
            .flatten();
        let plugin = (document.language == Language::Unknown && profile.is_none())
            .then(|| files::plugin_for_path(&path, &configuration))
            .flatten();
        drop(configuration);
        if let Some(profile) = profile {
            return ocomment_core::transform_profile(document.text.as_bytes(), &profile, options)
                .expect("profiles were validated while loading configuration");
        }
        if let Some(plugin) = plugin {
            return self
                .plugins
                .read()
                .await
                .transform(
                    &plugin,
                    document.text.as_bytes(),
                    &document.language_name,
                    &path,
                    options,
                )
                .unwrap_or_else(|error| plugin_failure(document.text.as_bytes(), error));
        }
        if let Some(incremental) = &document.incremental
            && incremental.language() == language
            && incremental.scan_options() == &options.scan
            && incremental.source() == document.text.as_bytes()
        {
            return incremental.transform(options.layout);
        }
        transform(document.text.as_bytes(), language, options)
    }

    async fn lsp_diagnostics(
        &self,
        uri: &Url,
        document: &Document,
    ) -> Vec<tower_lsp::lsp_types::Diagnostic> {
        let result = self.result(uri, document).await;
        let encoding = self.encoding.read().await.clone();
        let mut diagnostics = Vec::new();
        for comment in result
            .report
            .comments
            .iter()
            .filter(|comment| comment.disposition.is_remove())
        {
            diagnostics.push(tower_lsp::lsp_types::Diagnostic {
                range: span_to_range(document.text.as_bytes(), comment.span, &encoding),
                severity: Some(DiagnosticSeverity::HINT),
                code: Some(NumberOrString::String("removable-comment".into())),
                code_description: None,
                source: Some("ocomment".into()),
                message: removable_label(comment.kind),
                related_information: None,
                tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                data: None,
            });
        }
        for diagnostic in &result.report.diagnostics {
            diagnostics.push(tower_lsp::lsp_types::Diagnostic {
                range: span_to_range(document.text.as_bytes(), diagnostic.span, &encoding),
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String(diagnostic.code.clone())),
                code_description: None,
                source: Some("ocomment".into()),
                message: diagnostic.message.clone(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
        diagnostics
    }

    async fn publish(&self, uri: Url) {
        if self.pull_diagnostics.load(Ordering::Relaxed) {
            return;
        }
        let document = self.documents.read().await.get(&uri).cloned();
        if let Some(document) = document {
            let enabled = self.configuration.read().await.config.lsp.diagnostics;
            let diagnostics = if enabled {
                self.lsp_diagnostics(&uri, &document).await
            } else {
                Vec::new()
            };
            self.client
                .publish_diagnostics(uri, diagnostics, Some(document.version))
                .await;
        }
    }

    async fn reload_configuration(&self) {
        match config::load(self.explicit_config.as_deref()) {
            Ok(configuration) => {
                let plugins =
                    match PluginHost::load(&configuration.root, &configuration.config.plugins) {
                        Ok(plugins) => plugins,
                        Err(error) => {
                            self.client
                                .show_message(
                                    MessageType::ERROR,
                                    format!("OComment plugins: {error:#}"),
                                )
                                .await;
                            return;
                        }
                    };
                *self.configuration.write().await = configuration;
                *self.plugins.write().await = plugins;
                {
                    let configuration = self.configuration.read().await;
                    let mut documents = self.documents.write().await;
                    for (uri, document) in documents.iter_mut() {
                        document.incremental =
                            incremental_for_document(uri, document, &configuration);
                    }
                }
                self.configuration_generation
                    .fetch_add(1, Ordering::Relaxed);
                let uris: Vec<_> = self.documents.read().await.keys().cloned().collect();
                for uri in uris {
                    self.publish(uri).await;
                }
                let _ = self.client.code_lens_refresh().await;
                let _ = self.client.workspace_diagnostic_refresh().await;
            }
            Err(error) => {
                self.client
                    .show_message(
                        MessageType::ERROR,
                        format!("OComment configuration: {error:#}"),
                    )
                    .await
            }
        }
    }

    async fn document_workspace_edit(
        &self,
        uri: &Url,
        document: &Document,
        spans: Option<&[ByteSpan]>,
    ) -> WorkspaceEdit {
        let result = self.result(uri, document).await;
        let edits: Vec<_> = result
            .edits
            .iter()
            .filter(|edit| spans.is_none_or(|spans| spans.contains(&edit.span)))
            .cloned()
            .collect();
        let encoding = self.encoding.read().await.clone();
        annotated_workspace_edit(
            vec![WorkspaceEditEntry {
                uri: uri.clone(),
                text: document.text.clone(),
                version: Some(document.version),
                edits,
            }],
            &encoding,
        )
    }

    async fn send_progress(&self, token: Option<&ProgressToken>, value: WorkDoneProgress) {
        if let Some(token) = token {
            self.client
                .send_notification::<tower_lsp::lsp_types::notification::Progress>(ProgressParams {
                    token: token.clone(),
                    value: ProgressParamsValue::WorkDone(value),
                })
                .await;
        }
    }

    async fn all_documents_workspace_edit(
        &self,
        progress_token: Option<ProgressToken>,
    ) -> WorkspaceEdit {
        let documents = self.workspace_documents().await;
        self.send_progress(
            progress_token.as_ref(),
            WorkDoneProgress::Begin(WorkDoneProgressBegin {
                title: "Scanning OComment workspace".into(),
                cancellable: Some(true),
                message: Some(format!("0/{} files", documents.len())),
                percentage: Some(0),
            }),
        )
        .await;
        let mut transformed = Vec::new();
        let total = documents.len();
        for (index, snapshot) in documents.into_iter().enumerate() {
            let result = self.result(&snapshot.uri, &snapshot.document).await;
            transformed.push(WorkspaceEditEntry {
                uri: snapshot.uri,
                text: snapshot.document.text,
                version: snapshot.version,
                edits: result.edits,
            });
            self.send_progress(
                progress_token.as_ref(),
                WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(true),
                    message: Some(format!("{}/{} files", index + 1, total)),
                    percentage: Some(progress_percentage(index + 1, total)),
                }),
            )
            .await;
        }
        self.send_progress(
            progress_token.as_ref(),
            WorkDoneProgress::End(WorkDoneProgressEnd {
                message: Some(format!("{total} files scanned")),
            }),
        )
        .await;
        let encoding = self.encoding.read().await.clone();
        annotated_workspace_edit(transformed, &encoding)
    }

    async fn workspace_documents(&self) -> Vec<WorkspaceSnapshot> {
        let open: HashMap<_, _> = self
            .documents
            .read()
            .await
            .iter()
            .map(|(uri, document)| (uri.clone(), document.clone()))
            .collect();
        let mut snapshots: Vec<_> = open
            .iter()
            .map(|(uri, document)| WorkspaceSnapshot {
                uri: uri.clone(),
                document: document.clone(),
                version: Some(document.version),
            })
            .collect();
        let roots: Vec<_> = self
            .workspace_folders
            .read()
            .await
            .iter()
            .filter_map(|folder| folder.uri.to_file_path().ok())
            .collect();
        let discovery = {
            let configuration = self.configuration.read().await;
            files::discover_workspace(&roots, &configuration)
        };
        let discovery = match discovery {
            Ok(discovery) => discovery,
            Err(error) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("OComment workspace discovery failed: {error:#}"),
                    )
                    .await;
                snapshots.sort_by(|left, right| left.uri.as_str().cmp(right.uri.as_str()));
                return snapshots;
            }
        };
        for skipped in discovery.skipped.iter().filter(|item| item.error) {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!(
                        "OComment could not inspect {}: {}",
                        skipped.path.display(),
                        skipped.reason
                    ),
                )
                .await;
        }
        for file in discovery.files {
            let Ok(uri) = Url::from_file_path(&file.path) else {
                continue;
            };
            if open.contains_key(&uri) {
                continue;
            }
            let Ok(text) = String::from_utf8(file.source) else {
                continue;
            };
            let language_name = if file.language == Language::Unknown {
                file.path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unknown")
                    .to_ascii_lowercase()
            } else {
                file.language.as_str().to_owned()
            };
            snapshots.push(WorkspaceSnapshot {
                uri,
                document: Document {
                    text,
                    language_name,
                    language: file.language,
                    dialect: file.dialect,
                    version: 0,
                    incremental: None,
                },
                version: None,
            });
        }
        snapshots.sort_by(|left, right| left.uri.as_str().cmp(right.uri.as_str()));
        snapshots
    }

    fn diagnostic_result_id(&self, document: &Document, version: Option<i32>) -> String {
        let identity = version.map_or_else(
            || format!("disk:{:016x}", stable_text_hash(document.text.as_bytes())),
            |version| format!("open:{version}"),
        );
        format!(
            "{}:{}",
            identity,
            self.configuration_generation.load(Ordering::Relaxed)
        )
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        let offered = params
            .capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.clone())
            .unwrap_or_default();
        self.pull_diagnostics.store(
            params
                .capabilities
                .text_document
                .as_ref()
                .and_then(|capabilities| capabilities.diagnostic.as_ref())
                .is_some(),
            Ordering::Relaxed,
        );
        self.dynamic_file_watching.store(
            params
                .capabilities
                .workspace
                .as_ref()
                .and_then(|capabilities| capabilities.did_change_watched_files.as_ref())
                .and_then(|capabilities| capabilities.dynamic_registration)
                .unwrap_or(false),
            Ordering::Relaxed,
        );
        let encoding = offered
            .into_iter()
            .find(|encoding| {
                *encoding == PositionEncodingKind::UTF8
                    || *encoding == PositionEncodingKind::UTF16
                    || *encoding == PositionEncodingKind::UTF32
            })
            .unwrap_or(PositionEncodingKind::UTF16);
        *self.encoding.write().await = encoding.clone();
        *self.workspace_folders.write().await = params.workspace_folders.unwrap_or_default();
        let on_save = self.configuration.read().await.config.lsp.on_save;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(encoding),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: Some(on_save),
                        will_save_wait_until: Some(on_save),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::SOURCE_FIX_ALL,
                            CodeActionKind::new("source.fixAll.ocomment"),
                        ]),
                        resolve_provider: Some(false),
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: Some(true),
                        },
                    },
                )),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "ocomment.fixDocument".into(),
                        "ocomment.fixWorkspace".into(),
                    ],
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: Some(true),
                    },
                }),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("ocomment".into()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: true,
                        work_done_progress_options: WorkDoneProgressOptions {
                            work_done_progress: Some(true),
                        },
                    },
                )),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "ocomment".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "OComment LSP initialized")
            .await;
        if self.dynamic_file_watching.load(Ordering::Relaxed) {
            let options = DidChangeWatchedFilesRegistrationOptions {
                watchers: ["**/.ocomment.toml", "**/.ocomment.lock"]
                    .into_iter()
                    .map(|pattern| FileSystemWatcher {
                        glob_pattern: GlobPattern::String(pattern.into()),
                        kind: None,
                    })
                    .collect(),
            };
            let registration = Registration {
                id: "ocomment-configuration-watch".into(),
                method: "workspace/didChangeWatchedFiles".into(),
                register_options: serde_json::to_value(options).ok(),
            };
            if let Err(error) = self.client.register_capability(vec![registration]).await {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("OComment could not register configuration watchers: {error}"),
                    )
                    .await;
            }
        }
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let item = params.text_document;
        let (language, dialect) =
            language_from_lsp(&item.language_id, &item.uri, item.text.as_bytes());
        let mut document = Document {
            text: item.text,
            language_name: item.language_id.to_ascii_lowercase(),
            language,
            dialect,
            version: item.version,
            incremental: None,
        };
        document.incremental = {
            let configuration = self.configuration.read().await;
            incremental_for_document(&item.uri, &document, &configuration)
        };
        self.documents
            .write()
            .await
            .insert(item.uri.clone(), document);
        self.publish(item.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let encoding = self.encoding.read().await.clone();
        let mut documents = self.documents.write().await;
        let Some(document) = documents.get_mut(&uri) else {
            return;
        };
        if params.text_document.version <= document.version {
            drop(documents);
            self.client
                .log_message(
                    MessageType::WARNING,
                    "ignored stale OComment document version",
                )
                .await;
            return;
        }
        let mut next_text = document.text.clone();
        let mut next_incremental = document.incremental.clone();
        let mut valid = true;
        for change in params.content_changes {
            let span = if let Some(range) = change.range {
                let bytes = next_text.as_bytes();
                let Some(start) = position_to_byte(bytes, range.start, &encoding) else {
                    valid = false;
                    break;
                };
                let Some(end) = position_to_byte(bytes, range.end, &encoding) else {
                    valid = false;
                    break;
                };
                ByteSpan::new(start, end)
            } else {
                ByteSpan::new(0, next_text.len())
            };
            if span.start > span.end
                || !next_text.is_char_boundary(span.start)
                || !next_text.is_char_boundary(span.end)
            {
                valid = false;
                break;
            }
            if let Some(incremental) = &mut next_incremental {
                let Some(version) = incremental.version().checked_add(1) else {
                    valid = false;
                    break;
                };
                if incremental
                    .apply_changes(
                        &[DocumentChange {
                            span,
                            replacement: change.text.as_bytes().to_vec(),
                        }],
                        version,
                    )
                    .is_err()
                {
                    valid = false;
                    break;
                }
            }
            next_text.replace_range(span.start..span.end, &change.text);
        }
        valid &= next_incremental
            .as_ref()
            .is_none_or(|incremental| incremental.source() == next_text.as_bytes());
        if !valid {
            drop(documents);
            self.client
                .log_message(
                    MessageType::ERROR,
                    "ignored invalid OComment incremental change batch; send a full document update",
                )
                .await;
            return;
        }
        document.text = next_text;
        document.version = params.text_document.version;
        document.incremental = next_incremental;
        drop(documents);
        self.publish(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .write()
            .await
            .remove(&params.text_document.uri);
        if !self.pull_diagnostics.load(Ordering::Relaxed) {
            self.client
                .publish_diagnostics(params.text_document.uri, Vec::new(), None)
                .await;
        }
    }

    async fn will_save_wait_until(
        &self,
        params: WillSaveTextDocumentParams,
    ) -> jsonrpc::Result<Option<Vec<TextEdit>>> {
        if !self.configuration.read().await.config.lsp.on_save {
            return Ok(None);
        }
        let Some(document) = self
            .documents
            .read()
            .await
            .get(&params.text_document.uri)
            .cloned()
        else {
            return Ok(None);
        };
        let result = self.result(&params.text_document.uri, &document).await;
        if !result.report.valid {
            return Ok(None);
        }
        let encoding = self.encoding.read().await.clone();
        Ok(Some(
            result
                .edits
                .into_iter()
                .map(|edit| TextEdit {
                    range: span_to_range(document.text.as_bytes(), edit.span, &encoding),
                    new_text: String::from_utf8_lossy(&edit.replacement).into_owned(),
                })
                .collect(),
        ))
    }

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> jsonrpc::Result<DocumentDiagnosticReportResult> {
        let document = self
            .documents
            .read()
            .await
            .get(&params.text_document.uri)
            .cloned();
        let Some(document) = document else {
            let report = RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: Vec::new(),
                },
            };
            return Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Full(report),
            ));
        };
        let result_id = self.diagnostic_result_id(&document, Some(document.version));
        if params.previous_result_id.as_deref() == Some(&result_id) {
            return Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                    related_documents: None,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id,
                    },
                }),
            ));
        }
        let items = self
            .lsp_diagnostics(&params.text_document.uri, &document)
            .await;
        let report = RelatedFullDocumentDiagnosticReport {
            related_documents: None,
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: Some(result_id),
                items,
            },
        };
        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(report),
        ))
    }

    async fn workspace_diagnostic(
        &self,
        params: WorkspaceDiagnosticParams,
    ) -> jsonrpc::Result<WorkspaceDiagnosticReportResult> {
        let progress_token = params.work_done_progress_params.work_done_token;
        let previous: HashMap<_, _> = params
            .previous_result_ids
            .into_iter()
            .map(|item| (item.uri, item.value))
            .collect();
        let documents = self.workspace_documents().await;
        let total = documents.len();
        self.send_progress(
            progress_token.as_ref(),
            WorkDoneProgress::Begin(WorkDoneProgressBegin {
                title: "Diagnosing OComment workspace".into(),
                cancellable: Some(true),
                message: Some(format!("0/{total} files")),
                percentage: Some(0),
            }),
        )
        .await;
        let mut items = Vec::with_capacity(documents.len());
        for (index, snapshot) in documents.into_iter().enumerate() {
            let result_id = self.diagnostic_result_id(&snapshot.document, snapshot.version);
            if previous.get(&snapshot.uri) == Some(&result_id) {
                items.push(WorkspaceDocumentDiagnosticReport::Unchanged(
                    WorkspaceUnchangedDocumentDiagnosticReport {
                        uri: snapshot.uri,
                        version: snapshot.version.map(i64::from),
                        unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                            result_id,
                        },
                    },
                ));
            } else {
                let diagnostics = self
                    .lsp_diagnostics(&snapshot.uri, &snapshot.document)
                    .await;
                items.push(WorkspaceDocumentDiagnosticReport::Full(
                    WorkspaceFullDocumentDiagnosticReport {
                        uri: snapshot.uri,
                        version: snapshot.version.map(i64::from),
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: Some(result_id),
                            items: diagnostics,
                        },
                    },
                ));
            }
            self.send_progress(
                progress_token.as_ref(),
                WorkDoneProgress::Report(WorkDoneProgressReport {
                    cancellable: Some(true),
                    message: Some(format!("{}/{} files", index + 1, total)),
                    percentage: Some(progress_percentage(index + 1, total)),
                }),
            )
            .await;
        }
        self.send_progress(
            progress_token.as_ref(),
            WorkDoneProgress::End(WorkDoneProgressEnd {
                message: Some(format!("{total} files diagnosed")),
            }),
        )
        .await;
        Ok(WorkspaceDiagnosticReportResult::Report(
            WorkspaceDiagnosticReport { items },
        ))
    }

    async fn hover(&self, params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let Some(document) = self.documents.read().await.get(&uri).cloned() else {
            return Ok(None);
        };
        let encoding = self.encoding.read().await.clone();
        let Some(offset) = position_to_byte(
            document.text.as_bytes(),
            params.text_document_position_params.position,
            &encoding,
        ) else {
            return Ok(None);
        };
        let result = self.result(&uri, &document).await;
        let Some(comment) = result
            .report
            .comments
            .iter()
            .find(|comment| comment.span.contains(offset) || comment.span.end == offset)
        else {
            return Ok(None);
        };
        let text = match &comment.disposition {
            Disposition::Remove => format!("OComment: {}", removable_label(comment.kind)),
            Disposition::Keep { reason } => {
                format!("OComment: {}", kept_label(comment.kind, reason))
            }
        };
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(text)),
            range: Some(span_to_range(
                document.text.as_bytes(),
                comment.span,
                &encoding,
            )),
        }))
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> jsonrpc::Result<Option<CodeActionResponse>> {
        let progress_token = params.work_done_progress_params.work_done_token.clone();
        let uri = params.text_document.uri;
        let Some(document) = self.documents.read().await.get(&uri).cloned() else {
            return Ok(None);
        };
        let encoding = self.encoding.read().await.clone();
        let Some(selection) = range_to_span(document.text.as_bytes(), params.range, &encoding)
        else {
            return Ok(None);
        };
        let result = self.result(&uri, &document).await;
        if !result.report.valid {
            return Ok(None);
        }
        let selected: Vec<_> = result
            .edits
            .iter()
            .filter(|edit| {
                edit.span.intersects(selection)
                    || (selection.is_empty()
                        && (edit.span.contains(selection.start)
                            || edit.span.end == selection.start))
            })
            .map(|edit| edit.span)
            .collect();
        let mut actions = Vec::new();
        if let Some(span) = selected.first().copied() {
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "Remove this comment".into(),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(
                    self.document_workspace_edit(&uri, &document, Some(&[span]))
                        .await,
                ),
                is_preferred: Some(true),
                ..CodeAction::default()
            }));
        }
        if selected.len() > 1 {
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: "Remove comments in selection".into(),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(
                    self.document_workspace_edit(&uri, &document, Some(&selected))
                        .await,
                ),
                ..CodeAction::default()
            }));
        }
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Remove all comments in document".into(),
            kind: Some(CodeActionKind::new("source.fixAll.ocomment")),
            edit: Some(self.document_workspace_edit(&uri, &document, None).await),
            ..CodeAction::default()
        }));
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Remove comments in workspace".into(),
            kind: Some(CodeActionKind::new("source.fixAll.ocomment")),
            edit: Some(self.all_documents_workspace_edit(progress_token).await),
            ..CodeAction::default()
        }));
        Ok(Some(actions))
    }

    async fn code_lens(&self, params: CodeLensParams) -> jsonrpc::Result<Option<Vec<CodeLens>>> {
        if !self.configuration.read().await.config.lsp.code_lens {
            return Ok(None);
        }
        let uri = params.text_document.uri;
        let Some(document) = self.documents.read().await.get(&uri).cloned() else {
            return Ok(None);
        };
        let count = self.result(&uri, &document).await.edits.len();
        if count == 0 {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![CodeLens {
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            command: Some(tower_lsp::lsp_types::Command {
                title: format!("Remove {count} comments"),
                command: "ocomment.fixDocument".into(),
                arguments: Some(vec![json!(uri)]),
            }),
            data: None,
        }]))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> jsonrpc::Result<Option<Value>> {
        let progress_token = params.work_done_progress_params.work_done_token.clone();
        let edit = if params.command == "ocomment.fixWorkspace" {
            self.all_documents_workspace_edit(progress_token).await
        } else if params.command == "ocomment.fixDocument" {
            let Some(uri) = params
                .arguments
                .first()
                .and_then(|value| value.as_str())
                .and_then(|value| Url::parse(value).ok())
            else {
                return Ok(None);
            };
            let Some(document) = self.documents.read().await.get(&uri).cloned() else {
                return Ok(None);
            };
            self.document_workspace_edit(&uri, &document, None).await
        } else {
            return Ok(None);
        };
        let response = self.client.apply_edit(edit).await?;
        Ok(Some(
            json!({"applied": response.applied, "failureReason": response.failure_reason}),
        ))
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.reload_configuration().await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        if params.changes.iter().any(|change| {
            change.uri.path().ends_with("/.ocomment.toml")
                || change.uri.path().ends_with("/.ocomment.lock")
        }) {
            self.reload_configuration().await;
        }
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let mut folders = self.workspace_folders.write().await;
        folders.retain(|folder| {
            !params
                .event
                .removed
                .iter()
                .any(|removed| removed.uri == folder.uri)
        });
        folders.extend(params.event.added);
    }
}

fn plugin_failure(source: &[u8], error: anyhow::Error) -> TransformResult {
    TransformResult {
        output: source.to_vec(),
        edits: Vec::new(),
        report: ScanReport {
            language: Language::Unknown,
            comments: Vec::new(),
            diagnostics: vec![CoreDiagnostic {
                code: "plugin-error".into(),
                message: format!("scanner plugin failed: {error:#}"),
                severity: Severity::Error,
                span: ByteSpan::new(0, 0),
            }],
            valid: false,
        },
        source_map: SourceMap::default(),
    }
}

fn unchanged_result(source: &[u8], language: Language) -> TransformResult {
    TransformResult {
        output: source.to_vec(),
        edits: Vec::new(),
        report: ScanReport {
            language,
            comments: Vec::new(),
            diagnostics: Vec::new(),
            valid: true,
        },
        source_map: SourceMap::default(),
    }
}

fn language_from_lsp(id: &str, uri: &Url, source: &[u8]) -> (Language, Dialect) {
    match id.to_ascii_lowercase().as_str() {
        "javascriptreact" => (Language::JavaScript, Dialect::Jsx),
        "typescriptreact" => (Language::TypeScript, Dialect::Tsx),
        "objective-c" => (Language::C, Dialect::ObjectiveC),
        "objective-cpp" => (Language::Cpp, Dialect::ObjectiveCpp),
        "cuda-cpp" => (Language::Cpp, Dialect::Cuda),
        /* NOTE: One editor id covers sh, Bash, and zsh alike, and the dialects
         * differ — `$'...'` is an ANSI-C quoted string in the last two only.
         * The id settles the language, so the dialect is taken from the path
         * and the bytes whenever they agree it is a shell script at all, and
         * falls back to the language default when a buffer offers neither. */
        "shellscript" => (
            Language::Shell,
            detected_dialect(uri, source, Language::Shell).unwrap_or(Dialect::Standard),
        ),
        id => id
            .parse()
            .map(|language| (language, Dialect::Standard))
            .unwrap_or_else(|_| {
                let path = uri.to_file_path().ok();
                detect_language(path.as_deref(), source)
                    .map(|value| (value.language, value.dialect))
                    .unwrap_or((Language::Unknown, Dialect::Standard))
            }),
    }
}

/// The dialect the path and the bytes imply, when they agree with the language
/// the client named.
fn detected_dialect(uri: &Url, source: &[u8], language: Language) -> Option<Dialect> {
    let path = uri.to_file_path().ok();
    detect_language(path.as_deref(), source)
        .filter(|detection| detection.language == language)
        .map(|detection| detection.dialect)
}

fn incremental_for_document(
    uri: &Url,
    document: &Document,
    configuration: &ResolvedConfig,
) -> Option<IncrementalDocument> {
    if document.language == Language::Unknown
        || configuration
            .config
            .languages
            .get(document.language.as_str())
            .and_then(|language| language.enabled)
            == Some(false)
    {
        return None;
    }
    let path = uri
        .to_file_path()
        .unwrap_or_else(|_| PathBuf::from(uri.path()));
    let (language, options) = configuration.for_path(&path, document.language, document.dialect);
    (language != Language::Unknown).then(|| {
        IncrementalDocument::new(
            document.text.as_bytes().to_vec(),
            language,
            options.scan,
            i64::from(document.version),
        )
    })
}

fn annotated_workspace_edit(
    entries: Vec<WorkspaceEditEntry>,
    encoding: &PositionEncodingKind,
) -> WorkspaceEdit {
    let annotation = "ocomment.remove".to_string();
    let documents = entries
        .into_iter()
        .filter_map(|entry| {
            if entry.edits.is_empty() {
                return None;
            }
            Some(TextDocumentEdit {
                text_document: OptionalVersionedTextDocumentIdentifier {
                    uri: entry.uri,
                    version: entry.version,
                },
                edits: entry
                    .edits
                    .into_iter()
                    .map(|edit| {
                        OneOf::Right(AnnotatedTextEdit {
                            text_edit: TextEdit {
                                range: span_to_range(entry.text.as_bytes(), edit.span, encoding),
                                new_text: String::from_utf8_lossy(&edit.replacement).into_owned(),
                            },
                            annotation_id: annotation.clone(),
                        })
                    })
                    .collect(),
            })
        })
        .collect();
    let mut annotations = HashMap::new();
    annotations.insert(
        annotation,
        ChangeAnnotation {
            label: "Remove comments with OComment".into(),
            needs_confirmation: Some(false),
            description: Some("Byte-safe comment transformation".into()),
        },
    );
    WorkspaceEdit {
        changes: None,
        document_changes: Some(DocumentChanges::Edits(documents)),
        change_annotations: Some(annotations),
    }
}

fn progress_percentage(completed: usize, total: usize) -> u32 {
    completed
        .saturating_mul(100)
        .checked_div(total)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(100)
        .min(100)
}

fn stable_text_hash(bytes: &[u8]) -> u64 {
    // NOTE: FNV-1a is sufficient for an opaque, session-local LSP result identifier.
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn span_to_range(source: &[u8], span: ByteSpan, encoding: &PositionEncodingKind) -> Range {
    Range::new(
        byte_to_position(source, span.start, encoding),
        byte_to_position(source, span.end, encoding),
    )
}

fn range_to_span(source: &[u8], range: Range, encoding: &PositionEncodingKind) -> Option<ByteSpan> {
    Some(ByteSpan::new(
        position_to_byte(source, range.start, encoding)?,
        position_to_byte(source, range.end, encoding)?,
    ))
}

fn byte_to_position(source: &[u8], offset: usize, encoding: &PositionEncodingKind) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut start = 0usize;
    while let Some(next) = next_line_start(source, start) {
        if next > offset {
            break;
        }
        line += 1;
        start = next;
    }
    let slice = &source[start..offset];
    let character = if *encoding == PositionEncodingKind::UTF8 {
        slice.len() as u32
    } else if let Ok(text) = std::str::from_utf8(slice) {
        if *encoding == PositionEncodingKind::UTF32 {
            text.chars().count() as u32
        } else {
            text.encode_utf16().count() as u32
        }
    } else {
        slice.len() as u32
    };
    Position::new(line, character)
}

fn position_to_byte(
    source: &[u8],
    position: Position,
    encoding: &PositionEncodingKind,
) -> Option<usize> {
    let mut start = 0usize;
    for _ in 0..position.line {
        start = next_line_start(source, start)?;
    }
    let end = source[start..]
        .iter()
        .position(|byte| matches!(*byte, b'\r' | b'\n'))
        .map_or(source.len(), |relative| start + relative);
    if *encoding == PositionEncodingKind::UTF8 {
        let offset = start.checked_add(position.character as usize)?;
        return (offset <= end && std::str::from_utf8(&source[start..offset]).is_ok())
            .then_some(offset);
    }
    let text = std::str::from_utf8(&source[start..end]).ok()?;
    let mut units = 0u32;
    for (relative, character) in text.char_indices() {
        if units == position.character {
            return Some(start + relative);
        }
        units += if *encoding == PositionEncodingKind::UTF32 {
            1
        } else {
            character.len_utf16() as u32
        };
        if units > position.character {
            return None;
        }
    }
    (units == position.character).then_some(end)
}

fn next_line_start(source: &[u8], start: usize) -> Option<usize> {
    let relative = source
        .get(start..)?
        .iter()
        .position(|byte| matches!(*byte, b'\r' | b'\n'))?;
    let ending = start + relative;
    Some(
        if source.get(ending) == Some(&b'\r') && source.get(ending + 1) == Some(&b'\n') {
            ending + 2
        } else {
            ending + 1
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn position_round_trip_all_encodings() {
        let source = "a😀b\rnext\r\n二\nlast".as_bytes();
        for encoding in [
            PositionEncodingKind::UTF8,
            PositionEncodingKind::UTF16,
            PositionEncodingKind::UTF32,
        ] {
            for offset in [0, 1, 5, 6, 7, 11, 13, 16, 17, 21] {
                let position = byte_to_position(source, offset, &encoding);
                assert_eq!(position_to_byte(source, position, &encoding), Some(offset));
            }
        }
    }
}
