package oasis

import (
	"fmt"
	"os"
	"path/filepath"
)

// Template is a ready-to-run C2D algorithm starting point — a COMPLETE, working
// pure-stdlib train.py (not a TODO skeleton), plus its params schema and output
// kind. `lumen oasis init <name> --template <key>` picks one. Every template is
// aggregates-only: no raw row ever leaves the data boundary.
type Template struct {
	Key          string
	Description  string
	OutputKind   string
	ParamsSchema string
	TrainPy      string
}

// Templates returns the built-in algorithm templates, default first.
func Templates() []Template {
	return []Template{statsTemplate, histogramTemplate, quantilesTemplate, correlationTemplate, groupbyTemplate, linregTemplate, logregTemplate, dpStatsTemplate}
}

// DefaultTemplate is what a bare `lumen oasis init <name>` scaffolds.
func DefaultTemplate() Template { return statsTemplate }

// TemplateByName resolves a template by key.
func TemplateByName(key string) (Template, bool) {
	for _, t := range Templates() {
		if t.Key == key {
			return t, true
		}
	}
	return Template{}, false
}

// ScaffoldTemplate writes a complete algorithm dir from a template: oasis.toml
// (with the template's output_kind + params_schema), the Dockerfile, and the
// template's working train.py.
func ScaffoldTemplate(dir string, m Manifest, t Template) error {
	if t.OutputKind != "" {
		m.OutputKind = t.OutputKind
	}
	if t.ParamsSchema != "" {
		m.ParamsSchema = t.ParamsSchema
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "oasis.toml"), []byte(formatTOML(m)), 0o644); err != nil {
		return fmt.Errorf("write manifest: %w", err)
	}
	if err := os.WriteFile(filepath.Join(dir, m.Dockerfile), []byte(dockerfileTemplate), 0o644); err != nil {
		return fmt.Errorf("write dockerfile: %w", err)
	}
	if err := os.WriteFile(filepath.Join(dir, "train.py"), []byte(t.TrainPy), 0o644); err != nil {
		return fmt.Errorf("write train.py: %w", err)
	}
	gi := filepath.Join(dir, ".gitignore")
	if err := os.WriteFile(gi, []byte("__pycache__/\n*.pyc\n/oasis-lock.json\n"), 0o644); err != nil {
		return fmt.Errorf("write gitignore: %w", err)
	}
	return nil
}

const dockerfileTemplate = `FROM python:3.11-slim
COPY train.py /app/train.py
WORKDIR /app
USER 65534:65534
ENTRYPOINT ["python", "/app/train.py"]
`

// pyHeader is the shared C2D boilerplate every template starts from: contract
// paths, structured aggregates-only logging, dataset loading, numeric-column
// helpers, and the zip(model.json, metrics.json) -> /out/output.bin writer.
const pyHeader = `#!/usr/bin/env python3
"""C2D algorithm (pure Python stdlib). AGGREGATES ONLY — no raw row leaves the
data boundary. Container contract: read /data (read-only) + optional /params.json,
write /out/output.bin = zip(model.json, metrics.json). Paths overridable via
VO_DATA_DIR / VO_OUT_DIR / VO_PARAMS for local testing.
"""
import csv
import io
import json
import os
import sys
import zipfile

DATA_DIR = os.environ.get("VO_DATA_DIR", "/data")
OUT_DIR = os.environ.get("VO_OUT_DIR", "/out")
PARAMS_FILE = os.environ.get("VO_PARAMS", "/params.json")


def log(stage, **kw):
    print(json.dumps({"stage": stage, **kw}), flush=True)


def die(reason, code=2):
    log("error", reason=reason)
    sys.exit(code)


def load_params():
    if os.path.exists(PARAMS_FILE):
        try:
            with open(PARAMS_FILE) as f:
                return json.load(f) or {}
        except (OSError, ValueError):
            return {}
    return {}


def find_input():
    if not os.path.isdir(DATA_DIR):
        die("no_data_dir")
    names = sorted(os.listdir(DATA_DIR))
    for n in names:
        if n.lower().endswith((".csv", ".tsv")):
            return os.path.join(DATA_DIR, n)
    if names:
        return os.path.join(DATA_DIR, names[0])
    die("no_input_file")


def read_rows(path):
    sep = "\t" if path.lower().endswith(".tsv") else ","
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter=sep))


def _isnum(v):
    try:
        float(v)
        return True
    except (TypeError, ValueError):
        return False


def numeric_columns(rows, candidates=None):
    cols = candidates or (list(rows[0].keys()) if rows else [])
    out = []
    for c in cols:
        ne = [r.get(c, "") for r in rows if r.get(c, "") not in (None, "")]
        if ne and all(_isnum(v) for v in ne):
            out.append(c)
    return out


def col_values(rows, c):
    xs = []
    for r in rows:
        v = r.get(c, "")
        if v in (None, ""):
            continue
        try:
            xs.append(float(v))
        except (TypeError, ValueError):
            pass
    return xs


def write_output(model, metrics):
    os.makedirs(OUT_DIR, exist_ok=True)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("model.json", json.dumps(model))
        z.writestr("metrics.json", json.dumps(metrics))
    with open(os.path.join(OUT_DIR, "output.bin"), "wb") as f:
        f.write(buf.getvalue())
`

