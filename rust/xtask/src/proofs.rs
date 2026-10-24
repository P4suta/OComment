use crate::proof_rules::{Evidence, admit};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Component, Path},
    process::Command,
};

const INVENTORY: &str = "proofs/inventory.toml";
const TOOL_CONFIG: &str = "proofs/mise.toml";
const KANI_TOOL: &str = "cargo:kani-verifier";
const INPUT_LIMIT: usize = 4 * 1024 * 1024;
const REPORT_LIMIT: usize = 32 * 1024 * 1024;
const HARNESS_LIMIT: usize = 64;
const SOURCE_LIMIT: usize = 8;
const SUPPORTED_TARGET: &str = "x86_64-unknown-linux-gnu";
const CBMC_VERSION: &str = "6.11.0 (cbmc-6.11.0)";
const CHECKED_RUSTFLAGS: &str = "--edition=2024 -C overflow-checks=yes -C debug-assertions=yes -Z mir-opt-level=3 -Z mir-enable-passes=-Inline,-JumpThreading -Zcrate-attr=feature(stmt_expr_attributes,proc_macro_hygiene)";

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    version: u32,
    harness: String,
    sources: Vec<Source>,
    harnesses: Vec<Harness>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    module: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Harness {
    name: String,
    kind: Kind,
    covers: Option<usize>,
    #[serde(default)]
    assertions: Vec<String>,
    assertion: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    Proof,
    Counterexample,
}

#[derive(Debug, Deserialize)]
struct Export {
    metadata: Metadata,
    tools: Tools,
    harness_metadata: Vec<HarnessMetadata>,
    verification_results: Verification,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    version: String,
    kani_version: String,
    target: String,
}

#[derive(Debug, Deserialize)]
struct Tools {
    kani: String,
    cbmc: String,
}

#[derive(Debug, Deserialize)]
struct HarnessMetadata {
    pretty_name: String,
    attributes: Attributes,
}

#[derive(Debug, Deserialize)]
struct Attributes {
    kind: String,
    should_panic: bool,
}

#[derive(Debug, Deserialize)]
struct Verification {
    summary: Summary,
    results: Vec<HarnessResult>,
}

#[derive(Debug, Deserialize)]
struct Summary {
    total_harnesses: usize,
    executed: usize,
    status: String,
    successful: usize,
    failed: usize,
}

#[derive(Debug, Deserialize)]
struct HarnessResult {
    harness_id: String,
    status: String,
    checks: Vec<Property>,
}

