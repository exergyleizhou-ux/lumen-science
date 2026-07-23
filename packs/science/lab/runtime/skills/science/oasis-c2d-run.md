---
name: oasis-c2d-run
description: Submit a trained model or analysis script as a C2D (Compute-to-Data) algorithm on the Oasis marketplace. Handles scaffold, validate, build, and deploy.
runAs: inline
allowedTools: [read_file, write_file, bash, glob]
---

## Purpose
Push a research artifact (model, analysis pipeline) to the Oasis C2D marketplace so others can run it on their data without seeing your data — privacy-preserving compute.

## Workflow
1. Ensure workspace has a `train.py` or equivalent entrypoint
2. Run `lumen oasis init` to scaffold the algorithm manifest (if not present)
3. Run `lumen oasis validate` to check C2D contract compliance
4. Run `lumen oasis build` to package the algorithm
5. Run `lumen oasis deploy` to register it on the marketplace
6. Record the deployment in provenance.jsonl

## Pre-requisites
- Oasis token configured (`lumen science config` or Oasis embed login)
- `train.py` must accept `--data-dir` and write to `output/`
- Algorithm must emit only aggregates, not raw rows (privacy check)
