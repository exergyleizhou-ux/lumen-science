package proxy

import (
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
	"time"
)

var blockedHostSuffixes = []string{
	"anthropic.com",
	"claude.ai",
	"claude.com",
}

func isBlockedHost(host string) bool {
	h := strings.ToLower(strings.TrimSuffix(host, "."))
	for _, s := range blockedHostSuffixes {
		if h == s || strings.HasSuffix(h, "."+s) {
			return true
		}
	}
	return false
}

func (s *Server) handleConnect(w http.ResponseWriter, r *http.Request) {
	target := r.Host
	if target == "" {
		target = r.URL.Host
	}
	host := target
	if i := strings.LastIndex(host, ":"); i >= 0 {
		host = host[:i]
	}
	host = strings.Trim(host, "[]")

	if isBlockedHost(host) {
		s.log(fmt.Sprintf("CONNECT %s -> 401 logged-out fast-fail", target))
		connectReply(w, http.StatusUnauthorized)
		return
	}

	port := "443"
	if i := strings.LastIndex(target, ":"); i >= 0 {
		port = target[i+1:]
	}
	upstream, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 10*time.Second)
	if err != nil {
		s.log(fmt.Sprintf("CONNECT %s -> 502 upstream unreachable: %v", target, err))
		connectReply(w, http.StatusBadGateway)
		return
	}

	w.WriteHeader(http.StatusOK)
	if hj, ok := w.(http.Hijacker); ok {
		clientConn, _, err := hj.Hijack()
		if err != nil {
			upstream.Close()
			return
		}
		s.log(fmt.Sprintf("CONNECT %s -> tunnel established", target))
		go tunnel(clientConn, upstream)
		return
	}
	upstream.Close()
	connectReply(w, http.StatusInternalServerError)
}

func connectReply(w http.ResponseWriter, code int) {
	w.Header().Set("Content-Length", "0")
	w.Header().Set("Connection", "close")
	w.WriteHeader(code)
}

func tunnel(client, upstream net.Conn) {
	defer client.Close()
	defer upstream.Close()
	done := make(chan struct{}, 2)
	copyBoth := func(dst, src net.Conn) {
		_, _ = io.Copy(dst, src)
		done <- struct{}{}
	}
	go copyBoth(upstream, client)
	go copyBoth(client, upstream)
	<-done
}
