package proxy

import (
	"context"
	"net"
	"net/http"
	"testing"
	"time"
)

func reserveLoopbackAddr(t *testing.T) string {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	addr := ln.Addr().String()
	if err := ln.Close(); err != nil {
		t.Fatal(err)
	}
	return addr
}

func startProxyServer(t *testing.T, cfg Config) (addr string, srv *Server) {
	t.Helper()
	if cfg.Addr == "" {
		cfg.Addr = reserveLoopbackAddr(t)
	}
	var err error
	srv, err = New(cfg)
	if err != nil {
		t.Fatal(err)
	}
	done := make(chan struct{})
	go func() {
		defer close(done)
		_ = srv.ListenAndServe()
	}()
	t.Cleanup(func() {
		ctx, c := context.WithTimeout(context.Background(), 2*time.Second)
		defer c()
		_ = srv.Shutdown(ctx)
		<-done
	})
	healthPath := "/health"
	if cfg.AuthSecret != "" {
		healthPath = "/" + cfg.AuthSecret + healthPath
	}
	for i := 0; i < 50; i++ {
		resp, err := http.Get("http://" + cfg.Addr + healthPath)
		if err == nil {
			resp.Body.Close()
			if resp.StatusCode == http.StatusOK || resp.StatusCode == http.StatusForbidden {
				return cfg.Addr, srv
			}
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal("proxy did not become ready")
	return "", nil
}