var statsTemplate = Template{
	Key:          "stats",
	Description:  "Per-column descriptive statistics (n, mean, min, max, std) — the default.",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `

def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    stats = {}
    for c in numeric_columns(rows, params.get("columns")):
        xs = col_values(rows, c)
        if not xs:
            continue
        n = len(xs)
        mean = sum(xs) / n
        var = sum((x - mean) ** 2 for x in xs) / n
        stats[c] = {"n": n, "mean": mean, "min": min(xs), "max": max(xs), "std": var ** 0.5}
    log("computed", numeric_columns=len(stats))
    write_output(
        {"format": "vo-colstats-1", "n_rows": len(rows), "columns": stats},
        {"status": "ok", "rows": len(rows), "numeric_columns": len(stats)},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var correlationTemplate = Template{
	Key:          "correlation",
	Description:  "Pairwise Pearson correlation matrix between numeric columns.",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `

def _aligned(rows, a, b):
    xa, xb = [], []
    for r in rows:
        va, vb = r.get(a, ""), r.get(b, "")
        if va in (None, "") or vb in (None, ""):
            continue
        try:
            fa, fb = float(va), float(vb)
        except (TypeError, ValueError):
            continue
        xa.append(fa)
        xb.append(fb)
    return xa, xb


def _pearson(xa, xb):
    n = len(xa)
    if n < 2:
        return None
    ma, mb = sum(xa) / n, sum(xb) / n
    cov = sum((xa[i] - ma) * (xb[i] - mb) for i in range(n))
    va = sum((x - ma) ** 2 for x in xa)
    vb = sum((x - mb) ** 2 for x in xb)
    if va == 0 or vb == 0:
        return None
    return cov / ((va * vb) ** 0.5)


def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    cols = numeric_columns(rows, params.get("columns"))
    corr = {}
    for i, a in enumerate(cols):
        for b in cols[i:]:
            xa, xb = _aligned(rows, a, b)
            r = _pearson(xa, xb)
            corr.setdefault(a, {})[b] = r
            corr.setdefault(b, {})[a] = r
    log("computed", columns=len(cols))
    write_output(
        {"format": "vo-correlation-1", "columns": cols, "correlation": corr},
        {"status": "ok", "rows": len(rows), "numeric_columns": len(cols)},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var linregTemplate = Template{
	Key:          "linreg",
	Description:  "Multivariate linear regression (gradient descent) — train a model on data you can't see; outputs coefficients + R^2.",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"target":{"type":"string"},"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `

def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    cols = numeric_columns(rows, params.get("columns"))
    target = params.get("target") or (cols[-1] if cols else None)
    feats = [c for c in cols if c != target]
    if not target or not feats:
        die("need >=2 numeric columns (features + a target)")

    X, y = [], []
    for r in rows:
        try:
            row = [float(r[c]) for c in feats]
            t = float(r[target])
        except (KeyError, TypeError, ValueError):
            continue
        X.append(row)
        y.append(t)
    n = len(y)
    if n < 2:
        die("not_enough_rows")

    cols_t = list(zip(*X))
    means = [sum(col) / n for col in cols_t]
    stds = [((sum((v - means[j]) ** 2 for v in col) / n) ** 0.5) or 1.0 for j, col in enumerate(cols_t)]
    Xs = [[(row[j] - means[j]) / stds[j] for j in range(len(feats))] for row in X]
    ymean = sum(y) / n

    w = [0.0] * len(feats)
    b = ymean
    lr = 0.1
    for _ in range(3000):
        gw = [0.0] * len(feats)
        gb = 0.0
        for i in range(n):
            pred = b + sum(w[j] * Xs[i][j] for j in range(len(feats)))
            err = pred - y[i]
            gb += err
            for j in range(len(feats)):
                gw[j] += err * Xs[i][j]
        b -= lr * gb / n
        for j in range(len(feats)):
            w[j] -= lr * gw[j] / n

    sse = sum((b + sum(w[j] * Xs[i][j] for j in range(len(feats))) - y[i]) ** 2 for i in range(n))
    sst = sum((y[i] - ymean) ** 2 for i in range(n))
    r2 = 1 - sse / sst if sst else 0.0

    coef = {feats[j]: w[j] / stds[j] for j in range(len(feats))}
    intercept = b - sum(w[j] * means[j] / stds[j] for j in range(len(feats)))
    log("trained", features=len(feats), r2=round(r2, 4))
    write_output(
        {"format": "vo-linreg-1", "target": target, "intercept": intercept, "coefficients": coef},
        {"status": "ok", "rows": n, "r2": r2},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var histogramTemplate = Template{
	Key:          "histogram",
	Description:  "Per-column binned histograms (distribution shape) — aggregate counts per bin.",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"bins":{"type":"integer"},"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `

def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    bins = int(params.get("bins", 10)) or 10
    hist = {}
    for c in numeric_columns(rows, params.get("columns")):
        xs = col_values(rows, c)
        if not xs:
            continue
        lo, hi = min(xs), max(xs)
        width = (hi - lo) / bins if hi > lo else 1.0
        counts = [0] * bins
        for x in xs:
            k = int((x - lo) / width) if width else 0
            k = max(0, min(k, bins - 1))
            counts[k] += 1
        hist[c] = {"bins": bins, "edges": [lo + i * width for i in range(bins + 1)], "counts": counts, "n": len(xs)}
    log("computed", numeric_columns=len(hist))
    write_output(
        {"format": "vo-histogram-1", "columns": hist},
        {"status": "ok", "rows": len(rows), "numeric_columns": len(hist)},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var dpStatsTemplate = Template{
	Key:          "dp-stats",
	Description:  "Differentially-private counts + means (Laplace mechanism, epsilon budget). Declare per-column bounds in params for a real guarantee.",
	OutputKind:   "metrics",
	ParamsSchema: `{"type":"object","properties":{"epsilon":{"type":"number"},"bounds":{"type":"object"},"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `
import math
import random


def _laplace(scale):
    # Inverse-CDF sample of Laplace(0, scale).
    u = random.random() - 0.5
    sign = 1.0 if u >= 0 else -1.0
    return -scale * sign * math.log(1.0 - 2.0 * abs(u))


def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    eps = float(params.get("epsilon", 1.0)) or 1.0
    bounds = params.get("bounds", {}) or {}
    cols = numeric_columns(rows, params.get("columns"))

    # Split the privacy budget across the count query + each column mean.
    share = eps / (len(cols) + 1)
    dp_count = len(rows) + _laplace(1.0 / share)  # row count, sensitivity 1

    out = {}
    for c in cols:
        raw = col_values(rows, c)
        if not raw:
            continue
        b = bounds.get(c)
        if b:
            lo, hi = float(b[0]), float(b[1])
        else:
            # Fallback: data-derived range. NOTE: min/max are themselves sensitive;
            # declare bounds in params for a real DP guarantee.
            lo, hi = min(raw), max(raw)
        clamped = [min(max(v, lo), hi) for v in raw]
        n = len(clamped)
        true_mean = sum(clamped) / n
        sensitivity = (hi - lo) / n if n else 0.0
        out[c] = {"dp_mean": true_mean + _laplace(sensitivity / share), "clip_lo": lo, "clip_hi": hi}

    log("computed", epsilon=eps, numeric_columns=len(out))
    write_output(
        {"format": "vo-dpstats-1", "epsilon": eps, "dp_row_count": dp_count, "columns": out},
        {"status": "ok", "epsilon": eps, "numeric_columns": len(out), "mechanism": "laplace"},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var groupbyTemplate = Template{
	Key:          "groupby",
	Description:  "Group-by aggregation with k-anonymity — count + per-column means per group, suppressing groups smaller than min_group_size (default 5).",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"by":{"type":"string"},"min_group_size":{"type":"integer"},"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `

def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    if not rows:
        write_output({"format": "vo-groupby-1", "groups": {}}, {"status": "ok", "rows": 0, "groups": 0})
        return
    k = int(params.get("min_group_size", 5)) or 5

    by = params.get("by")
    numeric = set(numeric_columns(rows))
    allcols = list(rows[0].keys())
    if not by:
        cats = [c for c in allcols if c not in numeric]
        if cats:
            by = cats[0]  # prefer a categorical column
        else:
            by = min(allcols, key=lambda c: len({r.get(c, "") for r in rows}))
    value_cols = [c for c in numeric_columns(rows, params.get("columns")) if c != by]

    buckets = {}
    for r in rows:
        buckets.setdefault(r.get(by, ""), []).append(r)

    out = {}
    suppressed = 0
    suppressed_rows = 0
    for key, grp in buckets.items():
        if len(grp) < k:  # k-anonymity: never emit a group that could re-identify
            suppressed += 1
            suppressed_rows += len(grp)
            continue
        agg = {"count": len(grp)}
        for c in value_cols:
            xs = col_values(grp, c)
            if xs:
                agg[c + "_mean"] = sum(xs) / len(xs)
        out[key] = agg
    log("computed", groups=len(out), suppressed=suppressed, k=k)
    write_output(
        {"format": "vo-groupby-1", "group_by": by, "min_group_size": k, "groups": out},
        {"status": "ok", "rows": len(rows), "groups": len(out), "suppressed_groups": suppressed, "suppressed_rows": suppressed_rows},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var quantilesTemplate = Template{
	Key:          "quantiles",
	Description:  "Per-column quantiles (p25/median/p75/p95) — robust statistics, outlier-resistant.",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"quantiles":{"type":"array","items":{"type":"number"}},"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `

def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    qs = params.get("quantiles") or [0.25, 0.5, 0.75, 0.95]
    out = {}
    for c in numeric_columns(rows, params.get("columns")):
        xs = sorted(col_values(rows, c))
        if not xs:
            continue
        res = {}
        for q in qs:
            if len(xs) == 1:
                res[str(q)] = xs[0]
                continue
            pos = q * (len(xs) - 1)
            lo = int(pos)
            hi = min(lo + 1, len(xs) - 1)
            res[str(q)] = xs[lo] + (pos - lo) * (xs[hi] - xs[lo])
        out[c] = {"n": len(xs), "quantiles": res}
    log("computed", numeric_columns=len(out))
    write_output(
        {"format": "vo-quantiles-1", "columns": out},
        {"status": "ok", "rows": len(rows), "numeric_columns": len(out)},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}

var logregTemplate = Template{
	Key:          "logreg",
	Description:  "Logistic regression (binary classification, gradient descent). Target binarized at its median; outputs coefficients + accuracy.",
	OutputKind:   "model",
	ParamsSchema: `{"type":"object","properties":{"target":{"type":"string"},"columns":{"type":"array","items":{"type":"string"}}}}`,
	TrainPy: pyHeader + `
import math


def _sigmoid(z):
    if z < -60:
        return 0.0
    if z > 60:
        return 1.0
    return 1.0 / (1.0 + math.exp(-z))


def main():
    params = load_params()
    rows = read_rows(find_input())
    log("loaded", rows=len(rows))
    cols = numeric_columns(rows, params.get("columns"))
    target = params.get("target") or (cols[-1] if cols else None)
    feats = [c for c in cols if c != target]
    if not target or not feats:
        die("need >=2 numeric columns (features + a target)")

    X, yraw = [], []
    for r in rows:
        try:
            X.append([float(r[c]) for c in feats])
            yraw.append(float(r[target]))
        except (KeyError, TypeError, ValueError):
            continue
    n = len(yraw)
    if n < 2:
        die("not_enough_rows")

    thr = sorted(yraw)[n // 2]  # binarize target at its median → works on any numeric column
    y = [1.0 if v > thr else 0.0 for v in yraw]

    cols_t = list(zip(*X))
    means = [sum(col) / n for col in cols_t]
    stds = [((sum((v - means[j]) ** 2 for v in col) / n) ** 0.5) or 1.0 for j, col in enumerate(cols_t)]
    Xs = [[(X[i][j] - means[j]) / stds[j] for j in range(len(feats))] for i in range(n)]

    w = [0.0] * len(feats)
    b = 0.0
    lr = 0.1
    for _ in range(3000):
        gw = [0.0] * len(feats)
        gb = 0.0
        for i in range(n):
            p = _sigmoid(b + sum(w[j] * Xs[i][j] for j in range(len(feats))))
            err = p - y[i]
            gb += err
            for j in range(len(feats)):
                gw[j] += err * Xs[i][j]
        b -= lr * gb / n
        for j in range(len(feats)):
            w[j] -= lr * gw[j] / n

    correct = 0
    for i in range(n):
        pred = 1.0 if _sigmoid(b + sum(w[j] * Xs[i][j] for j in range(len(feats)))) >= 0.5 else 0.0
        if pred == y[i]:
            correct += 1
    acc = correct / n
    coef = {feats[j]: w[j] / stds[j] for j in range(len(feats))}
    intercept = b - sum(w[j] * means[j] / stds[j] for j in range(len(feats)))
    log("trained", features=len(feats), accuracy=round(acc, 4))
    write_output(
        {"format": "vo-logreg-1", "target": target, "threshold": thr, "intercept": intercept, "coefficients": coef},
        {"status": "ok", "rows": n, "accuracy": acc},
    )
    log("done")


if __name__ == "__main__":
    main()
`,
}
