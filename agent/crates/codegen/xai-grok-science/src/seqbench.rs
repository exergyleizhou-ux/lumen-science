//! Offline Motif-class sequence analysis. Zero network, zero shell.
//! Deterministic — same bytes always produce the same report.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const TOOL: &str = "lumen-seqbench";
pub const TOOL_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub sequence: String,
    pub kind: String, // dna | rna | protein | unknown
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Orf {
    pub frame: i32,
    pub start: usize,
    pub end: usize,
    pub length_aa: usize,
    pub protein: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RestrictionHit {
    pub enzyme: String,
    pub site: String,
    pub position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordSummary {
    pub id: String,
    pub kind: String,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gc_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_complement: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translation_frames: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orfs: Vec<Orf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restriction_hits: Vec<RestrictionHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub schema_version: u32,
    pub tool: String,
    pub tool_version: String,
    pub source_sha256: String,
    pub records: Vec<RecordSummary>,
    pub notes: Vec<String>,
}

/// Parse multi-FASTA or raw sequence. Fail-closed on empty.
pub fn parse_fasta(raw: &str) -> Result<Vec<Record>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty sequence input".into());
    }
    if !raw.starts_with('>') {
        let seq = normalize_seq(raw);
        if seq.is_empty() {
            return Err("no sequence characters found".into());
        }
        let kind = detect_kind(&seq);
        return Ok(vec![Record {
            id: "unnamed".into(),
            description: String::new(),
            sequence: seq,
            kind,
        }]);
    }
    let mut out = Vec::new();
    let mut cur_id = String::new();
    let mut cur_desc = String::new();
    let mut buf = String::new();
    let mut have = false;
    let flush = |id: &str, desc: &str, buf: &mut String, out: &mut Vec<Record>| {
        let seq = normalize_seq(buf);
        buf.clear();
        if seq.is_empty() {
            return;
        }
        let kind = detect_kind(&seq);
        out.push(Record {
            id: if id.is_empty() {
                "unnamed".into()
            } else {
                id.to_string()
            },
            description: desc.to_string(),
            sequence: seq,
            kind,
        });
    };
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(header) = line.strip_prefix('>') {
            if have {
                flush(&cur_id, &cur_desc, &mut buf, &mut out);
            }
            let header = header.trim();
            if let Some((id, desc)) = header.split_once(' ') {
                cur_id = id.to_string();
                cur_desc = desc.trim().to_string();
            } else {
                cur_id = header.to_string();
                cur_desc.clear();
            }
            have = true;
            continue;
        }
        if have {
            buf.push_str(line);
        }
    }
    if have {
        flush(&cur_id, &cur_desc, &mut buf, &mut out);
    }
    if out.is_empty() {
        return Err("no FASTA records parsed".into());
    }
    Ok(out)
}

pub fn analyze(records: &[Record], source_bytes: &[u8]) -> Analysis {
    let source_sha256 = hex_sha256(source_bytes);
    let mut summaries = Vec::with_capacity(records.len());
    for r in records {
        summaries.push(summarize(r));
    }
    Analysis {
        schema_version: 1,
        tool: TOOL.into(),
        tool_version: TOOL_VERSION.into(),
        source_sha256,
        records: summaries,
        notes: vec![
            "Deterministic offline analysis. Not a substitute for wet-lab validation.".into(),
            "Restriction sites are recognition-pattern hits only.".into(),
            "ORFs use standard genetic code; min length 30 aa; ATG start.".into(),
        ],
    }
}

