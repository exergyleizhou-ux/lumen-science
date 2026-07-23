package proxy

import "regexp"

var dateSuffixRE = regexp.MustCompile(`-\d{8}$`)

// ResolveModel maps a Science-side model id to the upstream model name.
func ResolveModel(spec ProviderSpec, name string) string {
	if spec.ForceModelOverride && spec.ForceModel != "" {
		return spec.ForceModel
	}
	if name == "" {
		return spec.DefaultModel
	}
	if v, ok := spec.ModelMap[name]; ok {
		return v
	}
	ids := make(map[string]struct{}, len(spec.Models))
	for _, m := range spec.Models {
		ids[m.ID] = struct{}{}
	}
	if _, ok := ids[name]; ok {
		return name
	}
	stripped := dateSuffixRE.ReplaceAllString(name, "")
	if v, ok := spec.ModelMap[stripped]; ok {
		return v
	}
	for k, v := range spec.ModelMap {
		if stringsHasPrefix(name, k) || stringsHasPrefix(stripped, k) {
			return v
		}
	}
	return spec.DefaultModel
}

func stringsHasPrefix(s, prefix string) bool {
	return len(s) >= len(prefix) && s[:len(prefix)] == prefix
}

// ClampMaxTokens caps max_tokens per upstream model limits.
func ClampMaxTokens(spec ProviderSpec, v int, model string) int {
	if v <= 0 {
		return v
	}
	cap := spec.DefaultCap
	if spec.ModelCaps != nil {
		if c, ok := spec.ModelCaps[model]; ok {
			cap = c
		}
	}
	if cap > 0 && v > cap {
		return cap
	}
	return v
}
