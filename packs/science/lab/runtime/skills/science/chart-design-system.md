---
name: chart-design-system
description: Apply Oasis publication-grade design tokens to matplotlib/seaborn charts. Consistent fonts, colors, and layout for journal submission.
runAs: inline
allowedTools: [bash, write_file, read_file]
---

## Oasis Design Tokens for matplotlib

Apply these rcParams to matplotlib for publication-ready charts:

```python
import matplotlib as mpl
import matplotlib.pyplot as plt

mpl.rcParams.update({
    # Fonts: Oasis system — Instrument Serif for titles, system sans-serif for labels
    "font.family": "sans-serif",
    "font.sans-serif": ["Helvetica", "Arial", "DejaVu Sans"],
    "font.size": 10,
    "axes.titlesize": 12,
    "axes.labelsize": 10,
    "xtick.labelsize": 8,
    "ytick.labelsize": 8,
    "legend.fontsize": 8,

    # Color palette: earthy, colorblind-friendly
    "axes.prop_cycle": mpl.cycler(color=[
        "#c28b4b",  # Oasis gold
        "#5b8c7a",  # Forest green
        "#8c6b4b",  # Warm brown
        "#4b7b8c",  # Steel blue
        "#c26b4b",  # Terracotta
        "#6b5b8c",  # Muted purple
    ]),

    # Layout
    "figure.dpi": 300,
    "savefig.dpi": 300,
    "savefig.bbox": "tight",
    "figure.figsize": (6, 4),

    # Grid
    "axes.grid": True,
    "grid.alpha": 0.3,
    "grid.color": "#e0dbd1",

    # Spines
    "axes.spines.top": False,
    "axes.spines.right": False,
})
```

## Usage
1. Copy the rcParams block to `figures/style.py`
2. Import `from figures.style import *` in analysis scripts
3. Re-render figures with `python figures/plot.py`
4. Verify: 300 DPI, Oasis gold primary color, colorblind-safe palette