pub fn markdown_report(a: &Analysis, source_label: &str) -> String {
    let mut b = String::new();
    b.push_str("# Sequence analysis report\n\n");
    b.push_str(&format!("- tool: `{}` {}\n", a.tool, a.tool_version));
    b.push_str(&format!("- source: `{source_label}`\n"));
    b.push_str(&format!("- source_sha256: `{}`\n", a.source_sha256));
    b.push_str(&format!("- records: {}\n\n", a.records.len()));
    for r in &a.records {
        b.push_str(&format!("## {}\n\n", r.id));
        b.push_str("| field | value |\n|---|---|\n");
        b.push_str(&format!("| kind | {} |\n", r.kind));
        b.push_str(&format!("| length | {} |\n", r.length));
        if let Some(gc) = r.gc_percent {
            b.push_str(&format!("| GC% | {gc:.2} |\n"));
        }
        b.push('\n');
        if !r.orfs.is_empty() {
            b.push_str("### ORFs (min 30 aa)\n\n");
            b.push_str("| frame | start | end | aa | truncated |\n|---|---:|---:|---:|---|\n");
            for o in &r.orfs {
                b.push_str(&format!(
                    "| {:+} | {} | {} | {} | {} |\n",
                    o.frame, o.start, o.end, o.length_aa, o.truncated
                ));
            }
            b.push('\n');
        }
        if !r.restriction_hits.is_empty() {
            b.push_str("### Restriction sites\n\n");
            b.push_str("| enzyme | site | position |\n|---|---|---:|\n");
            for h in &r.restriction_hits {
                b.push_str(&format!(
                    "| {} | `{}` | {} |\n",
                    h.enzyme, h.site, h.position
                ));
            }
            b.push('\n');
        }
    }
    b.push_str(
        "## Provenance\n\nGenerated offline by Lumen Science seqbench. Not medical advice.\n",
    );
    b
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn summarize(r: &Record) -> RecordSummary {
    let mut s = RecordSummary {
        id: r.id.clone(),
        kind: r.kind.clone(),
        length: r.sequence.len(),
        gc_percent: None,
        reverse_complement: None,
        translation_frames: BTreeMap::new(),
        orfs: Vec::new(),
        restriction_hits: Vec::new(),
    };
    if r.kind == "dna" || r.kind == "rna" {
        s.gc_percent = Some(gc_percent(&r.sequence));
        let rna = r.kind == "rna";
        s.reverse_complement = Some(reverse_complement(&r.sequence, rna));
        for frame in 1..=3 {
            let pep = translate(&r.sequence, frame);
            let short = if pep.len() > 80 {
                format!("{}…", &pep[..80])
            } else {
                pep
            };
            s.translation_frames.insert(format!("+{frame}"), short);
            let rc = reverse_complement(&r.sequence, rna);
            let pep_n = translate(&rc, frame);
            let short_n = if pep_n.len() > 80 {
                format!("{}…", &pep_n[..80])
            } else {
                pep_n
            };
            s.translation_frames.insert(format!("-{frame}"), short_n);
        }
        s.orfs = find_orfs(&r.sequence, rna, 30);
        if r.kind == "dna" {
            s.restriction_hits = find_restriction_sites(&r.sequence);
        }
    }
    s
}

fn normalize_seq(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '.')
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| c.is_ascii_alphabetic() || *c == '*')
        .collect()
}

fn detect_kind(seq: &str) -> String {
    let mut a = 0usize;
    let mut c = 0usize;
    let mut g = 0usize;
    let mut t = 0usize;
    let mut u = 0usize;
    let mut aa = 0usize;
    for ch in seq.chars() {
        match ch {
            'A' => a += 1,
            'C' => c += 1,
            'G' => g += 1,
            'T' => t += 1,
            'U' => u += 1,
            'E' | 'F' | 'I' | 'L' | 'P' | 'Q' | '*' => aa += 1,
            _ => {}
        }
    }
    let n = seq.len().max(1);
    let nt = a + c + g + t + u;
    if (nt as f64) / (n as f64) >= 0.85 {
        if u > t {
            return "rna".into();
        }
        return "dna".into();
    }
    if aa > 0 {
        return "protein".into();
    }
    "unknown".into()
}

fn gc_percent(seq: &str) -> f64 {
    let mut gc = 0usize;
    let mut n = 0usize;
    for ch in seq.chars() {
        match ch {
            'G' | 'C' | 'S' => {
                gc += 1;
                n += 1;
            }
            'A' | 'T' | 'U' | 'W' => n += 1,
            _ => {}
        }
    }
    if n == 0 {
        0.0
    } else {
        100.0 * (gc as f64) / (n as f64)
    }
}

fn reverse_complement(seq: &str, rna: bool) -> String {
    let map = |c: char| -> char {
        match c {
            'A' => {
                if rna {
                    'U'
                } else {
                    'T'
                }
            }
            'T' | 'U' => 'A',
            'G' => 'C',
            'C' => 'G',
            'R' => 'Y',
            'Y' => 'R',
            'S' => 'S',
            'W' => 'W',
            'K' => 'M',
            'M' => 'K',
            'B' => 'V',
            'D' => 'H',
            'H' => 'D',
            'V' => 'B',
            'N' => 'N',
            other => other,
        }
    };
    seq.chars().rev().map(map).collect()
}

fn to_dna(seq: &str) -> String {
    seq.replace('U', "T")
}

fn translate(seq: &str, frame: usize) -> String {
    if !(1..=3).contains(&frame) {
        return String::new();
    }
    let dna = to_dna(seq);
    let bytes = dna.as_bytes();
    let mut out = String::new();
    let mut i = frame - 1;
    while i + 3 <= bytes.len() {
        let codon = std::str::from_utf8(&bytes[i..i + 3]).unwrap_or("NNN");
        out.push(genetic_code(codon));
        i += 3;
    }
    out
}

