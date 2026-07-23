package native

import "testing"

func TestRequiredAuthCombo(t *testing.T) {
	cases := []struct {
		tool string
		want AuthLevel
	}{
		{"search_datasets", AuthAnonymous},
		{"get_dataset_detail", AuthAnonymous},
		{"preview_schema", AuthUserToken},
		{"list_algorithms", AuthUserToken},
		{"list_offer_signals", AuthAnonymous},
		{"submit_c2d_job", AuthUserToken},
		{"unknown_tool", AuthUserToken},
	}
	for _, tc := range cases {
		if got := RequiredAuth(tc.tool); got != tc.want {
			t.Fatalf("%s: got %v want %v", tc.tool, got, tc.want)
		}
	}
}
