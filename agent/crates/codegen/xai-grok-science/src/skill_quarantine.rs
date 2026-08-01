//! Actor-gated, store-owned quarantine for uploaded `.zip` / `.skill` bundles.
//!
//! Archive inspection is pure and bounded. It never extracts to disk. Product
//! callers retain the admitted bytes inside `SessionActor`, obtain the normal
//! production permission decision, and only then call [`finish_quarantine`].
//! Success stores the original archive and a deterministic manifest as hashed
//! Science artifacts; it never materializes or enables a live skill.
//!
//! The bounded ZIP admission rules are a Rust, fail-closed adaptation of the
//! archive-reader concerns in AIPOCH Open Science's
//! `src/main/skills/zip-extract.ts` at `fd2853f0b9bdb6c063ccc1e741687584ab94bf9a`
//! (Apache-2.0; SHA-256
//! `613b5ae735796472e477d041d0525c248799087ccb4aeaf1251a3dc17bed9bed`).
//! Unlike that UI-oriented lenient extractor, this authority path rejects an
//! ambiguous, unsafe, malformed, unsupported, or over-budget archive before a
//! run is created. Open Science's materializer is deliberately not adopted:
//! quarantine records immutable evidence and never writes an enabled skill.

use crate::{
    Approval, ApprovalDecision, Artifact, CallId, Evidence, Provenance, Result, RunContext, RunId,
    RunRecord, RunState, ScienceError, ScienceStore, csv::ScienceRunTicket,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Component, Path},
};
use zip::{CompressionMethod, ZipArchive};

const ORIGINAL_ARTIFACT_PATH: &str = "quarantine/original.skill";
const MANIFEST_ARTIFACT_PATH: &str = "quarantine/manifest.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillArchiveLimits {
    pub max_archive_bytes: usize,
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_depth: usize,
    pub max_skills: usize,
    pub max_expansion_ratio: u64,
}

