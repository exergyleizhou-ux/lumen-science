package native

import "testing"

func TestAllLiveChecksPass(t *testing.T) {
	if AllLiveChecksPass(nil) {
		t.Fatal("empty results should not pass")
	}
	if !AllLiveChecksPass([]LiveResult{{FleetID: "pubmed", Tool: "search_articles", Pass: true}}) {
		t.Fatal("single pass should pass")
	}
	if AllLiveChecksPass([]LiveResult{
		{FleetID: "pubmed", Pass: true},
		{FleetID: "geo", Pass: false, Error: "timeout"},
	}) {
		t.Fatal("mixed results should fail")
	}
}