#[derive(Debug, Deserialize)]
struct Property {
    category: String,
    status: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolConfig {
    tools: BTreeMap<String, String>,
}

struct Input {
    path: String,
    bytes: Vec<u8>,
}

struct Plan {
    inventory: Inventory,
    version: String,
    inputs: Vec<Input>,
}

pub(crate) fn run(root: &Path, arguments: &[String]) -> Result<()> {
    match arguments {
        [] => verify(root),
        [argument] if argument == "--check" => check(root),
        _ => bail!("use `cargo xtask proofs` or `cargo xtask proofs --check`"),
    }
}

fn read_capped(path: &Path, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "{} must be a regular file",
        path.display()
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(u64::try_from(limit)? + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "{} exceeds the evidence byte limit",
        path.display()
    );
    Ok(bytes)
}

fn input(root: &Path, relative: &str) -> Result<Input> {
    let path = Path::new(relative);
    ensure!(
        !relative.is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "proof input must stay inside the repository: {relative}"
    );
    let path = root.join(path);
    ensure!(
        path.canonicalize()?.starts_with(root.canonicalize()?),
        "proof input escapes the repository"
    );
    Ok(Input {
        path: relative.into(),
        bytes: read_capped(&path, INPUT_LIMIT)?,
    })
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn load(root: &Path) -> Result<Plan> {
    let declaration = input(root, INVENTORY)?;
    let inventory: Inventory = toml::from_str(std::str::from_utf8(&declaration.bytes)?)?;
    ensure!(
        inventory.version == 1
            && !inventory.sources.is_empty()
            && inventory.sources.len() <= SOURCE_LIMIT
            && !inventory.harnesses.is_empty()
            && inventory.harnesses.len() <= HARNESS_LIMIT,
        "proof inventory is empty, oversized, or unsupported"
    );
    let mut modules = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for source in &inventory.sources {
        ensure!(
            identifier(&source.module)
                && modules.insert(source.module.as_str())
                && paths.insert(source.path.as_str()),
            "duplicate or invalid proof source"
        );
    }
    let mut names = BTreeSet::new();
    let mut assertions = BTreeSet::new();
    for harness in &inventory.harnesses {
        ensure!(
            identifier(&harness.name) && names.insert(harness.name.as_str()),
            "duplicate or invalid proof harness"
        );
        let mut contracts = BTreeSet::new();
        match harness.kind {
            Kind::Proof => ensure!(
                harness
                    .covers
                    .is_some_and(|covers| (1..=HARNESS_LIMIT).contains(&covers))
                    && harness.assertion.is_none()
                    && !harness.assertions.is_empty()
                    && harness.assertions.len() <= HARNESS_LIMIT
                    && harness
                        .assertions
                        .iter()
                        .all(|assertion| { !assertion.is_empty() && contracts.insert(assertion) }),
                "proof {} needs named assertions and reachable outcome witnesses",
                harness.name
            ),
            Kind::Counterexample => ensure!(
                harness.covers.is_none()
                    && harness.assertions.is_empty()
                    && harness
                        .assertion
                        .as_deref()
                        .is_some_and(|assertion| assertion.starts_with("probe: ")
                            && assertion.len() > 12
                            && assertions.insert(assertion)),
                "counterexample {} needs its own assertion marker",
                harness.name
            ),
        }
    }
    ensure!(
        inventory
            .harnesses
            .iter()
            .any(|harness| harness.kind == Kind::Proof)
            && inventory
                .harnesses
                .iter()
                .any(|harness| harness.kind == Kind::Counterexample),
        "the inventory needs proofs and rejecting probes"
    );
    let tool = input(root, TOOL_CONFIG)?;
    let config: ToolConfig = toml::from_str(std::str::from_utf8(&tool.bytes)?)?;
    let version = config
        .tools
        .get(KANI_TOOL)
        .context("missing pinned Kani tool")?
        .clone();
    let parts: Vec<_> = version.split('.').collect();
    ensure!(
        config.tools.len() == 1
            && parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "the proof verifier needs one exact version"
    );
    let mut inputs = Vec::new();
    for source in &inventory.sources {
        inputs.push(input(root, &source.path)?);
    }
    inputs.push(input(root, &inventory.harness)?);
    inputs.push(declaration);
    inputs.push(tool);
    Ok(Plan {
        inventory,
        version,
        inputs,
    })
}

fn format_check(root: &Path, plan: &Plan) -> Result<()> {
    let status = Command::new("rustfmt")
        .current_dir(root)
        .args(["--check", "--edition", "2024", &plan.inventory.harness])
        .status()?;
    ensure!(
        status.success(),
        "proof harness formatting or syntax failed"
    );
    Ok(())
}

pub(crate) fn check(root: &Path) -> Result<()> {
    let plan = load(root)?;
    format_check(root, &plan)?;
    println!(
        "Validated {} proof inventory entries; no proof execution",
        plan.inventory.harnesses.len()
    );
    Ok(())
}

fn verify(root: &Path) -> Result<()> {
    let plan = load(root)?;
    format_check(root, &plan)?;
    let parent = root.join("rust/target/assurance");
    fs::create_dir_all(&parent)?;
    let directory = tempfile::Builder::new()
        .prefix("keep-blocks-")
        .tempdir_in(parent)?
        .keep();
    let mut wrapper = String::new();
    for (index, source) in plan.inventory.sources.iter().enumerate() {
        let filename = format!("source-{index}.rs");
        let bytes = &plan
            .inputs
            .iter()
            .find(|input| input.path == source.path)
            .context("source snapshot is missing")?
            .bytes;
        fs::write(directory.join(&filename), bytes)?;
        wrapper.push_str(&format!("#[path = {filename:?}] mod {};\n", source.module));
    }
    let harness = plan
        .inputs
        .iter()
        .find(|input| input.path == plan.inventory.harness)
        .context("harness snapshot is missing")?;
    fs::write(directory.join("harness.rs"), &harness.bytes)?;
    wrapper.push_str("include!(\"harness.rs\");\n");
    fs::write(directory.join("subject.rs"), wrapper)?;
    let names: Vec<_> = plan
        .inventory
        .harnesses
        .iter()
        .map(|harness| harness.name.as_str())
        .collect();
    let status = Command::new("kani")
        .current_dir(&directory)
        .args([
            "subject.rs",
            "--target-dir",
            ".",
            "--output-format",
            "terse",
            "-Z",
            "unstable-options",
            "--jobs",
            "1",
            "--harness-timeout",
            "60s",
            "--export-json",
            "results.json",
            "--cbmc-args",
            "--object-bits",
            "12",
        ])
        .env("RUSTFLAGS", CHECKED_RUSTFLAGS)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .with_context(|| {
            format!(
                "Kani did not run; unfinished harnesses: {}",
                names.join(", ")
            )
        })?;
    let report = read_capped(&directory.join("results.json"), REPORT_LIMIT).with_context(|| {
        format!(
            "Kani supplied no complete report; unfinished harnesses: {}",
            names.join(", ")
        )
    })?;
    let export: Export =
        serde_json::from_slice(&report).context("Kani report is not valid structured evidence")?;
    let unchanged = plan.inputs.iter().all(|input| {
        read_capped(&root.join(&input.path), INPUT_LIMIT)
            .is_ok_and(|current| current == input.bytes)
    });
    validate(
        &plan.inventory,
        &plan.version,
        &export,
        status.success(),
        unchanged,
    )
    .with_context(|| format!("proof artifacts: {}", directory.display()))?;
    for input in &plan.inputs {
        let digest: String = Sha256::digest(&input.bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        println!("proof input {} sha256={digest}", input.path);
    }
    let proofs = plan
        .inventory
        .harnesses
        .iter()
        .filter(|harness| harness.kind == Kind::Proof)
        .count();
    println!(
        "Verified {proofs} proof harnesses and rejected {} false claims",
        plan.inventory.harnesses.len() - proofs
    );
    Ok(())
}

fn properties(expected: &Harness, actual: &HarnessResult) -> bool {
    if actual.status != "Success" || actual.checks.is_empty() {
        return false;
    }
    match expected.kind {
        Kind::Proof => {
            actual.checks.iter().all(|check| {
                check.status == "Success"
                    || check.status == "Unreachable"
                    || check.category == "cover" && check.status == "Satisfied"
            }) && !expected.assertions.is_empty()
                && expected.assertions.iter().all(|marker| {
                    actual.checks.iter().any(|check| {
                        check.category == "assertion"
                            && check.status == "Success"
                            && check.description.trim_matches('"') == marker
                    })
                })
                && expected.covers.is_some_and(|count| {
                    actual
                        .checks
                        .iter()
                        .filter(|check| check.category == "cover" && check.status == "Satisfied")
                        .count()
                        >= count
                })
        }
        Kind::Counterexample => {
            let failed: Vec<_> = actual
                .checks
                .iter()
                .filter(|check| !matches!(check.status.as_str(), "Success" | "Unreachable"))
                .collect();
            failed.len() == 1
                && failed[0].status == "Failure"
                && failed[0].category == "assertion"
                && expected
                    .assertion
                    .as_ref()
                    .is_some_and(|marker| failed[0].description.trim_matches('"') == marker)
        }
    }
}

fn validate(
    inventory: &Inventory,
    version: &str,
    export: &Export,
    process: bool,
    source: bool,
) -> Result<()> {
    let expected: BTreeSet<_> = inventory
        .harnesses
        .iter()
        .map(|harness| harness.name.as_str())
        .collect();
    let summary = &export.verification_results.summary;
    let observed: BTreeSet<_> = export
        .verification_results
        .results
        .iter()
        .map(|result| result.harness_id.as_str())
        .collect();
    let metadata: BTreeSet<_> = export
        .harness_metadata
        .iter()
        .map(|harness| harness.pretty_name.as_str())
        .collect();
    let attributes = export.harness_metadata.iter().all(|actual| {
        inventory
            .harnesses
            .iter()
            .find(|expected| expected.name == actual.pretty_name)
            .is_some_and(|expected| {
                actual.attributes.kind == "Proof"
                    && actual.attributes.should_panic == (expected.kind == Kind::Counterexample)
            })
    });
    let established = inventory.harnesses.iter().all(|expected| {
        export
            .verification_results
            .results
            .iter()
            .find(|actual| actual.harness_id == expected.name)
            .is_some_and(|actual| properties(expected, actual))
    });
    let evidence = Evidence {
        process,
        completed: !expected.is_empty()
            && summary.status == "completed"
            && summary.total_harnesses == expected.len()
            && summary.executed == expected.len()
            && summary.successful == expected.len()
            && summary.failed == 0,
        identities: expected.len() == inventory.harnesses.len()
            && expected == observed
            && expected == metadata
            && observed.len() == export.verification_results.results.len()
            && metadata.len() == export.harness_metadata.len()
            && attributes,
        version: export.metadata.version == "1.0"
            && export.metadata.kani_version == version
            && export.tools.kani == version
            && export.tools.cbmc == CBMC_VERSION
            && export.metadata.target == SUPPORTED_TARGET,
        properties: established,
        source,
    };
    ensure!(
        admit(evidence),
        "proof evidence is incomplete or invalid for [{}]: {evidence:?}",
        expected.into_iter().collect::<Vec<_>>().join(", ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> (Inventory, Value) {
        let inventory = Inventory {
            version: 1,
            harness: "proofs/test.rs".into(),
            sources: vec![],
            harnesses: vec![
                Harness {
                    name: "proved".into(),
                    kind: Kind::Proof,
                    covers: Some(2),
                    assertions: vec!["the contract".into()],
                    assertion: None,
                },
                Harness {
                    name: "refuted".into(),
                    kind: Kind::Counterexample,
                    covers: None,
                    assertions: vec![],
                    assertion: Some("probe: expected contradiction".into()),
                },
            ],
        };
        let export = json!({
            "metadata": {"version":"1.0", "kani_version":"0.68.0", "target":"x86_64-unknown-linux-gnu"},
            "tools": {"kani":"0.68.0", "cbmc":"6.11.0 (cbmc-6.11.0)"},
            "harness_metadata": [
                {"pretty_name":"proved", "attributes":{"kind":"Proof", "should_panic":false}},
                {"pretty_name":"refuted", "attributes":{"kind":"Proof", "should_panic":true}}
            ],
            "verification_results": {
                "summary":{"total_harnesses":2,"executed":2,"status":"completed","successful":2,"failed":0},
                "results":[
                    {"harness_id":"proved","status":"Success","checks":[
                        {"category":"assertion","status":"Success","description":"the contract"},
                        {"category":"cover","status":"Satisfied","description":"accepted"},
                        {"category":"cover","status":"Satisfied","description":"refused"}
                    ]},
                    {"harness_id":"refuted","status":"Success","checks":[
                        {"category":"assertion","status":"Failure","description":"probe: expected contradiction"}
                    ]}
                ]
            }
        });
        (inventory, export)
    }

    fn accepts(inventory: &Inventory, value: Value, process: bool, source: bool) -> bool {
        let Ok(export) = serde_json::from_value(value) else {
            return false;
        };
        validate(inventory, "0.68.0", &export, process, source).is_ok()
    }

    #[test]
    fn complete_matching_evidence_is_accepted() {
        let (inventory, export) = fixture();
        assert!(accepts(&inventory, export, true, true));
    }

    #[test]
    fn missing_execution_or_changed_source_never_receives_credit() {
        let (inventory, export) = fixture();
        assert!(!accepts(&inventory, export.clone(), false, true));
        assert!(!accepts(&inventory, export, true, false));
    }

    #[test]
    fn incomplete_or_inconsistent_summaries_refuse() {
        let (inventory, export) = fixture();
        for field in ["executed", "total_harnesses", "successful", "failed"] {
            let mut value = export.clone();
            value["verification_results"]["summary"][field] = json!(99);
            assert!(!accepts(&inventory, value, true, true), "{field}");
        }
        let mut value = export;
        value["verification_results"]["summary"]["status"] = json!("incomplete");
        assert!(!accepts(&inventory, value, true, true));
    }

    #[test]
    fn duplicate_unknown_or_missing_harnesses_refuse() {
        let (inventory, export) = fixture();
        for name in ["proved", "unknown"] {
            let mut value = export.clone();
            value["verification_results"]["results"][1]["harness_id"] = json!(name);
            assert!(!accepts(&inventory, value, true, true), "{name}");
        }
        let mut value = export;
        value["harness_metadata"] = json!([]);
        assert!(!accepts(&inventory, value, true, true));
    }

    #[test]
    fn wrong_verifier_protocol_or_target_refuses() {
        let (inventory, export) = fixture();
        for (field, wrong) in [
            ("kani_version", "0.67.0"),
            ("version", "unknown"),
            ("target", "i686-unknown-linux-gnu"),
        ] {
            let mut value = export.clone();
            value["metadata"][field] = json!(wrong);
            assert!(!accepts(&inventory, value, true, true), "{field}");
        }
    }

    #[test]
    fn missing_witnesses_empty_checks_and_unfinished_properties_refuse() {
        let (inventory, export) = fixture();
        let mut value = export.clone();
        value["verification_results"]["results"][0]["checks"] = json!([]);
        assert!(!accepts(&inventory, value, true, true));
        for status in ["Unsatisfiable", "Undetermined", "SolverError"] {
            let mut value = export.clone();
            value["verification_results"]["results"][0]["checks"][1]["status"] = json!(status);
            assert!(!accepts(&inventory, value, true, true), "{status}");
        }
    }

    #[test]
    fn counterexamples_require_the_named_assertion_failure() {
        let (inventory, export) = fixture();
        for (field, wrong) in [
            ("category", "unwinding"),
            ("description", "index out of bounds"),
            ("status", "Success"),
        ] {
            let mut value = export.clone();
            value["verification_results"]["results"][1]["checks"][0][field] = json!(wrong);
            assert!(!accepts(&inventory, value, true, true), "{field}");
        }
    }

    #[test]
    fn panic_expectations_cannot_replace_positive_proofs() {
        let (inventory, mut export) = fixture();
        export["harness_metadata"][0]["attributes"]["should_panic"] = json!(true);
        assert!(!accepts(&inventory, export, true, true));
    }

    #[test]
    fn unreachable_library_assertions_do_not_mask_real_evidence() {
        let (inventory, mut export) = fixture();
        for index in 0..2 {
            export["verification_results"]["results"][index]["checks"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "category":"assertion", "status":"Unreachable", "description":"unused library branch"
                }));
        }
        assert!(accepts(&inventory, export, true, true));
    }

    #[test]
    fn a_dead_generic_branch_still_requires_every_declared_outcome() {
        let (inventory, mut export) = fixture();
        export["verification_results"]["results"][0]["checks"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "category":"cover", "status":"Unreachable", "description":"a different constant input length"
            }));
        assert!(accepts(&inventory, export.clone(), true, true));
        export["verification_results"]["results"][0]["checks"][1]["status"] = json!("Unreachable");
        assert!(!accepts(&inventory, export, true, true));
    }

