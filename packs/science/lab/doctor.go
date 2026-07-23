package lab

import (
	"os/exec"

	labruntime "lumen/internal/science/lab/runtime"
	"lumen/internal/science/paths"
)

// DoctorResult holds lab environment health checks.
type DoctorResult struct {
	Python   string `json:"python"`
	PythonOK bool   `json:"python_ok"`
	Error    string `json:"error,omitempty"`
}

// RunDoctor validates the lab runtime environment.
func RunDoctor(sciDir string) DoctorResult {
	r := DoctorResult{}
	dataDir := paths.DataDir(sciDir)
	r.Python = labruntime.ResolvePython(dataDir)
	if _, err := exec.LookPath(r.Python); err == nil {
		r.PythonOK = true
	} else {
		r.Error = "python (" + r.Python + ") not found; conda env may be missing. Run: lumen science start"
	}
	return r
}
