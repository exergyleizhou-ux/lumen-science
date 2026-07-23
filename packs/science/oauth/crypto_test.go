package oauth

import "testing"

func TestEncryptDecryptRoundtrip(t *testing.T) {
	key, err := randBytes(32)
	if err != nil {
		t.Fatal(err)
	}
	keyB64 := base64Std(key)
	pt := []byte(`{"email":"virtual@localhost.invalid","x":1}`)
	body, err := encryptTokenV2(pt, keyB64)
	if err != nil {
		t.Fatal(err)
	}
	if !stringsHasPrefix(body, "v2:") {
		t.Fatalf("expected v2: prefix, got %q", body[:10])
	}
	back, err := decryptTokenV2(body, keyB64)
	if err != nil {
		t.Fatal(err)
	}
	if string(back) != string(pt) {
		t.Fatalf("roundtrip mismatch")
	}
}

func TestDecryptWrongKey(t *testing.T) {
	k1 := base64Std(randBytesMust(32))
	k2 := base64Std(randBytesMust(32))
	body, err := encryptTokenV2([]byte("hello"), k1)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := decryptTokenV2(body, k2); err == nil {
		t.Fatal("expected decrypt failure with wrong key")
	}
}

func stringsHasPrefix(s, prefix string) bool {
	return len(s) >= len(prefix) && s[:len(prefix)] == prefix
}