fn genetic_code(codon: &str) -> char {
    match codon {
        "TTT" | "TTC" => 'F',
        "TTA" | "TTG" | "CTT" | "CTC" | "CTA" | "CTG" => 'L',
        "TCT" | "TCC" | "TCA" | "TCG" | "AGT" | "AGC" => 'S',
        "TAT" | "TAC" => 'Y',
        "TAA" | "TAG" | "TGA" => '*',
        "TGT" | "TGC" => 'C',
        "TGG" => 'W',
        "CCT" | "CCC" | "CCA" | "CCG" => 'P',
        "CAT" | "CAC" => 'H',
        "CAA" | "CAG" => 'Q',
        "CGT" | "CGC" | "CGA" | "CGG" | "AGA" | "AGG" => 'R',
        "ATT" | "ATC" | "ATA" => 'I',
        "ATG" => 'M',
        "ACT" | "ACC" | "ACA" | "ACG" => 'T',
        "AAT" | "AAC" => 'N',
        "AAA" | "AAG" => 'K',
        "GTT" | "GTC" | "GTA" | "GTG" => 'V',
        "GCT" | "GCC" | "GCA" | "GCG" => 'A',
        "GAT" | "GAC" => 'D',
        "GAA" | "GAG" => 'E',
        "GGT" | "GGC" | "GGA" | "GGG" => 'G',
        _ => 'X',
    }
}

fn find_orfs(seq: &str, rna: bool, min_aa: usize) -> Vec<Orf> {
    let mut out = Vec::new();
    for frame in 1..=3 {
        out.extend(orfs_on_strand(seq, frame, min_aa));
    }
    let rc = reverse_complement(seq, rna);
    for frame in 1..=3 {
        for mut o in orfs_on_strand(&rc, frame, min_aa) {
            o.frame = -frame;
            out.push(o);
        }
    }
    out.truncate(50);
    out
}

fn orfs_on_strand(seq: &str, frame: i32, min_aa: usize) -> Vec<Orf> {
    let dna = to_dna(seq);
    let bytes = dna.as_bytes();
    let mut out = Vec::new();
    let mut i = (frame.unsigned_abs() as usize) - 1;
    while i + 2 < bytes.len() {
        if &bytes[i..i + 3] != b"ATG" {
            i += 3;
            continue;
        }
        let start = i;
        let mut pep = String::new();
        let mut trunc = true;
        let mut j = i;
        while j + 2 < bytes.len() {
            let codon = std::str::from_utf8(&bytes[j..j + 3]).unwrap_or("NNN");
            let aa = genetic_code(codon);
            if aa == '*' {
                trunc = false;
                j += 3;
                break;
            }
            pep.push(aa);
            j += 3;
        }
        if pep.len() >= min_aa {
            out.push(Orf {
                frame,
                start,
                end: j,
                length_aa: pep.len(),
                protein: pep,
                truncated: trunc,
            });
        }
        i += 3;
    }
    out
}

const ENZYMES: &[(&str, &str)] = &[
    ("EcoRI", "GAATTC"),
    ("BamHI", "GGATCC"),
    ("HindIII", "AAGCTT"),
    ("XhoI", "CTCGAG"),
    ("NotI", "GCGGCCGC"),
    ("NdeI", "CATATG"),
    ("NcoI", "CCATGG"),
    ("SacI", "GAGCTC"),
    ("KpnI", "GGTACC"),
    ("PstI", "CTGCAG"),
    ("SalI", "GTCGAC"),
    ("XbaI", "TCTAGA"),
    ("SpeI", "ACTAGT"),
    ("BglII", "AGATCT"),
    ("ClaI", "ATCGAT"),
];

