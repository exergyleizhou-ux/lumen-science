{
  "schema_version": 1,
  "generated_at": "2026-07-25",
  "milestone": "LS5-0",
  "purpose": "Lumen Science 1.0 baseline freeze for 5.0 development",

  "product": {
    "name": "Lumen Science",
    "version": "0.1.250",
    "head_sha": "d603efed558ffd7c715328be94c2f7b315ebb301",
    "head_short": "d603efe",
    "branch": "main",
    "repositories": {
      "lumen": "https://github.com/exergyleizhou-ux/lumen",
      "lumen-science": "https://github.com/exergyleizhou-ux/lumen-science"
    }
  },

  "connectors": {
    "total": 42,
    "implemented": 42,
    "rejected": 1,
    "license_pending": 1,
    "live_verified": 6,
    "offline_verified": 42,
    "integrations": [
      "pubmed", "chembl", "crossref", "uniprot", "europepmc", "openalex",
      "semantic-scholar", "arxiv", "biorxiv", "rcsb-pdb", "pdbe", "alphafold",
      "interpro", "sifts", "pubchem", "bindingdb", "gtopdb", "surechembl",
      "chebi", "ensembl", "ncbi-gene", "dbsnp", "clinvar", "gnomad", "ucsc",
      "mygene", "myvariant", "reactome", "string-db", "intact", "wikipathways",
      "opentargets", "geo", "arrayexpress", "gtex", "hpa", "expression-atlas",
      "single-cell-atlas", "depmap", "eutils", "biogrid", "kegg"
    ],
    "rejected_connectors": {
      "biogrid": "rejected-credential-in-url",
      "kegg": "license-pending"
    }
  },

  "skills": {
    "total": 27,
    "categories": {
      "protein-structure": ["alphafold2", "esmfold2", "openfold3"],
      "protein-design": ["proteinmpnn", "ligandmpnn", "solublempnn"],
      "protein-representation": ["fair-esm2"],
      "molecular-docking": ["diffdock"],
      "molecular-dynamics": ["boltz"],
      "molecular-biology": ["motif-for-claude-science"],
      "genomics": ["borzoi", "evo2"],
      "single-cell": ["scgpt", "scvi-tools"],
      "protein-ligand": ["chai1"],
      "chemistry": ["molecule-viewer"],
      "visualization": ["chart-design-system", "figure-publication"],
      "quality": ["integrity-auditor", "traceability-review"],
      "research": ["literature-survey", "research-brief", "literature-review", "indication-dossier"],
      "compute": ["oasis-c2d-run"],
      "infrastructure": ["env-management", "remote-compute-ssh"]
    }
  },

  "offline_product_loop": {
    "status": "complete",
    "components": [
      "connector fetch (42/42)",
      "raw artifact registration (SHA-256)",
      "Python notebook kernel",
      "derived artifact",
      "reviewer verification",
      "renderer/Motif viewer",
      "reopen/replay",
      "evidence graph"
    ]
  },

  "test_results": {
    "rust_offline": "181 passed, 0 failed",
    "rust_live_ignored": "8 (arxiv, chembl, crossref, europepmc, openalex, pubmed, semantic-scholar, uniprot)",
    "go_compute": "11 passed, 0 failed",
    "lumen_guard": "22 passed, 0 failed",
    "xai_system_power": "4 passed, 0 failed (Windows dark wake included)",
    "total_offline": "218 passed, 0 failed",
    "total_live_ignored": "8"
  },

  "security": {
    "lumen_guard_bash": "22 tests, all pass",
    "lumen_guard_writepath": "all path checks pass",
    "windows_coverage": "full (drive letters, cmd commands, backslash paths)",
    "unsafe_mode_bypass": "supported via LUMEN_UNSAFE=1",
    "git_commit_exemption": "active"
  },

  "cross_platform": {
    "windows": {
      "status": "verified",
      "build": "MSVC release, 124.8MB",
      "dark_wake": "detected via GetSystemMetrics(SM_CMONITORS)",
      "shell_detection": "platform-aware (bash→cmd→powershell)",
      "scripts": "10 PowerShell scripts + 1 Winget manifest"
    },
    "macos": {
      "status": "verified",
      "dark_wake": "detected via IOPMConnectionGetSystemCapabilities",
      "shell": "bash/sh"
    },
    "linux": {
      "status": "verified",
      "shell": "bash/sh"
    }
  },

  "known_gaps": [
    "8 live network tests not yet executed (require network + API keys)",
    "No formal release pipeline (SBOM, signing, CI matrix)",
    "Python kernel integration not end-to-end tested on all platforms",
    "Skills registry has 27 entries but not all have been runtime-verified"
  ],

  "upstream_sources": {
    "aipoch_open_science": {
      "repository": "https://github.com/aipoch/open-science",
      "observed_commit": "ff70c93a2b7913b799870895ac2ecb081362736b",
      "license": "Apache-2.0",
      "derived_assets": ["42 connector semantics", "18 AI skills"]
    },
    "jvogan_motif": {
      "repository": "https://github.com/jvogan/motif",
      "observed_commit": "876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0",
      "license": "MIT",
      "derived_assets": ["Motif molecular viewer", "Motif-for-Claude-Science skill"]
    }
  },

  "next_milestone": "LS5-1 golden corpus sampling",
  "acceptance": "ACCEPT — V1 baseline confirmed, ready for 5.0 development"
}
