package native

import "testing"

func TestShippedFleetHasFiveMembers(t *testing.T) {
	shipped := ShippedFleet()
	if len(shipped) != 5 {
		t.Fatalf("shipped count = %d, want 5", len(shipped))
	}
	want := map[string]bool{"pubmed": true, "chembl": true, "oasis": true, "c2d": true, "geo": true}
	for _, m := range shipped {
		if !want[m.ID] {
			t.Fatalf("unexpected shipped id %q", m.ID)
		}
		if m.Status != "shipped" {
			t.Fatalf("%s status = %q", m.ID, m.Status)
		}
	}
}

func TestShippedFleetIDs(t *testing.T) {
	ids := ShippedFleetIDs()
	if len(ids) != 5 {
		t.Fatalf("ids len = %d", len(ids))
	}
	if ids[0] != "pubmed" || ids[4] != "geo" {
		t.Fatalf("ids order = %v", ids)
	}
}

func TestNativeFleetVersion(t *testing.T) {
	if NativeFleetVersion == "" {
		t.Fatal("NativeFleetVersion unset")
	}
}
