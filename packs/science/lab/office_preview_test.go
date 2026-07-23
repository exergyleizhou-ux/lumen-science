package lab

import (
	"archive/zip"
	"bytes"
	"testing"
)

func TestExtractDocxText(t *testing.T) {
	// Minimal fake docx: zip with word/document.xml containing w:t text
	var buf bytes.Buffer
	zw := zip.NewWriter(&buf)
	w, err := zw.Create("word/document.xml")
	if err != nil {
		t.Fatal(err)
	}
	xml := `<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>Hello Office Preview</w:t></w:r></w:p>
<w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p></w:body></w:document>`
	if _, err := w.Write([]byte(xml)); err != nil {
		t.Fatal(err)
	}
	if err := zw.Close(); err != nil {
		t.Fatal(err)
	}
	text, kind, err := ExtractOfficeText("report.docx", buf.Bytes(), 5000)
	if err != nil {
		t.Fatal(err)
	}
	if kind != "docx" {
		t.Fatalf("kind %s", kind)
	}
	if !bytes.Contains([]byte(text), []byte("Hello Office Preview")) {
		t.Fatalf("text %q", text)
	}
	if !bytes.Contains([]byte(text), []byte("Second paragraph")) {
		t.Fatalf("missing second: %q", text)
	}
}

func TestExtractOfficeRejectsUnknown(t *testing.T) {
	_, _, err := ExtractOfficeText("a.bin", []byte{1, 2, 3}, 100)
	if err == nil {
		t.Fatal("expected error")
	}
}
