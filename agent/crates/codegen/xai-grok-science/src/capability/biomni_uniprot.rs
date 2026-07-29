//! Biomni `query_uniprot` → Lumen `connector_fetch` (connector_id = uniprot).
//!
//! First executable ecosystem capability. Does not run Biomni Python, does not
//! accept caller-supplied endpoints/URLs/headers/shell, and does not write the
//! Science store. Mapping only; SessionActor remains the sole execution path.

use crate::ScienceError;
use serde::{Deserialize, Serialize};

/// Stable capability id used by product UI and ACP.
pub const BIOMNI_QUERY_UNIPROT_CAPABILITY_ID: &str = "ecosystem/biomni/query_uniprot";

/// Max UTF-8 bytes for a prompt after trim.
pub const MAX_PROMPT_BYTES: usize = 2_000;

/// Source provenance for this adapted capability (from ecosystem catalog + lock).
pub const BIOMNI_QUERY_UNIPROT_PROVENANCE: BiomniUniprotProvenance = BiomniUniprotProvenance {
    repository: "https://github.com/snap-stanford/Biomni.git",
    exact_commit: "400c1f366b96a35ca253e13c9b06c5076af41d65",
    source_path: "biomni/tool/tool_description/database.py",
    source_sha256: "875473dc5473cf4f7615c2b4fd886f543ca8a295f7c58eca00fdceb22d2883b6",
    license: "Apache-2.0",
    reuse_mode: "adapted-capability-mapping",
    lumen_executor: "x.ai/science/connector_fetch",
    lumen_connector_id: "uniprot",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiomniUniprotProvenance {
    pub repository: &'static str,
    pub exact_commit: &'static str,
    pub source_path: &'static str,
    pub source_sha256: &'static str,
    pub license: &'static str,
    pub reuse_mode: &'static str,
    pub lumen_executor: &'static str,
    pub lumen_connector_id: &'static str,
}

/// Allowed product input. Renderer/catalog fields outside this set are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BiomniUniprotInput {
    pub prompt: String,
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_max_results() -> u32 {
    5
}

/// Result of mapping onto the existing connector_fetch contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiomniUniprotMappedFetch {
    pub capability_id: &'static str,
    pub connector_id: &'static str,
    /// connector_fetch uses `query`; prompt is mapped here only.
    pub query: String,
    pub max_results: u32,
    pub controlled_tools: &'static [&'static str],
    pub provenance: BiomniUniprotProvenance,
}

/// Forbidden keys that must never influence execution (Biomni endpoint escape).
const FORBIDDEN_KEYS: &[&str] = &[
    "endpoint",
    "url",
    "baseUrl",
    "base_url",
    "method",
    "headers",
    "body",
    "command",
    "shell",
    "python",
    "filesystemPath",
    "filesystem_path",
    "provider",
    "model",
    "ownerId",
    "owner_id",
    "projectId",
    "project_id",
    "sessionId",
    "session_id",
    "runId",
    "run_id",
    "callId",
    "call_id",
    "connectorId",
    "connector_id",
];

/// Map a capability invocation JSON object to a fixed UniProt connector_fetch.
///
/// Accepts either camelCase (`maxResults`) or the catalog's `max_results` via
/// a pre-normalization step by the caller; this function deserializes only the
/// strict product contract after forbidden keys are stripped/checked.
pub fn map_biomni_query_uniprot(
    capability_id: &str,
    raw: &serde_json::Value,
) -> Result<BiomniUniprotMappedFetch, ScienceError> {
    if capability_id != BIOMNI_QUERY_UNIPROT_CAPABILITY_ID {
        return Err(ScienceError::Invalid(format!(
            "capability '{capability_id}' is not admitted for execution"
        )));
    }
    let obj = raw
        .as_object()
        .ok_or_else(|| ScienceError::Invalid("capability input must be a JSON object".into()))?;
    for key in obj.keys() {
        if FORBIDDEN_KEYS.iter().any(|f| f.eq_ignore_ascii_case(key)) {
            return Err(ScienceError::Invalid(format!(
                "capability input field '{key}' is forbidden; identity and network targets are fixed by Lumen"
            )));
        }
    }
    // Allow only prompt + maxResults (+ legacy max_results alias rewritten below).
    let mut normalized = serde_json::Map::new();
    for (key, value) in obj {
        match key.as_str() {
            "prompt" => {
                normalized.insert("prompt".into(), value.clone());
            }
            "maxResults" | "max_results" => {
                normalized.insert("maxResults".into(), value.clone());
            }
            other => {
                return Err(ScienceError::Invalid(format!(
                    "unknown capability field '{other}'"
                )));
            }
        }
    }
    let input: BiomniUniprotInput =
        serde_json::from_value(serde_json::Value::Object(normalized))
            .map_err(|e| ScienceError::Invalid(format!("invalid capability input: {e}")))?;

    let prompt = input.prompt.trim();
    if prompt.is_empty() {
        return Err(ScienceError::Invalid(
            "prompt must be non-empty after trim".into(),
        ));
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(ScienceError::Invalid(format!(
            "prompt exceeds {MAX_PROMPT_BYTES} byte limit"
        )));
    }
    if !(1..=50).contains(&input.max_results) {
        return Err(ScienceError::Invalid(
            "maxResults must be an integer in 1..=50".into(),
        ));
    }

    Ok(BiomniUniprotMappedFetch {
        capability_id: BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
        connector_id: "uniprot",
        query: prompt.to_owned(),
        max_results: input.max_results,
        controlled_tools: &["x.ai/science/connector_fetch"],
        provenance: BIOMNI_QUERY_UNIPROT_PROVENANCE,
    })
}

