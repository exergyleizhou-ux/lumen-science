package lab

import (
	"archive/zip"
	"bytes"
	"fmt"
	"io"
	"path/filepath"
	"regexp"
	"strings"
)

var xmlTagRe = regexp.MustCompile(`<[^>]+>`)

// ExtractOfficeText returns a plain-text preview for Office Open XML files
// (.docx / .pptx / .xlsx). Not a full WYSIWYG preview — productivity-oriented extract.
func ExtractOfficeText(path string, data []byte, maxRunes int) (text string, kind string, err error) {
	if maxRunes <= 0 {
		maxRunes = 20000
	}
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".docx":
		kind = "docx"
		text, err = extractOOXML(data, []string{"word/document.xml"}, maxRunes)
	case ".pptx":
		kind = "pptx"
		text, err = extractOOXML(data, nil, maxRunes) // all ppt/slides/slide*.xml
	case ".xlsx":
		kind = "xlsx"
		text, err = extractXLSX(data, maxRunes)
	default:
		return "", "", fmt.Errorf("not an office file: %s", ext)
	}
	return text, kind, err
}

func extractOOXML(data []byte, preferred []string, maxRunes int) (string, error) {
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return "", err
	}
	var parts []string
	want := map[string]bool{}
	for _, p := range preferred {
		want[p] = true
	}
	for _, f := range zr.File {
		name := f.Name
		use := false
		if len(want) > 0 {
			use = want[name]
		} else if strings.HasPrefix(name, "ppt/slides/slide") && strings.HasSuffix(name, ".xml") {
			use = true
		}
		if !use {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			continue
		}
		body, _ := io.ReadAll(io.LimitReader(rc, 2<<20))
		_ = rc.Close()
		// Word stores text in <w:t>; PPT in <a:t>
		parts = append(parts, stripXMLText(string(body)))
	}
	if len(parts) == 0 {
		// fallback: any xml with t tags
		for _, f := range zr.File {
			if !strings.HasSuffix(f.Name, ".xml") {
				continue
			}
			rc, err := f.Open()
			if err != nil {
				continue
			}
			body, _ := io.ReadAll(io.LimitReader(rc, 1<<20))
			_ = rc.Close()
			t := stripXMLText(string(body))
			if strings.TrimSpace(t) != "" {
				parts = append(parts, t)
			}
			if len(parts) > 5 {
				break
			}
		}
	}
	out := strings.Join(parts, "\n\n")
	return trimRunes(out, maxRunes), nil
}

func extractXLSX(data []byte, maxRunes int) (string, error) {
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return "", err
	}
	// shared strings
	var shared []string
	for _, f := range zr.File {
		if f.Name != "xl/sharedStrings.xml" {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			break
		}
		body, _ := io.ReadAll(io.LimitReader(rc, 4<<20))
		_ = rc.Close()
		// crude: pull all tag-stripped segments
		shared = strings.Split(stripXMLText(string(body)), "\n")
		break
	}
	var rows []string
	for _, f := range zr.File {
		if !strings.HasPrefix(f.Name, "xl/worksheets/sheet") || !strings.HasSuffix(f.Name, ".xml") {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			continue
		}
		body, _ := io.ReadAll(io.LimitReader(rc, 2<<20))
		_ = rc.Close()
		// extract <v> values (shared string indexes or numbers)
		vRe := regexp.MustCompile(`<v>([^<]*)</v>`)
		tRe := regexp.MustCompile(`t="s"`)
		// simpler: just dump shared strings + any numeric v
		_ = tRe
		matches := vRe.FindAllStringSubmatch(string(body), 200)
		var cells []string
		for _, m := range matches {
			if len(m) < 2 {
				continue
			}
			val := m[1]
			// try as shared string index
			var idx int
			if _, err := fmt.Sscanf(val, "%d", &idx); err == nil && idx >= 0 && idx < len(shared) {
				cells = append(cells, strings.TrimSpace(shared[idx]))
			} else {
				cells = append(cells, val)
			}
		}
		if len(cells) > 0 {
			rows = append(rows, "— "+f.Name+" —")
			rows = append(rows, strings.Join(cells, " | "))
		}
	}
	if len(rows) == 0 && len(shared) > 0 {
		rows = shared
	}
	return trimRunes(strings.Join(rows, "\n"), maxRunes), nil
}

func stripXMLText(xml string) string {
	// Replace common OOXML text element closings with newlines for readability
	s := strings.ReplaceAll(xml, "</w:p>", "\n")
	s = strings.ReplaceAll(s, "</a:p>", "\n")
	s = strings.ReplaceAll(s, "</w:tr>", "\n")
	s = xmlTagRe.ReplaceAllString(s, "")
	s = strings.ReplaceAll(s, "&lt;", "<")
	s = strings.ReplaceAll(s, "&gt;", ">")
	s = strings.ReplaceAll(s, "&amp;", "&")
	s = strings.ReplaceAll(s, "&quot;", "\"")
	// collapse blank lines
	var b strings.Builder
	for _, line := range strings.Split(s, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		b.WriteString(line)
		b.WriteByte('\n')
	}
	return strings.TrimSpace(b.String())
}

func trimRunes(s string, max int) string {
	r := []rune(s)
	if len(r) <= max {
		return s
	}
	return string(r[:max]) + "\n…[truncated]…"
}