impl Default for SkillArchiveLimits {
    fn default() -> Self {
        Self {
            // ACP carries the archive as base64 and both encoded and decoded
            // forms exist briefly. Keep this product seam deliberately below
            // the legacy Desktop's 256 MiB direct-writer allowance.
            // The ACP transport is newline-delimited JSON capped at 64 MiB.
            // A 32 MiB archive expands to about 42.7 MiB as canonical base64,
            // leaving room for the request envelope without a loose ingress
            // file outside SessionActor.
            max_archive_bytes: 32 * 1024 * 1024,
            max_entries: 1_024,
            max_file_bytes: 16 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024,
            max_depth: 8,
            max_skills: 128,
            max_expansion_ratio: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillQuarantineRequest {
    pub operation_id: String,
    pub selected_subpaths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillManifestFile {
    pub archive_path: String,
    pub relative_path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillQuarantineManifest {
    pub schema_version: u32,
    pub recorded_at: Option<DateTime<Utc>>,
    pub operation_id: String,
    pub archive_sha256: String,
    pub admission_sha256: String,
    pub selected_subpaths: Vec<String>,
    pub files: Vec<SkillManifestFile>,
    pub total_uncompressed_bytes: u64,
    pub materialized: bool,
    pub enabled: bool,
}

/// Immutable in-memory capability produced by bounded archive inspection.
///
/// All fields are private so an ACP adapter cannot assert hashes, selected
/// roots, or parsed file metadata. The retained archive is re-inspected after
/// Allow before any artifact is committed.
#[derive(Debug, Clone)]
pub struct SkillImportAdmission {
    request: SkillQuarantineRequest,
    limits: SkillArchiveLimits,
    archive_bytes: Vec<u8>,
    manifest: SkillQuarantineManifest,
}

impl SkillImportAdmission {
    pub fn archive_sha256(&self) -> &str {
        &self.manifest.archive_sha256
    }

    pub fn sha256(&self) -> &str {
        &self.manifest.admission_sha256
    }

    pub fn operation_id(&self) -> &str {
        &self.request.operation_id
    }

    pub fn selected_subpaths(&self) -> &[String] {
        &self.manifest.selected_subpaths
    }

    pub fn file_count(&self) -> usize {
        self.manifest.files.len()
    }

    pub fn total_uncompressed_bytes(&self) -> u64 {
        self.manifest.total_uncompressed_bytes
    }

    pub fn manifest(&self) -> &SkillQuarantineManifest {
        &self.manifest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillQuarantineResult {
    pub run: RunRecord,
    pub operation_id: String,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub approvals: Vec<Approval>,
    pub replay_after: u64,
}

/// Stable authority run id for one owner/project/session operation key.
///
/// The archive digest is deliberately not part of the id: reusing an
/// operation key with different bytes must collide and fail closed rather
/// than silently create a second operation.
pub fn operation_run_id(
    owner_id: &str,
    project_id: &crate::ProjectId,
    session_id: &str,
    operation_id: &str,
) -> RunId {
    let digest = hex_sha256(
        serde_json::json!({
            "kind": "skill_quarantine_import",
            "ownerId": owner_id,
            "projectId": project_id.0,
            "sessionId": session_id,
            "operationId": operation_id,
        })
        .to_string()
        .as_bytes(),
    );
    RunId::new(format!("skillq-{}", &digest[..40]))
}

#[derive(Debug)]
struct InspectedFile {
    archive_path: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Directory,
}

/// Inspect an archive entirely in memory and return a capability whose hashes,
/// roots and file manifest were derived by this kernel.
pub fn inspect_archive(
    archive_bytes: &[u8],
    request: &SkillQuarantineRequest,
    limits: SkillArchiveLimits,
) -> Result<SkillImportAdmission> {
    validate_request(request, limits)?;
    if archive_bytes.is_empty() || archive_bytes.len() > limits.max_archive_bytes {
        return Err(ScienceError::Invalid(format!(
            "skill archive must contain 1..={} bytes",
            limits.max_archive_bytes
        )));
    }

    let archive_sha256 = hex_sha256(archive_bytes);
    let files = inspect_zip_entries(archive_bytes, limits)?;
    let roots = discover_skill_roots(&files, limits)?;
    let selected = select_roots(&roots, &request.selected_subpaths)?;
    let selected_files = manifest_selected_files(&files, &selected)?;
    let total_uncompressed_bytes = selected_files
        .iter()
        .try_fold(0u64, |sum, file| sum.checked_add(file.bytes))
        .ok_or_else(|| ScienceError::Invalid("selected skill size overflow".into()))?;

    let admission_input = serde_json::to_vec(&serde_json::json!({
        "schemaVersion": 2,
        "operationId": request.operation_id,
        "archiveSha256": archive_sha256,
        "selectedSubpaths": selected,
        "files": selected_files,
        "limits": limits,
    }))?;
    let admission_sha256 = hex_sha256(&admission_input);
    let manifest = SkillQuarantineManifest {
        schema_version: 2,
        recorded_at: None,
        operation_id: request.operation_id.clone(),
        archive_sha256,
        admission_sha256,
        selected_subpaths: selected,
        files: selected_files,
        total_uncompressed_bytes,
        materialized: false,
        enabled: false,
    };
    Ok(SkillImportAdmission {
        request: request.clone(),
        limits,
        archive_bytes: archive_bytes.to_vec(),
        manifest,
    })
}

fn validate_request(request: &SkillQuarantineRequest, limits: SkillArchiveLimits) -> Result<()> {
    if request.operation_id.is_empty()
        || request.operation_id.len() > 128
        || request
            .operation_id
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')))
    {
        return Err(ScienceError::Invalid(
            "operationId must be 1..=128 ASCII letters, digits, '.', '-' or '_'".into(),
        ));
    }
    if request.selected_subpaths.is_empty() || request.selected_subpaths.len() > limits.max_skills {
        return Err(ScienceError::Invalid(format!(
            "selectedSubpaths must contain 1..={} roots",
            limits.max_skills
        )));
    }
    let mut exact = BTreeSet::new();
    let mut folded = BTreeSet::new();
    for root in &request.selected_subpaths {
        validate_subpath(root, limits.max_depth)?;
        if !exact.insert(root.clone()) || !folded.insert(root.to_lowercase()) {
            return Err(ScienceError::Invalid(
                "selectedSubpaths contain a duplicate or case collision".into(),
            ));
        }
    }
    let roots: Vec<&str> = exact.iter().map(String::as_str).collect();
    for (index, root) in roots.iter().enumerate() {
        for other in roots.iter().skip(index + 1) {
            if root.is_empty()
                || other.is_empty()
                || other.starts_with(&format!("{root}/"))
                || root.starts_with(&format!("{other}/"))
            {
                return Err(ScienceError::Invalid(
                    "selected skill roots may not overlap".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_subpath(path: &str, max_depth: usize) -> Result<()> {
    if path.contains('\\')
        || path.contains('\0')
        || path.starts_with('/')
        || has_windows_drive_prefix(path)
        || path.ends_with('/')
    {
        return Err(ScienceError::Invalid(format!(
            "unsafe skill archive path: {path:?}"
        )));
    }
    if path.is_empty() {
        return Ok(());
    }
    let components: Vec<&str> = path.split('/').collect();
    if components.len() > max_depth
        || components.iter().any(|component| {
            component.is_empty()
                || matches!(*component, "." | "..")
                || component.contains(':')
                || component.ends_with(['.', ' '])
                || is_windows_reserved_component(component)
        })
    {
        return Err(ScienceError::Invalid(format!(
            "unsafe or too-deep skill archive path: {path:?}"
        )));
    }
    let parsed = Path::new(path);
    if parsed
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ScienceError::Invalid(format!(
            "unsafe skill archive path: {path:?}"
        )));
    }
    Ok(())
}

fn is_windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split_once('.')
        .map_or(component, |(stem, _extension)| stem)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|suffix| suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9'))
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn inspect_zip_entries(
    archive_bytes: &[u8],
    limits: SkillArchiveLimits,
) -> Result<Vec<InspectedFile>> {
    let declared_entry_count = declared_zip_entry_count(archive_bytes, limits)?;
    let cursor = Cursor::new(archive_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| ScienceError::Invalid(format!("invalid ZIP archive: {error}")))?;
    if archive.len() != declared_entry_count {
        return Err(ScienceError::Invalid(
            "ZIP entry registry does not match the central-directory count".into(),
        ));
    }

    let mut explicit_names = BTreeSet::<String>::new();
    let mut kinds = BTreeMap::<String, EntryKind>::new();
    let mut folded_names = BTreeMap::<String, String>::new();
    let mut folded_kinds = BTreeMap::<String, EntryKind>::new();
    let mut files = Vec::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| ScienceError::Invalid(format!("unreadable ZIP entry: {error}")))?;
        if entry.encrypted() {
            return Err(ScienceError::Invalid(
                "encrypted ZIP entries are not admitted".into(),
            ));
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(ScienceError::Invalid(format!(
                "unsupported ZIP compression for entry {}",
                index
            )));
        }
        let raw_name = std::str::from_utf8(entry.name_raw())
            .map_err(|_| ScienceError::Invalid("ZIP entry name is not UTF-8".into()))?
            .to_owned();
        let is_directory = entry.is_dir() || raw_name.ends_with('/');
        let name = raw_name.strip_suffix('/').unwrap_or(&raw_name).to_owned();
        validate_subpath(&name, limits.max_depth)?;
        if name.is_empty() {
            return Err(ScienceError::Invalid("ZIP contains an empty path".into()));
        }
        let kind = if is_directory {
            EntryKind::Directory
        } else {
            EntryKind::File
        };
        if let Some(mode) = entry.unix_mode() {
            let file_type = mode & 0o170000;
            let expected = if is_directory { 0o040000 } else { 0o100000 };
            if file_type != 0 && file_type != expected {
                return Err(ScienceError::Invalid(format!(
                    "ZIP symlink or special entry is not admitted: {name}"
                )));
            }
        }
        if !explicit_names.insert(name.clone()) {
            return Err(ScienceError::Invalid(format!(
                "duplicate ZIP entry path: {name}"
            )));
        }
        let components: Vec<&str> = name.split('/').collect();
        let mut prefix = String::new();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            register_logical_path(
                &mut kinds,
                &mut folded_names,
                &mut folded_kinds,
                &prefix,
                EntryKind::Directory,
            )?;
        }
        register_logical_path(
            &mut kinds,
            &mut folded_names,
            &mut folded_kinds,
            &name,
            kind,
        )?;
        if is_directory {
            continue;
        }
        if entry.size() > limits.max_file_bytes {
            return Err(ScienceError::Invalid(format!(
                "ZIP entry exceeds per-file cap: {name}"
            )));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| ScienceError::Invalid("ZIP total size overflow".into()))?;
        if total > limits.max_total_bytes {
            return Err(ScienceError::Invalid(
                "ZIP exceeds total uncompressed cap".into(),
            ));
        }
        let compressed = entry.compressed_size();
        if entry.size() > 0
            && (compressed == 0
                || entry.size() > compressed.saturating_mul(limits.max_expansion_ratio))
        {
            return Err(ScienceError::Invalid(format!(
                "ZIP entry exceeds expansion-ratio cap: {name}"
            )));
        }
        let declared = entry.size();
        let mut bytes =
            Vec::with_capacity(usize::try_from(declared.min(limits.max_file_bytes)).unwrap_or(0));
        entry
            .by_ref()
            .take(limits.max_file_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| {
                ScienceError::Invalid(format!("cannot decompress ZIP entry {name}: {error}"))
            })?;
        if bytes.len() as u64 != declared || bytes.len() as u64 > limits.max_file_bytes {
            return Err(ScienceError::Invalid(format!(
                "ZIP entry size does not match its bounded payload: {name}"
            )));
        }
        files.push(InspectedFile {
            archive_path: name,
            bytes,
        });
    }
    validate_file_directory_conflicts(&kinds)?;
    validate_file_directory_conflicts(&folded_kinds)?;
    files.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));
    Ok(files)
}

fn declared_zip_entry_count(archive_bytes: &[u8], limits: SkillArchiveLimits) -> Result<usize> {
    const EOCD_MIN_BYTES: usize = 22;
    const MAX_COMMENT_BYTES: usize = u16::MAX as usize;
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";

    if archive_bytes.len() < EOCD_MIN_BYTES {
        return Err(ScienceError::Invalid(
            "ZIP end-of-central-directory record is missing".into(),
        ));
    }
    let search_start = archive_bytes
        .len()
        .saturating_sub(EOCD_MIN_BYTES + MAX_COMMENT_BYTES);
    let eocd_offset = (search_start..=archive_bytes.len() - EOCD_MIN_BYTES)
        .rev()
        .find(|offset| {
            archive_bytes[*offset..].starts_with(EOCD_SIGNATURE)
                && read_u16(archive_bytes, *offset + 20).is_some_and(|comment_bytes| {
                    *offset + EOCD_MIN_BYTES + usize::from(comment_bytes) == archive_bytes.len()
                })
        })
        .ok_or_else(|| {
            ScienceError::Invalid("ZIP end-of-central-directory record is invalid".into())
        })?;

    let disk_number = read_u16(archive_bytes, eocd_offset + 4).unwrap_or(u16::MAX);
    let central_directory_disk = read_u16(archive_bytes, eocd_offset + 6).unwrap_or(u16::MAX);
    let entries_on_disk = read_u16(archive_bytes, eocd_offset + 8).unwrap_or(u16::MAX);
    let total_entries = read_u16(archive_bytes, eocd_offset + 10).unwrap_or(u16::MAX);
    let central_directory_bytes = read_u32(archive_bytes, eocd_offset + 12).unwrap_or(u32::MAX);
    let central_directory_offset = read_u32(archive_bytes, eocd_offset + 16).unwrap_or(u32::MAX);

    if disk_number != 0
        || central_directory_disk != 0
        || entries_on_disk != total_entries
        || total_entries == u16::MAX
        || central_directory_bytes == u32::MAX
        || central_directory_offset == u32::MAX
    {
        return Err(ScienceError::Invalid(
            "multi-disk and ZIP64 skill archives are not admitted".into(),
        ));
    }
    let total_entries = usize::from(total_entries);
    if total_entries == 0 || total_entries > limits.max_entries {
        return Err(ScienceError::Invalid(format!(
            "ZIP entry count must be 1..={}",
            limits.max_entries
        )));
    }
    let central_directory_end = u64::from(central_directory_offset)
        .checked_add(u64::from(central_directory_bytes))
        .ok_or_else(|| ScienceError::Invalid("ZIP central directory size overflow".into()))?;
    if central_directory_end > eocd_offset as u64 {
        return Err(ScienceError::Invalid(
            "ZIP central directory extends beyond its end record".into(),
        ));
    }
    Ok(total_entries)
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn register_logical_path(
    kinds: &mut BTreeMap<String, EntryKind>,
    folded_names: &mut BTreeMap<String, String>,
    folded_kinds: &mut BTreeMap<String, EntryKind>,
    name: &str,
    kind: EntryKind,
) -> Result<()> {
    if let Some(previous_kind) = kinds.get(name) {
        if *previous_kind != kind {
            return Err(ScienceError::Invalid(format!(
                "ZIP file/directory path collision: {name}"
            )));
        }
    } else {
        kinds.insert(name.to_owned(), kind);
    }
    let folded_name = name.to_lowercase();
    if let Some(previous) = folded_names.get(&folded_name) {
        if previous != name {
            return Err(ScienceError::Invalid(format!(
                "case-colliding ZIP entry paths: {previous} and {name}"
            )));
        }
    } else {
        folded_names.insert(folded_name.clone(), name.to_owned());
    }
    if let Some(previous_kind) = folded_kinds.get(&folded_name) {
        if *previous_kind != kind {
            return Err(ScienceError::Invalid(format!(
                "case-folded ZIP file/directory collision: {name}"
            )));
        }
    } else {
        folded_kinds.insert(folded_name, kind);
    }
    Ok(())
}

fn validate_file_directory_conflicts(kinds: &BTreeMap<String, EntryKind>) -> Result<()> {
    for path in kinds.keys() {
        let mut prefix = String::new();
        let components: Vec<&str> = path.split('/').collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if kinds.get(&prefix) == Some(&EntryKind::File) {
                return Err(ScienceError::Invalid(format!(
                    "ZIP file/directory prefix collision: {prefix} and {path}"
                )));
            }
        }
    }
    Ok(())
}

fn discover_skill_roots(
    files: &[InspectedFile],
    limits: SkillArchiveLimits,
) -> Result<Vec<String>> {
    let mut roots = BTreeSet::new();
    for file in files {
        let path = Path::new(&file.archive_path);
        if path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
            continue;
        }
        let root = path
            .parent()
            .and_then(Path::to_str)
            .unwrap_or_default()
            .to_owned();
        roots.insert(root);
    }
    if roots.is_empty() {
        return Err(ScienceError::Invalid(
            "skill bundle does not contain an exact SKILL.md".into(),
        ));
    }
    if roots.len() > limits.max_skills {
        return Err(ScienceError::Invalid(format!(
            "skill bundle exceeds {} roots",
            limits.max_skills
        )));
    }
    Ok(roots.into_iter().collect())
}

fn select_roots(discovered: &[String], requested: &[String]) -> Result<Vec<String>> {
    let available: BTreeSet<&str> = discovered.iter().map(String::as_str).collect();
    let mut selected = requested.to_vec();
    selected.sort();
    for root in &selected {
        if !available.contains(root.as_str()) {
            return Err(ScienceError::Invalid(format!(
                "skill bundle has no SKILL.md root at {root:?}"
            )));
        }
    }
    Ok(selected)
}

fn manifest_selected_files(
    files: &[InspectedFile],
    selected: &[String],
) -> Result<Vec<SkillManifestFile>> {
    let mut manifest = Vec::new();
    for root in selected {
        let prefix = if root.is_empty() {
            String::new()
        } else {
            format!("{root}/")
        };
        let mut root_files = 0usize;
        for file in files {
            if !file.archive_path.starts_with(&prefix) {
                continue;
            }
            let relative = file.archive_path[prefix.len()..].to_owned();
            if relative.is_empty() {
                continue;
            }
            manifest.push(SkillManifestFile {
                archive_path: file.archive_path.clone(),
                relative_path: relative,
                sha256: hex_sha256(&file.bytes),
                bytes: file.bytes.len() as u64,
            });
            root_files += 1;
        }
        if root_files == 0 {
            return Err(ScienceError::Invalid(format!(
                "selected skill root {root:?} contains no files"
            )));
        }
    }
    manifest.sort_by(|left, right| {
        left.archive_path
            .cmp(&right.archive_path)
            .then(left.relative_path.cmp(&right.relative_path))
    });
    Ok(manifest)
}

pub fn begin_quarantine(
    store: &ScienceStore,
    mut context: RunContext,
    admission: &SkillImportAdmission,
) -> Result<ScienceRunTicket> {
    context.environment.insert(
        "skill_archive_sha256".into(),
        admission.archive_sha256().into(),
    );
    context
        .environment
        .insert("skill_admission_sha256".into(), admission.sha256().into());
    context
        .environment
        .insert("skill_operation_id".into(), admission.operation_id().into());
    let ticket = ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: CallId::new("science_skill_quarantine_import"),
    };
    store.create_run(context)?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        serde_json::json!({
            "kind": "skill_quarantine_import",
            "operation_id": admission.operation_id(),
            "archive_sha256": admission.archive_sha256(),
            "admission_sha256": admission.sha256(),
            "selected_subpaths": admission.selected_subpaths(),
            "file_count": admission.file_count(),
        }),
    )?;
    store.request_approval(Approval {
        project_id: ticket.project_id.clone(),
        run_id: ticket.run_id.clone(),
        call_id: ticket.call_id.clone(),
        owner_id: ticket.owner_id.clone(),
        decision: ApprovalDecision::Pending,
        decided_at: None,
    })?;
    store.transition(&ticket.run_id, RunState::AwaitingApproval, None)?;
    Ok(ticket)
}

/// Commit an already-Allowed quarantine import. This function does not decide
/// permission; it verifies the durable Allow and re-inspects the retained
/// archive before the first payload write.
pub fn finish_quarantine(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    admission: SkillImportAdmission,
) -> Result<SkillQuarantineResult> {
    let run = store.load_run(&ticket.run_id)?;
    let approvals = store.approvals(&ticket.run_id)?;
    let [approval] = approvals.as_slice() else {
        return Err(ScienceError::Invalid(
            "skill quarantine finish requires exactly one durable approval".into(),
        ));
    };
    if run.context.project_id != ticket.project_id
        || run.context.owner_id != ticket.owner_id
        || run.state != RunState::Running
        || approval.project_id != ticket.project_id
        || approval.run_id != ticket.run_id
        || approval.owner_id != ticket.owner_id
        || approval.call_id != ticket.call_id
        || approval.decision != ApprovalDecision::Allow
        || approval.decided_at.is_none()
        || run.context.environment.get("skill_archive_sha256")
            != Some(&admission.archive_sha256().to_owned())
        || run.context.environment.get("skill_admission_sha256")
            != Some(&admission.sha256().to_owned())
        || run.context.environment.get("skill_operation_id")
            != Some(&admission.operation_id().to_owned())
    {
        return Err(ScienceError::Invalid(
            "skill quarantine finish requires the exact allowed actor admission".into(),
        ));
    }

    let verified = match inspect_archive(
        &admission.archive_bytes,
        &admission.request,
        admission.limits,
    ) {
        Ok(verified) => verified,
        Err(error) => {
            return fail_and_discard(
                store,
                &ticket,
                format!("skill archive failed post-Allow verification: {error}"),
            );
        }
    };
    if verified.sha256() != admission.sha256() || verified.manifest != admission.manifest {
        return fail_and_discard(
            store,
            &ticket,
            "skill archive changed after admission".into(),
        );
    }
    match commit_quarantine(store, &ticket, &admission) {
        Ok(result) => Ok(result),
        Err(error) => fail_and_discard(store, &ticket, error.to_string()),
    }
}

fn commit_quarantine(
    store: &ScienceStore,
    ticket: &ScienceRunTicket,
    admission: &SkillImportAdmission,
) -> Result<SkillQuarantineResult> {
    let recorded_at = Utc::now();
    let mut recorded_manifest = admission.manifest.clone();
    recorded_manifest.recorded_at = Some(recorded_at);
    let manifest_bytes = serde_json::to_vec_pretty(&recorded_manifest)?;
    let archive_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new(ORIGINAL_ARTIFACT_PATH),
        &admission.archive_bytes,
        "application/zip",
        format!(
            "quarantined skill archive; {} selected root(s), not enabled",
            admission.selected_subpaths().len()
        ),
    )?;
    let manifest_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new(MANIFEST_ARTIFACT_PATH),
        &manifest_bytes,
        "application/json",
        "bounded skill quarantine manifest; materialized=false; enabled=false",
    )?;
    store.add_provenance(Provenance {
        run_id: ticket.run_id.clone(),
        source_uri: format!("upload://{}", admission.operation_id()),
        source_commit: None,
        source_path: None,
        license: "untrusted uploaded skill bundle; license not asserted".into(),
        retrieved_at: recorded_at,
        input_sha256: admission.archive_sha256().into(),
        tool: "Lumen Science bounded-skill-archive-v2 inside SessionActor".into(),
        environment: BTreeMap::from([
            ("authority".into(), "SessionActor".into()),
            ("network".into(), "disabled".into()),
            ("materialized".into(), "false".into()),
            ("enabled".into(), "false".into()),
            ("admission_sha256".into(), admission.sha256().to_owned()),
        ]),
    })?;
    store.add_evidence(Evidence {
        run_id: ticket.run_id.clone(),
        claim: format!(
            "quarantined {} selected skill root(s) without materializing or enabling them",
            admission.selected_subpaths().len()
        ),
        source: format!("upload://{}", admission.operation_id()),
        artifact_sha256: Some(manifest_artifact.sha256.clone()),
        verified_at: recorded_at,
    })?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "skill.quarantine.committed",
        serde_json::json!({
            "archive_artifact_sha256": archive_artifact.sha256,
            "manifest_artifact_sha256": manifest_artifact.sha256,
            "materialized": false,
            "enabled": false,
        }),
    )?;
    let reopened_archive = store.allowed_running_artifact_bytes(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        Path::new(ORIGINAL_ARTIFACT_PATH),
    )?;
    let reopened_manifest = store.allowed_running_artifact_bytes(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        Path::new(MANIFEST_ARTIFACT_PATH),
    )?;
    if reopened_archive != admission.archive_bytes || reopened_manifest != manifest_bytes {
        return Err(ScienceError::Invalid(
            "skill quarantine artifacts changed before successful commit".into(),
        ));
    }
    let running = store.load_run(&ticket.run_id)?;
    let projection = verified_projection(
        store,
        &running,
        admission.operation_id(),
        &reopened_archive,
        &reopened_manifest,
    )?;
    let run = store.transition_succeeded_verified(&ticket.run_id)?;
    Ok(SkillQuarantineResult {
        run,
        operation_id: admission.operation_id().to_owned(),
        artifacts: projection.artifacts,
        evidence: projection.evidence,
        provenance: projection.provenance,
        approvals: projection.approvals,
        replay_after: projection.replay_after,
    })
}

