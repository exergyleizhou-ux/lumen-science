---
name: figure-publication
description: Apply publication-grade styling to research figures — fonts, color palettes, resolution, and layout suitable for journal submission.
runAs: inline
allowedTools: [bash, write_file, read_file, glob]
---

## Purpose
Ensure all research figures meet publication standards: consistent styling, proper resolution, accessible color palettes, and journal-specific formatting.

## Guidelines
- **Fonts**: Use Oasis design system (Instrument Serif for titles, Geist for labels) or journal-specified fonts
- **Colors**: Use colorblind-friendly palettes (Viridis, Okabe-Ito). Avoid red-green only.
- **Resolution**: 300 DPI minimum for raster, vector (SVG/PDF) preferred
- **Layout**: Multi-panel figures use consistent spacing, aligned axes
- **Typography**: 8pt minimum font size, sans-serif for labels

## Workflow
1. Scan `figures/` and `reports/` for image files
2. Check resolution, color space, font usage
3. If matplotlib scripts found, update rcParams
4. Re-render with publication settings
5. Generate `figures/figure-checklist.md` with pass/fail per figure

## Supported Tools
- Python matplotlib/seaborn
- R ggplot2
- ImageMagick (raster conversion)
