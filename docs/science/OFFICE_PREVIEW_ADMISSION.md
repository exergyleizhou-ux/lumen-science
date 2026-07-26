# Office Preview Converter Admission

**Date:** 2026-07-26  
**Status:** Fail-closed — **not admitted** for product open until hostile-doc suite passes.

## Rule

UI isolation design (OOPIF / supervisor) may be absorbed from Open Science.  
**Converter trust boundary is not copied by default.**

Each format must record:

| Field | Required |
|-------|----------|
| exact dependency + version | yes |
| license (no GPL/AGPL default) | yes |
| input size cap | yes |
| timeout | yes |
| no-network policy | yes |
| hostile document tests | yes |
| converter provenance | yes |
| output artifact hash path | yes |

Until `hostileDocTestsPass=true` and `admitted=true` in the desktop admission table,  
`office:preview-open` / product gate **denies** open.

## Current table (code)

Source: `packs/science-desktop/src/main/files/office-preview-admission.ts`

| Format | Converter | Hostile tests | Admitted |
|--------|-----------|---------------|----------|
| docx | office-docx-isolated | false | false |
| xlsx | office-xlsx-isolated | false | false |
| pptx | office-pptx-isolated | false | false |
| pdf | pdfjs-legacy | false | false |

## Hostile corpus (required before flip)

- Malformed ZIP/OOXML containers  
- Macro / external relationship abuse  
- Zip bombs / deep nesting  
- Oversized sheets / slides  
- Network URL in relationships (must not fetch)

## Medical

Not certified. Not for clinical document interpretation.