fn fail_and_discard<T>(
    store: &ScienceStore,
    ticket: &ScienceRunTicket,
    reason: String,
) -> Result<T> {
    let cleanup = store.discard_running_outputs(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        &[
            Path::new(ORIGINAL_ARTIFACT_PATH),
            Path::new(MANIFEST_ARTIFACT_PATH),
        ],
    );
    let durable_reason = match cleanup {
        Ok(()) => reason.clone(),
        Err(error) => format!("{reason}; output rollback failed: {error}"),
    };
    let _ = store.append_event(
        &ticket.run_id,
        "SessionActor",
        "skill.quarantine.failed",
        serde_json::json!({ "reason": durable_reason }),
    );
    store.transition(
        &ticket.run_id,
        RunState::Failed,
        Some(durable_reason.clone()),
    )?;
    Err(ScienceError::Invalid(durable_reason))
}

pub fn aggregate(
    store: &ScienceStore,
    run: RunRecord,
    operation_id: String,
) -> Result<SkillQuarantineResult> {
    if run.state != RunState::Succeeded {
        return Err(ScienceError::Invalid(
            "skill quarantine replay requires a succeeded run".into(),
        ));
    }
    let original = store.artifact_bytes_bounded(
        &run.context.project_id,
        &run.context.run_id,
        &run.context.owner_id,
        Path::new(ORIGINAL_ARTIFACT_PATH),
        SkillArchiveLimits::default().max_archive_bytes as u64,
    )?;
    let manifest = store.artifact_bytes_bounded(
        &run.context.project_id,
        &run.context.run_id,
        &run.context.owner_id,
        Path::new(MANIFEST_ARTIFACT_PATH),
        4 * 1024 * 1024,
    )?;
    let projection = verified_projection(store, &run, &operation_id, &original, &manifest)?;
    Ok(SkillQuarantineResult {
        artifacts: projection.artifacts,
        evidence: projection.evidence,
        provenance: projection.provenance,
        approvals: projection.approvals,
        replay_after: projection.replay_after,
        operation_id,
        run,
    })
}