fn find_restriction_sites(seq: &str) -> Vec<RestrictionHit> {
    let dna = to_dna(seq);
    let mut hits = Vec::new();
    for (name, site) in ENZYMES {
        let mut start = 0;
        while let Some(pos) = dna[start..].find(site) {
            let abs = start + pos;
            hits.push(RestrictionHit {
                enzyme: (*name).into(),
                site: (*site).into(),
                position: abs,
            });
            start = abs + 1;
            if hits.len() >= 100 {
                return hits;
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multi_and_analyze() {
        let raw = ">a BRCA2\nGAATTCATGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAAAAAGCGGCCGCTAA\n";
        let recs = parse_fasta(raw).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].kind, "dna");
        let a = analyze(&recs, raw.as_bytes());
        assert!(!a.source_sha256.is_empty());
        let r = &a.records[0];
        assert!(r.restriction_hits.iter().any(|h| h.enzyme == "EcoRI"));
        assert!(r.restriction_hits.iter().any(|h| h.enzyme == "NotI"));
        let md = markdown_report(&a, "x.fa");
        assert!(md.contains("source_sha256"));
    }

    #[test]
    fn empty_fails() {
        assert!(parse_fasta("   ").is_err());
    }

    #[test]
    fn deterministic_hash() {
        let raw = ">x\nATGC\n";
        let r = parse_fasta(raw).unwrap();
        let a1 = analyze(&r, raw.as_bytes());
        let a2 = analyze(&r, raw.as_bytes());
        assert_eq!(a1.source_sha256, a2.source_sha256);
        assert_eq!(markdown_report(&a1, "a"), markdown_report(&a2, "a"));
    }
}

// ── SessionActor-gated run protocol ──────────────────────────────────────────
//
// `seq_analyze` used to parse, analyse and `std::fs::write` two files straight
// from the ACP request task: no permission prompt, no durable run record, no
// ownership check, and artifacts that existed on disk with no store entry
// claiming them. Every other science mutation goes through the actor's
// begin/decide/finish protocol; this one did not, so the sentence "the
// SessionActor is the sole execution authority" was not true.
//
// The two halves below close that. Analysis itself is pure and offline, so —
// unlike the csv and import paths — there is no external tool to drive: the
// actor decides, then computes and commits inside `finish_analysis`.

use crate::csv::ScienceRunTicket;
use crate::{
    Approval, ApprovalDecision, Artifact, CallId, Evidence, Provenance, RunContext, RunRecord,
    RunState, ScienceError, ScienceStore,
};
use chrono::Utc;
use std::path::Path;

/// Phase one: create the durable run and its pending approval BEFORE the
/// permission manager is awaited, so every allow/deny/timeout/cancel has a
/// record to finish rather than vanishing.
pub fn begin_analysis(
    store: &ScienceStore,
    context: RunContext,
) -> crate::Result<ScienceRunTicket> {
    let ticket = ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: CallId::new("science_seq_analyze"),
    };
    store.create_run(context)?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        serde_json::json!({ "kind": "seq_analyze" }),
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

/// What an allowed analysis produced, all of it store-committed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeqAnalyzeResult {
    pub run: RunRecord,
    pub analysis: Analysis,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub approvals: Vec<Approval>,
    pub records: usize,
    pub replay_after: u64,
}

