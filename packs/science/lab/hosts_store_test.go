package lab

import (
	"testing"
)

func TestRegisteredHostsCRUD(t *testing.T) {
	sci := t.TempDir()
	list, err := UpsertRegisteredHost(sci, RegisteredHost{Alias: "gpu1", Hostname: "10.0.0.1", User: "lab", Notes: "A100"})
	if err != nil {
		t.Fatal(err)
	}
	if len(list) != 1 || list[0].Hostname != "10.0.0.1" {
		t.Fatalf("%+v", list)
	}
	list, err = UpsertRegisteredHost(sci, RegisteredHost{Alias: "gpu1", Hostname: "10.0.0.2", User: "lab"})
	if err != nil || list[0].Hostname != "10.0.0.2" {
		t.Fatalf("upsert %v %+v", err, list)
	}
	list, err = LoadRegisteredHosts(sci)
	if err != nil || len(list) != 1 {
		t.Fatal(err)
	}
	// local alias ignored on save
	_, err = UpsertRegisteredHost(sci, RegisteredHost{Alias: "local", Hostname: "x"})
	if err != nil {
		t.Fatal(err)
	}
	list, _ = LoadRegisteredHosts(sci)
	for _, h := range list {
		if h.Alias == "local" {
			t.Fatal("local must not be stored")
		}
	}
	list, err = DeleteRegisteredHost(sci, "gpu1")
	if err != nil || len(list) != 0 {
		t.Fatalf("del %v %+v", err, list)
	}
}
