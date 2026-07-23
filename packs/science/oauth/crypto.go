package oauth

import (
	"crypto/aes"
	"crypto/cipher"
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"fmt"
	"io"
)

const (
	hkdfInfo = "operon:aes-256-gcm:oauth"
	aadV2    = "v2:oauth"
)

func deriveKey(oauthKeyB64 string) ([32]byte, error) {
	var zero [32]byte
	ikm, err := base64.StdEncoding.DecodeString(oauthKeyB64)
	if err != nil {
		return zero, fmt.Errorf("OAUTH_ENCRYPTION_KEY invalid base64: %w", err)
	}
	return hkdfSHA256(ikm, nil, []byte(hkdfInfo), 32)
}

// hkdfSHA256 implements HKDF-SHA256 expand with empty salt (Node hkdfSync salt=Buffer.alloc(0)).
func hkdfSHA256(ikm, salt, info []byte, length int) ([32]byte, error) {
	var out [32]byte
	if length != 32 {
		return out, fmt.Errorf("only 32-byte expand supported")
	}
	if salt == nil {
		salt = make([]byte, sha256.Size)
	}
	prk := hmac.New(sha256.New, salt)
	prk.Write(ikm)
	prkSum := prk.Sum(nil)

	var t []byte
	h := hmac.New(sha256.New, prkSum)
	h.Write(t)
	h.Write(info)
	h.Write([]byte{1})
	copy(out[:], h.Sum(nil))
	return out, nil
}

func encryptTokenV2(plaintext []byte, oauthKeyB64 string) (string, error) {
	key, err := deriveKey(oauthKeyB64)
	if err != nil {
		return "", err
	}
	iv := make([]byte, 12)
	if _, err := io.ReadFull(rand.Reader, iv); err != nil {
		return "", err
	}
	block, err := aes.NewCipher(key[:])
	if err != nil {
		return "", err
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return "", err
	}
	ct := gcm.Seal(nil, iv, plaintext, []byte(aadV2))
	framed := append(append([]byte{}, iv...), ct...)
	return "v2:" + base64.StdEncoding.EncodeToString(framed), nil
}

func decryptTokenV2(body, oauthKeyB64 string) ([]byte, error) {
	raw, err := base64.StdEncoding.DecodeString(body[3:]) // strip "v2:"
	if err != nil {
		return nil, fmt.Errorf("v2 body invalid base64: %w", err)
	}
	if len(raw) < 12+16 {
		return nil, fmt.Errorf("v2 ciphertext too short")
	}
	key, err := deriveKey(oauthKeyB64)
	if err != nil {
		return nil, err
	}
	iv, rest := raw[:12], raw[12:]
	block, err := aes.NewCipher(key[:])
	if err != nil {
		return nil, err
	}
	gcm, err := cipher.NewGCM(block)
	if err != nil {
		return nil, err
	}
	return gcm.Open(nil, iv, rest, []byte(aadV2))
}
