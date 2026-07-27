# Provenance — Biomni data, software, protocol, and knowledge catalog

| Field | Value |
|---|---|
| Repository | https://github.com/snap-stanford/Biomni.git |
| Commit | `400c1f366b96a35ca253e13c9b06c5076af41d65` |
| Catalog code license | Apache-2.0 |
| Data references | 76 |
| Software references | 113 |
| Protocol references | 82 |
| Vendored know-how documents | 2, each declaring CC BY 4.0 |
| Lumen catalog | `packs/science/skills/ecosystem/biomni-resource-catalog.json` |
| Runtime authority | none |
| Admission | 273 quarantined, 0 approved |

## Localized catalog

Lumen parses Biomni's `env_desc.py` and `env_desc_cm.py` only as literal
assignments. It records:

- whether a data/software entry appears in Biomni's commercial-mode subset;
- the dataset license/access status reported by Biomni's `license_info.md`;
- unknown or missing software versions, repositories, and licenses rather than
  inventing dependency certainty;
- exact protocol source paths and hashes without copying the protocol bodies;
- source and vendored hashes for the two explicitly CC-BY-4.0 know-how
  documents.

The generated catalog remains non-executable. Each entry is searchable in the
desktop Skills view alongside SCP skills and Biomni tools.

## Deliberately excluded bytes

This vendor tree does not contain:

- any of the 76 data-lake datasets;
- any downloaded package, CLI, model, or environment;
- the 82 Addgene/Thermo Fisher protocol bodies, whose publisher licenses are
  not established by Biomni's metadata;
- `addgene_grna_sequences.csv` or CRISPick download-link material;
- pickle schemas, mutable downloads, or executable upstream runtime code.

Descriptions and names are discovery metadata, not proof that a resource is
scientifically valid, current, commercially usable, installed, or runnable.

## Admission requirements

Data requires an exact version/digest, authoritative source, applicable
license/access permission, citation, parser fixture, and scientific review.
Software requires an exact package identity/version/source/license plus a
confined adapter. Protocols require publisher permission, version/citation,
domain safety review, and typed device/procedure gates. Know-how remains cited
reference material until its claims and examples are mapped to controlled Lumen
tools.
