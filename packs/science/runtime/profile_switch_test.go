package runtime

import (
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"lumen/internal/config"
	sciconfig "lumen/internal/science/config"
)

func mockUpstream(t *testing.T, status int) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		w.WriteHeader(status)
		if status == http.StatusOK {
			w.Write([]byte(`{"content":[]}`))
		}
	}))
}

func freePort(t *testing.T) int {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := ln.Addr().(*net.TCPAddr).Port
	ln.Close()
	return port
}

func writeSwitchFixture(t *testing.T, dir string, profiles []sciconfig.Profile, activeID string, proxyPort, sandboxPort int) {
	t.Helper()
	cfg := sciconfig.Default()
	cfg.SchemaVersion = sciconfig.CurrentSchemaVersion
	cfg.Profiles = profiles
	cfg.ActiveProfileID = activeID
	cfg.ProxyPort = proxyPort
	cfg.SandboxPort = sandboxPort
	if err := sciconfig.Save(dir, cfg); err != nil {
		t.Fatal(err)
	}
}

func TestSwitchProfileRejects401(t *testing.T) {
	okSrv := mockUpstream(t, http.StatusOK)
	defer okSrv.Close()
	badSrv := mockUpstream(t, http.StatusUnauthorized)
	defer badSrv.Close()

	dir := t.TempDir()
	proxyPort := freePort(t)
	p1 := sciconfig.Profile{ID: "p1", Name: "Active", TemplateID: "custom", APIKey: "sk-active", BaseURL: okSrv.URL}
	p2 := sciconfig.Profile{ID: "p2", Name: "Bad", TemplateID: "custom", APIKey: "sk-bad", BaseURL: badSrv.URL}
	writeSwitchFixture(t, dir, []sciconfig.Profile{p1, p2}, "p1", proxyPort, proxyPort+1)

	mgr, err := New(dir, &config.File{})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { mgr.StopProxy() })

	_, err = mgr.SwitchProfile("p2")
	if err == nil {
		t.Fatal("expected switch rejection")
	}
	if !strings.Contains(err.Error(), "401") && !strings.Contains(err.Error(), "无效") {
		t.Fatalf("unexpected error: %v", err)
	}
	cfg, _ := sciconfig.Load(dir)
	if cfg.ActiveProfileID != "p1" {
		t.Fatalf("active should remain p1, got %q", cfg.ActiveProfileID)
	}
}

func TestSwitchProfileActivatesOn200(t *testing.T) {
	okSrv := mockUpstream(t, http.StatusOK)
	defer okSrv.Close()

	dir := t.TempDir()
	proxyPort := freePort(t)
	p1 := sciconfig.Profile{ID: "p1", Name: "Active", TemplateID: "custom", APIKey: "sk-active", BaseURL: okSrv.URL}
	p2 := sciconfig.Profile{ID: "p2", Name: "Next", TemplateID: "custom", APIKey: "sk-next", BaseURL: okSrv.URL}
	writeSwitchFixture(t, dir, []sciconfig.Profile{p1, p2}, "p1", proxyPort, proxyPort+1)

	mgr, err := New(dir, &config.File{})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { mgr.StopProxy() })

	res, err := mgr.SwitchProfile("p2")
	if err != nil {
		t.Fatal(err)
	}
	if res.Action != "activated" || res.ProfileID != "p2" {
		t.Fatalf("result: %+v", res)
	}
	cfg, _ := sciconfig.Load(dir)
	if cfg.ActiveProfileID != "p2" {
		t.Fatalf("active=%q want p2", cfg.ActiveProfileID)
	}
	if cfg.Provider != "relay" {
		t.Fatalf("provider=%q", cfg.Provider)
	}
}

func TestSwitchProfileUnchangedWhenAlreadyActive(t *testing.T) {
	okSrv := mockUpstream(t, http.StatusOK)
	defer okSrv.Close()

	dir := t.TempDir()
	proxyPort := freePort(t)
	p1 := sciconfig.Profile{ID: "p1", Name: "Active", TemplateID: "custom", APIKey: "sk-active", BaseURL: okSrv.URL}
	writeSwitchFixture(t, dir, []sciconfig.Profile{p1}, "p1", proxyPort, proxyPort+1)

	mgr, err := New(dir, &config.File{})
	if err != nil {
		t.Fatal(err)
	}

	res, err := mgr.SwitchProfile("p1")
	if err != nil {
		t.Fatal(err)
	}
	if res.Action != "unchanged" {
		t.Fatalf("%+v", res)
	}
}

func TestSwitchProfileRollsBackOnProxyStartFailure(t *testing.T) {
	okSrv := mockUpstream(t, http.StatusOK)
	defer okSrv.Close()

	dir := t.TempDir()
	p1 := sciconfig.Profile{ID: "p1", Name: "Active", TemplateID: "custom", APIKey: "sk-active", BaseURL: okSrv.URL}
	p2 := sciconfig.Profile{ID: "p2", Name: "Next", TemplateID: "custom", APIKey: "sk-next", BaseURL: okSrv.URL}
	// Port 8765 is reserved — probe passes but StartProxy must fail.
	writeSwitchFixture(t, dir, []sciconfig.Profile{p1, p2}, "p1", 8765, 8990)

	mgr, err := New(dir, &config.File{})
	if err != nil {
		t.Fatal(err)
	}

	_, err = mgr.SwitchProfile("p2")
	if err == nil {
		t.Fatal("expected rollback error")
	}
	if !strings.Contains(err.Error(), "回滚") && !strings.Contains(err.Error(), "8765") {
		t.Fatalf("unexpected: %v", err)
	}
	cfg, _ := sciconfig.Load(dir)
	if cfg.ActiveProfileID != "p1" {
		t.Fatalf("rollback failed: active=%q", cfg.ActiveProfileID)
	}
}

func TestProbeProfileKeyRejects401(t *testing.T) {
	dir := t.TempDir()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer srv.Close()

	p := sciconfig.Profile{
		ID: "p1", Name: "bad", TemplateID: "custom", APIKey: "sk-bad", BaseURL: srv.URL,
	}
	cfg := sciconfig.Default()
	cfg.SchemaVersion = sciconfig.CurrentSchemaVersion
	cfg.Profiles = []sciconfig.Profile{p}
	cfg.ActiveProfileID = p.ID
	if err := sciconfig.Save(dir, cfg); err != nil {
		t.Fatal(err)
	}

	ok, hint, err := ProbeProfileKey(dir, "p1")
	if err != nil {
		t.Fatal(err)
	}
	if ok {
		t.Fatal("expected reject")
	}
	if hint == "" {
		t.Fatal("want hint")
	}
}
