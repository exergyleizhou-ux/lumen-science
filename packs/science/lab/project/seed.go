package project

import (
	labruntime "lumen/internal/science/lab/runtime"
)

// ApplySeedTemplate copies a research-pack seed example into the project workspace.
func ApplySeedTemplate(sciDir, template, workspace string) error {
	return labruntime.CopySeedExample(sciDir, template, workspace)
}
