package fetcher

import "net/http"

var client = &http.Client{
	// BUG: no timeout
}

func Fetch(url string) (*http.Response, error) {
	return client.Get(url)
}