    #[test]
    fn an_unrelated_assertion_cannot_replace_the_contract() {
        let (inventory, mut export) = fixture();
        export["verification_results"]["results"][0]["checks"][0]["description"] =
            json!("an unrelated library check");
        assert!(!accepts(&inventory, export, true, true));
    }

    #[test]
    fn evidence_inputs_refuse_escapes_missing_files_and_excess_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(root.join("subject.rs"), b"actual source").unwrap();
        assert_eq!(input(root, "subject.rs").unwrap().bytes, b"actual source");
        for path in ["../subject.rs", "", "missing.rs"] {
            assert!(input(root, path).is_err(), "{path}");
        }
        assert!(input(root, root.join("subject.rs").to_str().unwrap()).is_err());
        assert!(read_capped(&root.join("subject.rs"), 3).is_err());
        assert!(read_capped(root, INPUT_LIMIT).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("subject.rs"), root.join("link.rs")).unwrap();
            assert!(input(root, "link.rs").is_err());
        }
    }

    #[test]
    fn the_real_inventory_loads_and_its_malformed_variants_refuse() {
        let plan = load(&crate::root()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for input in &plan.inputs {
            let path = root.join(&input.path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, &input.bytes).unwrap();
        }
        assert!(load(root).is_ok());
        let declaration = fs::read_to_string(root.join(INVENTORY)).unwrap();
        let original: toml::Value = toml::from_str(&declaration).unwrap();
        for list in ["sources", "harnesses"] {
            let mut value = original.clone();
            let array = value[list].as_array_mut().unwrap();
            array.push(array[0].clone());
            fs::write(root.join(INVENTORY), toml::to_string(&value).unwrap()).unwrap();
            let error = load(root).err().expect("a duplicate inventory must fail");
            assert!(error.to_string().contains("duplicate"));
        }
        let mut value = original;
        value["harnesses"][0]["assertions"] = toml::Value::Array(vec![]);
        fs::write(root.join(INVENTORY), toml::to_string(&value).unwrap()).unwrap();
        assert!(load(root).is_err());
        fs::write(root.join(INVENTORY), declaration).unwrap();
        fs::write(
            root.join(TOOL_CONFIG),
            "[tools]\n\"cargo:kani-verifier\" = \"latest\"\n",
        )
        .unwrap();
        assert!(load(root).is_err());
    }
}