struct VerifiedProjection {
    artifacts: Vec<Artifact>,
    evidence: Vec<Evidence>,
    provenance: Vec<Provenance>,
    approvals: Vec<Approval>,
    replay_after: u64,
}

fn verified_projection(
    store: &ScienceStore,
    run: &RunRecord,
    operation_id: &str,
    original: &[u8],
    manifest_bytes: &[u8],
) -> Result<VerifiedProjection> {
    if !matches!(run.state, RunState::Running | RunState::Succeeded)
        || run.context.environment.get("skill_operation_id") != Some(&operation_id.to_owned())
    {
        return Err(ScienceError::Invalid(
            "skill quarantine projection is not bound to this operation".into(),
        ));
    }
    let artifacts = store.artifacts(&run.context.run_id)?;
    if artifacts.len() != 2 {
        return Err(ScienceError::Invalid(
            "skill quarantine requires exactly two artifacts".into(),
        ));
    }
    let archive_artifact = artifacts
        .iter()
        .find(|item| item.relative_path == Path::new(ORIGINAL_ARTIFACT_PATH))
        .ok_or_else(|| ScienceError::Invalid("skill archive artifact is missing".into()))?;
    let manifest_artifact = artifacts
        .iter()
        .find(|item| item.relative_path == Path::new(MANIFEST_ARTIFACT_PATH))
        .ok_or_else(|| ScienceError::Invalid("skill manifest artifact is missing".into()))?;
    let expected_call = CallId::new("science_skill_quarantine_import");
    if artifacts.iter().any(|item| {
        item.run_id != run.context.run_id
            || item.call_id != expected_call
            || !item.relative_path.starts_with(Path::new("quarantine"))
    }) || archive_artifact.sha256 != hex_sha256(original)
        || archive_artifact.bytes != original.len() as u64
        || manifest_artifact.sha256 != hex_sha256(manifest_bytes)
        || manifest_artifact.bytes != manifest_bytes.len() as u64
    {
        return Err(ScienceError::Invalid(
            "skill quarantine artifact registry failed verification".into(),
        ));
    }
    let manifest: SkillQuarantineManifest = serde_json::from_slice(manifest_bytes)?;
    let Some(recorded_at) = manifest.recorded_at else {
        return Err(ScienceError::Invalid(
            "skill quarantine manifest is missing its recorded timestamp".into(),
        ));
    };
    if manifest.schema_version != 2
        || manifest.operation_id != operation_id
        || manifest.archive_sha256 != hex_sha256(original)
        || manifest.materialized
        || manifest.enabled
        || run.context.environment.get("skill_archive_sha256") != Some(&manifest.archive_sha256)
        || run.context.environment.get("skill_admission_sha256") != Some(&manifest.admission_sha256)
    {
        return Err(ScienceError::Invalid(
            "skill quarantine manifest failed authority verification".into(),
        ));
    }
    let verified = inspect_archive(
        original,
        &SkillQuarantineRequest {
            operation_id: operation_id.to_owned(),
            selected_subpaths: manifest.selected_subpaths.clone(),
        },
        SkillArchiveLimits::default(),
    )?;
    let mut verified_manifest = verified.manifest;
    verified_manifest.recorded_at = Some(recorded_at);
    if verified_manifest != manifest {
        return Err(ScienceError::Invalid(
            "skill quarantine replay manifest does not match archive bytes".into(),
        ));
    }
    let approvals = store.approvals(&run.context.run_id)?;
    let [approval] = approvals.as_slice() else {
        return Err(ScienceError::Invalid(
            "skill quarantine projection requires exactly one approval".into(),
        ));
    };
    if approval.project_id != run.context.project_id
        || approval.run_id != run.context.run_id
        || approval.owner_id != run.context.owner_id
        || approval.call_id != expected_call
        || approval.decision != ApprovalDecision::Allow
        || approval.decided_at.is_none()
    {
        return Err(ScienceError::Invalid(
            "skill quarantine approval chain failed verification".into(),
        ));
    }
    let evidence = store.evidence(&run.context.run_id)?;
    let provenance = store.provenance(&run.context.run_id)?;
    let expected_evidence = Evidence {
        run_id: run.context.run_id.clone(),
        claim: format!(
            "quarantined {} selected skill root(s) without materializing or enabling them",
            manifest.selected_subpaths.len()
        ),
        source: format!("upload://{operation_id}"),
        artifact_sha256: Some(manifest_artifact.sha256.clone()),
        verified_at: recorded_at,
    };
    let expected_provenance = Provenance {
        run_id: run.context.run_id.clone(),
        source_uri: format!("upload://{operation_id}"),
        source_commit: None,
        source_path: None,
        license: "untrusted uploaded skill bundle; license not asserted".into(),
        retrieved_at: recorded_at,
        input_sha256: manifest.archive_sha256.clone(),
        tool: "Lumen Science bounded-skill-archive-v2 inside SessionActor".into(),
        environment: BTreeMap::from([
            ("authority".into(), "SessionActor".into()),
            ("network".into(), "disabled".into()),
            ("materialized".into(), "false".into()),
            ("enabled".into(), "false".into()),
            ("admission_sha256".into(), manifest.admission_sha256.clone()),
        ]),
    };
    if evidence.as_slice() != [expected_evidence] || provenance.as_slice() != [expected_provenance]
    {
        return Err(ScienceError::Invalid(
            "skill quarantine evidence or provenance chain failed verification".into(),
        ));
    }
    let events = store.events_after(&run.context.run_id, 0, 1_000)?;
    Ok(VerifiedProjection {
        artifacts,
        evidence,
        provenance,
        approvals,
        replay_after: events.last().map_or(0, |event| event.seq),
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectId, csv};
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn archive(entries: &[(&str, &[u8], Option<u32>)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (path, bytes, mode) in entries {
            let mut options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            if let Some(mode) = mode {
                options = options.unix_permissions(*mode);
            }
            writer.start_file(*path, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn archive_with_symlink() -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "alpha/SKILL.md",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(b"---\nname: alpha\n---\n").unwrap();
        writer
            .add_symlink(
                "alpha/link",
                "target",
                SimpleFileOptions::default().unix_permissions(0o777),
            )
            .unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn request(selected_subpaths: &[&str]) -> SkillQuarantineRequest {
        SkillQuarantineRequest {
            operation_id: "op-001".into(),
            selected_subpaths: selected_subpaths
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        }
    }

    #[test]
    fn allowed_quarantine_is_store_owned_hashed_and_not_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let bytes = archive(&[
            ("skills/alpha/SKILL.md", b"---\nname: alpha\n---\n", None),
            ("skills/alpha/reference.txt", b"bounded evidence", None),
        ]);
        let admission =
            inspect_archive(&bytes, &request(&["skills/alpha"]), Default::default()).unwrap();
        let context = csv::fixture_context(temp.path(), ProjectId::new("p"), "alice");
        let ticket = begin_quarantine(&store, context, &admission).unwrap();
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        csv::mark_allowed(&store, &ticket).unwrap();
        let result = finish_quarantine(&store, ticket, admission).unwrap();

        assert_eq!(result.run.state, RunState::Succeeded);
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.evidence.len(), 1);
        assert_eq!(result.provenance.len(), 1);
        assert!(
            result
                .artifacts
                .iter()
                .all(|artifact| artifact.relative_path.starts_with("quarantine"))
        );
        let manifest = result
            .artifacts
            .iter()
            .find(|artifact| artifact.relative_path == Path::new(MANIFEST_ARTIFACT_PATH))
            .unwrap();
        let stored = store
            .artifact_bytes(
                &result.run.context.project_id,
                &result.run.context.run_id,
                &result.run.context.owner_id,
                &manifest.relative_path,
            )
            .unwrap();
        let manifest: SkillQuarantineManifest = serde_json::from_slice(&stored).unwrap();
        assert!(!manifest.materialized);
        assert!(!manifest.enabled);
    }

    #[test]
    fn pending_and_non_allow_terminals_never_write_payloads() {
        for decision in [
            ApprovalDecision::Deny,
            ApprovalDecision::Timeout,
            ApprovalDecision::Cancel,
        ] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path());
            let bytes = archive(&[("SKILL.md", b"---\nname: root\n---\n", None)]);
            let admission = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
            let context = csv::fixture_context(temp.path(), ProjectId::new("p"), "alice");
            let ticket = begin_quarantine(&store, context, &admission).unwrap();
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            let run =
                csv::finish_without_execution(&store, &ticket, decision, "test terminal").unwrap();
            assert!(run.state.terminal());
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }

    #[test]
    fn finish_without_exact_allow_and_owner_binding_writes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let bytes = archive(&[("SKILL.md", b"---\nname: root\n---\n", None)]);
        let admission = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
        let context = csv::fixture_context(temp.path(), ProjectId::new("p"), "alice");
        let ticket = begin_quarantine(&store, context, &admission).unwrap();

        assert!(finish_quarantine(&store, ticket.clone(), admission.clone()).is_err());
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        csv::mark_allowed(&store, &ticket).unwrap();
        let forged = ScienceRunTicket {
            owner_id: "mallory".into(),
            ..ticket.clone()
        };
        assert!(finish_quarantine(&store, forged, admission).is_err());
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn operation_run_id_is_stable_and_bound_to_every_authority_identity() {
        let project = ProjectId::new("p");
        let baseline = operation_run_id("alice", &project, "session-1", "op-1");
        assert_eq!(
            baseline,
            operation_run_id("alice", &project, "session-1", "op-1")
        );
        assert_ne!(
            baseline,
            operation_run_id("mallory", &project, "session-1", "op-1")
        );
        assert_ne!(
            baseline,
            operation_run_id("alice", &ProjectId::new("other"), "session-1", "op-1")
        );
        assert_ne!(
            baseline,
            operation_run_id("alice", &project, "session-2", "op-1")
        );
        assert_ne!(
            baseline,
            operation_run_id("alice", &project, "session-1", "op-2")
        );
    }

    #[test]
    fn unsafe_duplicate_case_collision_and_special_entries_fail_closed() {
        for entries in [
            vec![("../SKILL.md", b"x".as_slice(), None)],
            vec![
                ("alpha/SKILL.md", b"x".as_slice(), None),
                ("alpha/skill.md", b"y".as_slice(), None),
            ],
            vec![
                ("Alpha", b"file".as_slice(), None),
                ("alpha/SKILL.md", b"x".as_slice(), None),
            ],
        ] {
            let bytes = archive(&entries);
            assert!(inspect_archive(&bytes, &request(&["alpha"]), Default::default()).is_err());
        }
        assert!(
            inspect_archive(
                &archive_with_symlink(),
                &request(&["alpha"]),
                Default::default()
            )
            .is_err()
        );
    }

    #[test]
    fn exact_duplicate_central_directory_names_fail_closed_before_zip_registry_collapse() {
        let mut bytes = archive(&[
            ("alpha/SKILL.md", b"x".as_slice(), None),
            ("beta_/SKILL.md", b"y".as_slice(), None),
        ]);
        let old_name = b"beta_/SKILL.md";
        let new_name = b"alpha/SKILL.md";
        let mut replacements = 0;
        for offset in 0..=bytes.len() - old_name.len() {
            if bytes[offset..].starts_with(old_name) {
                bytes[offset..offset + old_name.len()].copy_from_slice(new_name);
                replacements += 1;
            }
        }
        assert_eq!(
            replacements, 2,
            "local and central names must both be rewritten"
        );
        assert!(inspect_archive(&bytes, &request(&["alpha"]), Default::default()).is_err());
    }

    #[test]
    fn missing_root_empty_selection_and_resource_caps_fail_closed() {
        let bytes = archive(&[
            ("alpha/SKILL.md", b"x", None),
            ("alpha/large.bin", &[b'x'; 128], None),
        ]);
        assert!(
            inspect_archive(
                &bytes,
                &SkillQuarantineRequest {
                    operation_id: "op".into(),
                    selected_subpaths: Vec::new(),
                },
                Default::default(),
            )
            .is_err()
        );
        assert!(inspect_archive(&bytes, &request(&["missing"]), Default::default()).is_err());
        let limits = SkillArchiveLimits {
            max_file_bytes: 64,
            ..Default::default()
        };
        assert!(inspect_archive(&bytes, &request(&["alpha"]), limits).is_err());
    }

    #[test]
    fn root_empty_subpath_succeeds_and_empty_array_fails() {
        let bytes = archive(&[("SKILL.md", b"---\nname: root\n---\n", None)]);
        let admission = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
        assert_eq!(admission.selected_subpaths(), &[""]);
        assert!(
            inspect_archive(
                &bytes,
                &SkillQuarantineRequest {
                    operation_id: "op-empty-array".into(),
                    selected_subpaths: Vec::new(),
                },
                Default::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn windows_reserved_colon_and_trailing_dot_space_paths_fail_closed() {
        for path in [
            "CON/SKILL.md",
            "alpha/NUL.txt",
            "alpha/COM1/SKILL.md",
            "alpha/bad:name/SKILL.md",
            "alpha/trailing./SKILL.md",
            "alpha/trailing /SKILL.md",
            "C:/SKILL.md",
        ] {
            let bytes = archive(&[(path, b"x".as_slice(), None)]);
            assert!(
                inspect_archive(&bytes, &request(&[""]), Default::default()).is_err(),
                "path should fail closed: {path}"
            );
        }
    }

    #[test]
    fn expansion_ratio_cap_rejects_zip_bombs() {
        let payload = vec![b'A'; 10_000];
        let bytes = archive(&[("alpha/SKILL.md", payload.as_slice(), None)]);
        let limits = SkillArchiveLimits {
            max_expansion_ratio: 1,
            ..Default::default()
        };
        assert!(inspect_archive(&bytes, &request(&["alpha"]), limits).is_err());
    }

    #[test]
    fn implicit_directory_case_collision_fails_closed() {
        // Explicit file "Alpha" collides with implicit parent directory of
        // "alpha/SKILL.md" under case folding.
        let bytes = archive(&[
            ("Alpha", b"file".as_slice(), None),
            ("alpha/SKILL.md", b"x".as_slice(), None),
        ]);
        assert!(inspect_archive(&bytes, &request(&["alpha"]), Default::default()).is_err());
    }

    #[test]
    fn changed_archive_or_selection_after_begin_fails_and_writes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let original = archive(&[("SKILL.md", b"---\nname: root\n---\n", None)]);
        let changed = archive(&[("SKILL.md", b"---\nname: other\n---\n", None)]);
        let nested = archive(&[("nested/SKILL.md", b"---\nname: nested\n---\n", None)]);
        let admission = inspect_archive(&original, &request(&[""]), Default::default()).unwrap();
        let context = csv::fixture_context(temp.path(), ProjectId::new("p"), "alice");
        let ticket = begin_quarantine(&store, context, &admission).unwrap();
        csv::mark_allowed(&store, &ticket).unwrap();

        let different_bytes =
            inspect_archive(&changed, &request(&[""]), Default::default()).unwrap();
        assert!(finish_quarantine(&store, ticket.clone(), different_bytes).is_err());
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());

        // Re-open a fresh run for the selection change path.
        let temp2 = tempfile::tempdir().unwrap();
        let store2 = ScienceStore::new(temp2.path());
        let admission2 =
            inspect_archive(&nested, &request(&["nested"]), Default::default()).unwrap();
        let context2 = csv::fixture_context(temp2.path(), ProjectId::new("p"), "alice");
        let ticket2 = begin_quarantine(&store2, context2, &admission2).unwrap();
        csv::mark_allowed(&store2, &ticket2).unwrap();
        // Same bytes, different root selection is not possible for nested-only
        // archive; force a different admission digest via a second root archive.
        let multi = archive(&[
            ("a/SKILL.md", b"a".as_slice(), None),
            ("b/SKILL.md", b"b".as_slice(), None),
        ]);
        let selected_a = inspect_archive(&multi, &request(&["a"]), Default::default()).unwrap();
        let selected_b = inspect_archive(&multi, &request(&["b"]), Default::default()).unwrap();
        let temp3 = tempfile::tempdir().unwrap();
        let store3 = ScienceStore::new(temp3.path());
        let context3 = csv::fixture_context(temp3.path(), ProjectId::new("p"), "alice");
        let ticket3 = begin_quarantine(&store3, context3, &selected_a).unwrap();
        csv::mark_allowed(&store3, &ticket3).unwrap();
        assert!(finish_quarantine(&store3, ticket3.clone(), selected_b).is_err());
        assert!(store3.artifacts(&ticket3.run_id).unwrap().is_empty());
    }

    #[test]
    fn wrong_ticket_bindings_and_replay_tamper_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let bytes = archive(&[("SKILL.md", b"---\nname: root\n---\n", None)]);
        let admission = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
        let context = csv::fixture_context(temp.path(), ProjectId::new("p"), "alice");
        let ticket = begin_quarantine(&store, context, &admission).unwrap();
        csv::mark_allowed(&store, &ticket).unwrap();

        for forged in [
            ScienceRunTicket {
                project_id: ProjectId::new("other"),
                ..ticket.clone()
            },
            ScienceRunTicket {
                run_id: RunId::new("forged-run"),
                ..ticket.clone()
            },
            ScienceRunTicket {
                call_id: CallId::new("forged-call"),
                ..ticket.clone()
            },
        ] {
            assert!(finish_quarantine(&store, forged, admission.clone()).is_err());
        }
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());

        let result = finish_quarantine(&store, ticket.clone(), admission.clone()).unwrap();
        assert_eq!(result.run.state, RunState::Succeeded);

        // Tamper archive bytes after success — replay must fail.
        let archive_path = store
            .root()
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts")
            .join(ORIGINAL_ARTIFACT_PATH);
        std::fs::write(&archive_path, b"tampered-archive").unwrap();
        assert!(aggregate(&store, result.run.clone(), admission.operation_id().into()).is_err());

        // Restore archive and tamper manifest.
        std::fs::write(&archive_path, &bytes).unwrap();
        let manifest_path = store
            .root()
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts")
            .join(MANIFEST_ARTIFACT_PATH);
        std::fs::write(
            &manifest_path,
            br#"{"schemaVersion":1,"materialized":true}"#,
        )
        .unwrap();
        assert!(aggregate(&store, result.run.clone(), admission.operation_id().into()).is_err());
    }

    #[test]
    fn evidence_provenance_and_approval_registry_tamper_fail_replay() {
        let bytes = archive(&[("SKILL.md", b"---\nname: root\n---\n", None)]);

        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path());
        let admission = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
        let ticket = begin_quarantine(
            &store,
            csv::fixture_context(temp.path(), ProjectId::new("p"), "alice"),
            &admission,
        )
        .unwrap();
        csv::mark_allowed(&store, &ticket).unwrap();
        let result = finish_quarantine(&store, ticket.clone(), admission.clone()).unwrap();
        let evidence_path = store
            .root()
            .join("runs")
            .join(&ticket.run_id.0)
            .join("evidence.json");
        let mut evidence: Vec<Evidence> =
            serde_json::from_slice(&std::fs::read(&evidence_path).unwrap()).unwrap();
        evidence[0].claim.push_str(" tampered");
        std::fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        assert!(aggregate(&store, result.run, admission.operation_id().into()).is_err());

        let temp2 = tempfile::tempdir().unwrap();
        let store2 = ScienceStore::new(temp2.path());
        let admission2 = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
        let ticket2 = begin_quarantine(
            &store2,
            csv::fixture_context(temp2.path(), ProjectId::new("p"), "alice"),
            &admission2,
        )
        .unwrap();
        csv::mark_allowed(&store2, &ticket2).unwrap();
        let result2 = finish_quarantine(&store2, ticket2.clone(), admission2.clone()).unwrap();
        let provenance_path = store2
            .root()
            .join("runs")
            .join(&ticket2.run_id.0)
            .join("provenance.json");
        let mut provenance: Vec<Provenance> =
            serde_json::from_slice(&std::fs::read(&provenance_path).unwrap()).unwrap();
        provenance[0].tool.push_str(" tampered");
        std::fs::write(
            &provenance_path,
            serde_json::to_vec_pretty(&provenance).unwrap(),
        )
        .unwrap();
        assert!(aggregate(&store2, result2.run, admission2.operation_id().into()).is_err());

        let temp3 = tempfile::tempdir().unwrap();
        let store3 = ScienceStore::new(temp3.path());
        let admission3 = inspect_archive(&bytes, &request(&[""]), Default::default()).unwrap();
        let ticket3 = begin_quarantine(
            &store3,
            csv::fixture_context(temp3.path(), ProjectId::new("p"), "alice"),
            &admission3,
        )
        .unwrap();
        csv::mark_allowed(&store3, &ticket3).unwrap();
        let result3 = finish_quarantine(&store3, ticket3.clone(), admission3.clone()).unwrap();
        std::fs::write(
            store3
                .root()
                .join("runs")
                .join(&ticket3.run_id.0)
                .join("approvals.json"),
            b"[]",
        )
        .unwrap();
        assert!(aggregate(&store3, result3.run, admission3.operation_id().into()).is_err());
    }
}
