package runtime

import (
	"testing"
)

func TestKeyFingerprint(t *testing.T) {
	a := KeyFingerprint("sk-aaa")
	b := KeyFingerprint("sk-aaa")
	c := KeyFingerprint("sk-bbb")
	if a != b {
		t.Fatal("same key should fingerprint equally")
	}
	if a == c {
		t.Fatal("different keys should differ")
	}
}

func TestProxyHealthyMissing(t *testing.T) {
	if proxyHealthy(59999, "secret") {
		t.Fatal("expected unhealthy on unused high port")
	}
}
