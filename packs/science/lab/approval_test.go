package lab

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"lumen/internal/permission"
)

func TestApprovalHubBlocksUntilResolve(t *testing.T) {
	mode := permission.ModeDefault
	h := newApprovalHub(func() permission.Mode { return mode })
	idCh := make(chan string, 1)
	emit := func(kind string, payload map[string]any) {
		if kind != "approval_request" {
			t.Errorf("kind %s", kind)
			return
		}
		id, _ := payload["id"].(string)
		select {
		case idCh <- id:
		default:
		}
	}
	done := make(chan bool, 1)
	go func() {
		ok, _, err := h.decide(context.Background(), "bash", json.RawMessage(`{}`), emit)
		if err != nil {
			t.Errorf("decide: %v", err)
		}
		done <- ok
	}()
	var gotID string
	select {
	case gotID = <-idCh:
	case <-time.After(2 * time.Second):
		t.Fatal("no approval id emitted")
	}
	if !h.resolve(gotID, true, nil) {
		t.Fatal("resolve failed")
	}
	select {
	case ok := <-done:
		if !ok {
			t.Fatal("expected allow")
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timeout waiting decide")
	}
}

func TestApprovalHubPlanDenies(t *testing.T) {
	h := newApprovalHub(func() permission.Mode { return permission.ModePlan })
	ok, _, err := h.decide(context.Background(), "bash", nil, func(string, map[string]any) {})
	if err != nil || ok {
		t.Fatalf("plan should deny, ok=%v err=%v", ok, err)
	}
}

func TestApprovalHubBypassAllows(t *testing.T) {
	h := newApprovalHub(func() permission.Mode { return permission.ModeBypass })
	ok, _, err := h.decide(context.Background(), "bash", nil, func(string, map[string]any) {})
	if err != nil || !ok {
		t.Fatalf("bypass should allow, ok=%v err=%v", ok, err)
	}
}

func TestApprovalHubEditedArgs(t *testing.T) {
	h := newApprovalHub(func() permission.Mode { return permission.ModeDefault })
	idCh := make(chan string, 1)
	emit := func(kind string, payload map[string]any) {
		if kind == "approval_request" {
			idCh <- payload["id"].(string)
		}
	}
	done := make(chan struct{})
	var gotArgs json.RawMessage
	var ok bool
	var err error
	go func() {
		ok, gotArgs, err = h.decide(context.Background(), "bash", json.RawMessage(`{"command":"rm x"}`), emit)
		close(done)
	}()
	id := <-idCh
	edited := json.RawMessage(`{"command":"echo safe"}`)
	if !h.resolve(id, true, edited) {
		t.Fatal("resolve")
	}
	<-done
	if err != nil || !ok {
		t.Fatalf("ok=%v err=%v", ok, err)
	}
	if string(gotArgs) != string(edited) {
		t.Fatalf("args %s", gotArgs)
	}
}
