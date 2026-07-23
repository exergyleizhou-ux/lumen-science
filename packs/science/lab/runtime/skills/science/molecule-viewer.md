---
name: molecule-viewer
description: Visualize PDB/SDF/mol2 structures using 3Dmol.js. Open protein and small molecule structures from bio-tools MCP results in the right panel.
runAs: inline
allowedTools: [science_domain_call, write_file, read_file]
---

## 3Dmol.js Structure Viewer

When a bio-tools MCP query returns PDB IDs or SMILES strings, visualize them:

### Workflow
1. Retrieve structure from ChEMBL/protein-annotation domain: `science_domain_call`
2. For PDB: download from RCSB `https://files.rcsb.org/download/{PDB_ID}.pdb`
3. Save to `figures/{pdb_id}.pdb`
4. Open in Lab's 分子 tab (3Dmol.js viewer)

### 3Dmol Quick Reference
```javascript
// Loaded automatically in Lab's 分子 tab
let viewer = $3Dmol.createViewer("viewer", {backgroundColor: "#fbf9f6"});
viewer.addModel(pdbData, "pdb");
viewer.setStyle({}, {cartoon: {color: "#c28b4b"}});
viewer.zoomTo();
viewer.render();
```

### Supported Formats
- PDB (.pdb) — protein structures
- SDF (.sdf) — small molecules
- MOL2 (.mol2) — SYBYL format
- CIF (.cif) — mmCIF

The right panel "分子" tab embeds ESMFold/AlphaFold-ready viewer.