pub fn reject_unknown_capability(capability_id: &str) -> ScienceError {
    ScienceError::Invalid(format!(
        "capability '{capability_id}' is not admitted (Biomni catalog remains quarantined except {BIOMNI_QUERY_UNIPROT_CAPABILITY_ID})"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_legal_prompt_and_max_results_to_uniprot() {
        let mapped = map_biomni_query_uniprot(
            BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
            &json!({ "prompt": "  human insulin  ", "maxResults": 5 }),
        )
        .expect("map");
        assert_eq!(mapped.connector_id, "uniprot");
        assert_eq!(mapped.query, "human insulin");
        assert_eq!(mapped.max_results, 5);
        assert_eq!(mapped.controlled_tools, &["x.ai/science/connector_fetch"]);
        assert_eq!(
            mapped.provenance.exact_commit,
            "400c1f366b96a35ca253e13c9b06c5076af41d65"
        );
        assert_eq!(mapped.provenance.license, "Apache-2.0");
    }

    #[test]
    fn accepts_catalog_max_results_alias() {
        let mapped = map_biomni_query_uniprot(
            BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
            &json!({ "prompt": "insulin", "max_results": 3 }),
        )
        .expect("map");
        assert_eq!(mapped.max_results, 3);
    }

    #[test]
    fn rejects_max_results_bounds() {
        for bad in [json!(0), json!(51), json!(-1), json!(1.5), json!("5")] {
            let err = map_biomni_query_uniprot(
                BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
                &json!({ "prompt": "insulin", "maxResults": bad }),
            )
            .expect_err("must reject");
            assert!(
                err.to_string().contains("maxResults") || err.to_string().contains("invalid"),
                "{err}"
            );
        }
    }

    #[test]
    fn rejects_empty_and_oversized_prompt() {
        assert!(
            map_biomni_query_uniprot(
                BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
                &json!({ "prompt": "   " })
            )
            .is_err()
        );
        let long = "x".repeat(MAX_PROMPT_BYTES + 1);
        assert!(
            map_biomni_query_uniprot(
                BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
                &json!({ "prompt": long })
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_forbidden_network_and_identity_fields() {
        for key in [
            "endpoint",
            "url",
            "method",
            "headers",
            "ownerId",
            "projectId",
            "sessionId",
            "connectorId",
        ] {
            let err = map_biomni_query_uniprot(
                BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
                &json!({ "prompt": "insulin", key: "evil" }),
            )
            .expect_err("forbidden");
            assert!(
                err.to_string().contains(key) || err.to_string().contains("forbidden"),
                "{err}"
            );
        }
    }

    #[test]
    fn rejects_unknown_capability_and_other_biomni_tools() {
        assert!(
            map_biomni_query_uniprot(
                "ecosystem/biomni/analyze_enzyme_kinetics_assay",
                &json!({ "prompt": "x" })
            )
            .is_err()
        );
        assert!(map_biomni_query_uniprot("unknown/capability", &json!({ "prompt": "x" })).is_err());
    }

    #[test]
    fn defaults_max_results_when_omitted() {
        let mapped = map_biomni_query_uniprot(
            BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
            &json!({ "prompt": "insulin" }),
        )
        .expect("map");
        assert_eq!(mapped.max_results, 5);
    }

    #[test]
    fn rejects_shell_and_filesystem_escape_fields() {
        for key in [
            "command",
            "shell",
            "python",
            "filesystemPath",
            "body",
            "headers",
        ] {
            assert!(
                map_biomni_query_uniprot(
                    BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
                    &json!({ "prompt": "insulin", key: "x" }),
                )
                .is_err(),
                "key {key}"
            );
        }
    }

    #[test]
    fn provenance_is_fixed_lumen_connector_not_biomni_runtime() {
        let mapped = map_biomni_query_uniprot(
            BIOMNI_QUERY_UNIPROT_CAPABILITY_ID,
            &json!({ "prompt": "insulin" }),
        )
        .expect("map");
        assert_eq!(mapped.connector_id, "uniprot");
        assert_eq!(
            mapped.provenance.lumen_executor,
            "x.ai/science/connector_fetch"
        );
        assert_eq!(mapped.provenance.reuse_mode, "adapted-capability-mapping");
        assert!(!mapped.provenance.repository.contains("lumen-science"));
        assert_eq!(mapped.controlled_tools, &["x.ai/science/connector_fetch"]);
    }
}