/// Phase two, on an allowed run: compute, commit both outputs as ARTIFACTS
/// (hashed and owned by the store, not loose files), record provenance, and
/// land the run in a terminal state.
pub fn finish_analysis(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    source_path: &Path,
    source_bytes: &[u8],
) -> crate::Result<SeqAnalyzeResult> {
    // The same guard the csv path uses: only an allowed, running run may
    // commit output. Without it a caller could finish a run it never got
    // permission for.
    let run = store.load_run(&ticket.run_id)?;
    if run.state != RunState::Running
        || store
            .approvals(&ticket.run_id)?
            .iter()
            .find(|approval| approval.call_id == ticket.call_id)
            .is_none_or(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "seq analysis output requires an allowed running run".into(),
        ));
    }

    let text = String::from_utf8_lossy(source_bytes);
    let records = match parse_fasta(&text) {
        Ok(records) => records,
        Err(error) => {
            // A malformed input is a FAILED run, not a silent error: the run
            // record must say what happened to the permission that was granted.
            let _ = store.transition(&ticket.run_id, RunState::Failed, Some(error.clone()));
            return Err(ScienceError::Invalid(error));
        }
    };
    let analysis = analyze(&records, source_bytes);
    let report = markdown_report(
        &analysis,
        source_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("input.fa"),
    );
    let analysis_json = serde_json::to_vec_pretty(&analysis)?;

    let analysis_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new("analysis.json"),
        &analysis_json,
        "application/json",
        "table",
    )?;
    let report_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id,
        Path::new("report.md"),
        report.as_bytes(),
        "text/markdown",
        "document",
    )?;

    let tool_identity = format!("{TOOL} {TOOL_VERSION} inside SessionActor");
    store.add_provenance(Provenance {
        run_id: ticket.run_id.clone(),
        source_uri: format!("file://{}", source_path.display()),
        source_commit: None,
        source_path: Some(source_path.display().to_string()),
        license: "caller-supplied input".into(),
        retrieved_at: Utc::now(),
        input_sha256: hex_sha256(source_bytes),
        tool: tool_identity.clone(),
        environment: BTreeMap::from([
            ("algorithm".into(), "seqbench-v1".into()),
            ("authority".into(), "SessionActor".into()),
            ("network".into(), "disabled".into()),
        ]),
    })?;
    store.add_evidence(Evidence {
        run_id: ticket.run_id.clone(),
        claim: format!(
            "analyzed {} sequence record(s) with {TOOL} {TOOL_VERSION}",
            analysis.records.len()
        ),
        source: source_path.display().to_string(),
        artifact_sha256: Some(analysis_artifact.sha256.clone()),
        verified_at: Utc::now(),
    })?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "analysis.completed",
        serde_json::json!({
            "tool": tool_identity,
            "records": analysis.records.len(),
            "artifacts": [
                analysis_artifact.sha256,
                report_artifact.sha256,
            ],
        }),
    )?;
    let run = store.transition(&ticket.run_id, RunState::Succeeded, None)?;
    store.append_event(
        &ticket.run_id,
        "HostVerification",
        "run.succeeded",
        serde_json::json!({}),
    )?;
    let events = store.events_after(&ticket.run_id, 0, 1_000)?;

    Ok(SeqAnalyzeResult {
        records: analysis.records.len(),
        artifacts: store.artifacts(&ticket.run_id)?,
        evidence: store.evidence(&ticket.run_id)?,
        provenance: store.provenance(&ticket.run_id)?,
        approvals: store.approvals(&ticket.run_id)?,
        replay_after: events.last().map_or(0, |event| event.seq),
        run,
        analysis,
    })
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use crate::{ProjectId, RunId};

    const FASTA: &[u8] = b">seq1\nACGTACGT\n>seq2\nGAATTC\n";

    fn context(root: &Path, project: &str, owner: &str) -> RunContext {
        RunContext {
            run_id: RunId::new_v7(),
            project_id: ProjectId::new(project),
            session_id: "session-seq".into(),
            owner_id: owner.into(),
            workspace_root: root.to_path_buf(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-seqbench-v1".into(),
            artifact_root: root.join("science-store"),
            environment: BTreeMap::from([("network".into(), "disabled".into())]),
        }
    }

    #[test]
    fn begin_is_durable_and_finish_requires_allow() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let ticket = begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();

        let run = store.load_run(&ticket.run_id).unwrap();
        assert_eq!(run.state, RunState::AwaitingApproval);
        assert_eq!(
            store.approvals(&ticket.run_id).unwrap()[0].decision,
            ApprovalDecision::Pending
        );
        assert!(
            finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).is_err(),
            "finish without Allow must fail"
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn non_allow_terminals_never_create_artifacts() {
        for (decision, state) in [
            (ApprovalDecision::Deny, RunState::Denied),
            (ApprovalDecision::Timeout, RunState::TimedOut),
            (ApprovalDecision::Cancel, RunState::Cancelled),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let ticket =
                begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
            crate::csv::finish_without_execution(
                &store,
                &ticket,
                decision,
                "focused protocol test",
            )
            .unwrap();

            assert_eq!(store.load_run(&ticket.run_id).unwrap().state, state);
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }

    #[test]
    fn allow_commits_only_store_owned_hashed_artifacts_and_audit_chain() {
        let temp = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("science-store");
        let store = ScienceStore::new(&store_root);
        let ticket = begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();
        let result = finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).unwrap();

        assert_eq!(result.run.state, RunState::Succeeded);
        assert_eq!(result.records, 2);
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.evidence.len(), 1);
        assert_eq!(result.provenance.len(), 1);
        assert_eq!(result.approvals[0].decision, ApprovalDecision::Allow);
        assert_eq!(
            result.evidence[0].artifact_sha256.as_deref(),
            Some(result.artifacts[0].sha256.as_str())
        );
        for artifact in &result.artifacts {
            let bytes = store
                .artifact_bytes(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    &artifact.relative_path,
                )
                .unwrap();
            assert_eq!(hex_sha256(&bytes), artifact.sha256);
        }
        assert!(
            !store_root.join("project-a").join("seqbench").exists(),
            "the legacy loose artifact path must not be written"
        );
    }

    #[test]
    fn owner_project_and_call_boundaries_fail_closed() {
        for mutate in ["owner", "project", "call"] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let mut ticket =
                begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
            crate::csv::mark_allowed(&store, &ticket).unwrap();
            match mutate {
                "owner" => ticket.owner_id = "mallory".into(),
                "project" => ticket.project_id = ProjectId::new("project-b"),
                "call" => ticket.call_id = CallId::new("forged-call"),
                _ => unreachable!(),
            }

            assert!(
                finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).is_err(),
                "{mutate} boundary was bypassed"
            );
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }
}
