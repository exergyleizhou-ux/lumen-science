// Package brief builds a provenance-first scientific literature brief.
package brief

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"sort"
	"strings"
	"time"
)

const (
	defaultPubMedBase = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils"
	defaultChEMBLBase = "https://www.ebi.ac.uk/chembl/api/data"
	maxResponseBytes  = 4 << 20
)

// Article is a PubMed result with enough metadata for source verification.
type Article struct {
	PMID    string
	Title   string
	Journal string
	PubDate string
	Authors []string
}

// Compound is a ChEMBL molecule result related to the requested topic.
type Compound struct {
	ID       string
	Name     string
	MaxPhase int
}

// Result is the evidence collected for one research topic.
type Result struct {
	Topic     string
	FetchedAt time.Time
	Articles  []Article
	Compounds []Compound
	Warnings  []string
}

// Client owns the HTTP and endpoint dependencies used to generate a brief.
type Client struct {
	httpClient *http.Client
	pubMedBase string
	chemblBase string
	now        func() time.Time
	userAgent  string
	ncbiAPIKey string
}

// NewClient returns a client with bounded network timeouts and public API endpoints.
func NewClient(timeout time.Duration) *Client {
	if timeout <= 0 {
		timeout = 30 * time.Second
	}
	return &Client{
		httpClient: &http.Client{Timeout: timeout},
		pubMedBase: defaultPubMedBase,
		chemblBase: defaultChEMBLBase,
		now:        time.Now,
		userAgent:  "lumen-science/0.1 (research brief; contact: local-user)",
		ncbiAPIKey: os.Getenv("NCBI_API_KEY"),
	}
}

// Generate fetches PubMed as the required evidence source and ChEMBL as an
// optional enrichment source. It returns an error instead of inventing evidence.
func (c *Client) Generate(ctx context.Context, topic string, maxArticles, maxCompounds int) (Result, error) {
	topic = strings.TrimSpace(topic)
	if topic == "" {
		return Result{}, fmt.Errorf("topic is required")
	}
	if maxArticles <= 0 {
		maxArticles = 5
	}
	if maxCompounds <= 0 {
		maxCompounds = 3
	}

	articles, err := c.fetchPubMed(ctx, topic, maxArticles)
	if err != nil {
		return Result{}, fmt.Errorf("fetch PubMed evidence: %w", err)
	}
	result := Result{
		Topic:     topic,
		FetchedAt: c.now().UTC(),
		Articles:  articles,
	}
	compounds, err := c.fetchChEMBL(ctx, topic, maxCompounds)
	if err != nil {
		result.Warnings = append(result.Warnings, "ChEMBL enrichment unavailable: "+err.Error())
	} else {
		result.Compounds = compounds
	}
	return result, nil
}

func (c *Client) fetchPubMed(ctx context.Context, topic string, limit int) ([]Article, error) {
	searchValues := url.Values{
		"db":      {"pubmed"},
		"retmode": {"json"},
		"retmax":  {fmt.Sprint(limit)},
		"term":    {topic},
	}
	if c.ncbiAPIKey != "" {
		searchValues.Set("api_key", c.ncbiAPIKey)
	}
	searchURL, err := addQuery(c.pubMedBase+"/esearch.fcgi", searchValues)
	if err != nil {
		return nil, err
	}
	var search struct {
		Result struct {
			IDs []string `json:"idlist"`
		} `json:"esearchresult"`
	}
	if err := c.getJSON(ctx, searchURL, &search); err != nil {
		return nil, fmt.Errorf("search: %w", err)
	}
	if len(search.Result.IDs) == 0 {
		return []Article{}, nil
	}

	summaryValues := url.Values{
		"db":      {"pubmed"},
		"retmode": {"json"},
		"id":      {strings.Join(search.Result.IDs, ",")},
	}
	if c.ncbiAPIKey != "" {
		summaryValues.Set("api_key", c.ncbiAPIKey)
	}
	summaryURL, err := addQuery(c.pubMedBase+"/esummary.fcgi", summaryValues)
	if err != nil {
		return nil, err
	}
	var summary struct {
		Result map[string]json.RawMessage `json:"result"`
	}
	if err := c.getJSON(ctx, summaryURL, &summary); err != nil {
		return nil, fmt.Errorf("summary: %w", err)
	}
	articles := make([]Article, 0, len(search.Result.IDs))
	for _, pmid := range search.Result.IDs {
		raw, ok := summary.Result[pmid]
		if !ok {
			continue
		}
		var item struct {
			Title   string `json:"title"`
			Journal string `json:"fulljournalname"`
			Source  string `json:"source"`
			PubDate string `json:"pubdate"`
			Authors []struct {
				Name string `json:"name"`
			} `json:"authors"`
		}
		if err := json.Unmarshal(raw, &item); err != nil {
			continue
		}
		authors := make([]string, 0, len(item.Authors))
		for _, author := range item.Authors {
			if name := strings.TrimSpace(author.Name); name != "" {
				authors = append(authors, name)
			}
		}
		journal := strings.TrimSpace(item.Journal)
		if journal == "" {
			journal = strings.TrimSpace(item.Source)
		}
		articles = append(articles, Article{
			PMID: pmid, Title: cleanLine(item.Title), Journal: cleanLine(journal),
			PubDate: cleanLine(item.PubDate), Authors: authors,
		})
	}
	return articles, nil
}

func (c *Client) fetchChEMBL(ctx context.Context, topic string, limit int) ([]Compound, error) {
	searchURL, err := addQuery(c.chemblBase+"/molecule/search.json", url.Values{
		"q":     {topic},
		"limit": {fmt.Sprint(limit)},
	})
	if err != nil {
		return nil, err
	}
	var payload struct {
		Molecules []struct {
			ID       string `json:"molecule_chembl_id"`
			Name     string `json:"pref_name"`
			MaxPhase any    `json:"max_phase"`
		} `json:"molecules"`
	}
	if err := c.getJSON(ctx, searchURL, &payload); err != nil {
		return nil, err
	}
	compounds := make([]Compound, 0, len(payload.Molecules))
	for _, molecule := range payload.Molecules {
		phase := 0
		switch value := molecule.MaxPhase.(type) {
		case float64:
			phase = int(value)
		case string:
			_, _ = fmt.Sscanf(value, "%d", &phase)
		}
		compounds = append(compounds, Compound{
			ID: cleanLine(molecule.ID), Name: cleanLine(molecule.Name), MaxPhase: phase,
		})
	}
	return compounds, nil
}

func (c *Client) getJSON(ctx context.Context, endpoint string, destination any) error {
	displayEndpoint := redactAPIKey(endpoint)
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Accept", "application/json")
	req.Header.Set("User-Agent", c.userAgent)
	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("GET %s: %w", displayEndpoint, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, readErr := io.ReadAll(io.LimitReader(resp.Body, 1024))
		if readErr != nil {
			return fmt.Errorf("GET %s: HTTP %d (read error: %v)", displayEndpoint, resp.StatusCode, readErr)
		}
		return fmt.Errorf("GET %s: HTTP %d: %s", displayEndpoint, resp.StatusCode, clip(cleanLine(string(body)), 240))
	}
	decoder := json.NewDecoder(io.LimitReader(resp.Body, maxResponseBytes))
	if err := decoder.Decode(destination); err != nil {
		return fmt.Errorf("decode %s: %w", displayEndpoint, err)
	}
	return nil
}

func addQuery(rawURL string, values url.Values) (string, error) {
	parsed, err := url.Parse(rawURL)
	if err != nil {
		return "", fmt.Errorf("parse endpoint %q: %w", rawURL, err)
	}
	query := parsed.Query()
	for key, items := range values {
		for _, item := range items {
			query.Add(key, item)
		}
	}
	parsed.RawQuery = query.Encode()
	return parsed.String(), nil
}

func cleanLine(value string) string {
	return strings.Join(strings.Fields(value), " ")
}

func clip(value string, limit int) string {
	if len(value) <= limit {
		return value
	}
	return value[:limit] + "..."
}

func redactAPIKey(endpoint string) string {
	parsed, err := url.Parse(endpoint)
	if err != nil {
		return "<invalid endpoint>"
	}
	query := parsed.Query()
	if query.Has("api_key") {
		query.Set("api_key", "REDACTED")
		parsed.RawQuery = query.Encode()
	}
	return parsed.String()
}

// Markdown renders source links and collected metadata without synthesizing
// unsupported medical conclusions.
func (r Result) Markdown() string {
	var out strings.Builder
	fmt.Fprintf(&out, "# Science brief: %s\n\n", r.Topic)
	fmt.Fprintf(&out, "- Fetched at: `%s`\n", r.FetchedAt.Format(time.RFC3339))
	fmt.Fprintf(&out, "- Evidence policy: source metadata only; no medical advice or model-generated claims\n\n")

	out.WriteString("## PubMed evidence\n\n")
	if len(r.Articles) == 0 {
		out.WriteString("No PubMed records were returned for this query.\n\n")
	}
	for _, article := range r.Articles {
		title := article.Title
		if title == "" {
			title = "Untitled PubMed record"
		}
		fmt.Fprintf(&out, "- [%s](https://pubmed.ncbi.nlm.nih.gov/%s/) — PMID %s", title, url.PathEscape(article.PMID), article.PMID)
		metadata := make([]string, 0, 3)
		if article.Journal != "" {
			metadata = append(metadata, article.Journal)
		}
		if article.PubDate != "" {
			metadata = append(metadata, article.PubDate)
		}
		if len(article.Authors) > 0 {
			metadata = append(metadata, strings.Join(article.Authors, ", "))
		}
		if len(metadata) > 0 {
			fmt.Fprintf(&out, " (%s)", strings.Join(metadata, "; "))
		}
		out.WriteString("\n")
	}

	out.WriteString("\n## ChEMBL related compounds\n\n")
	if len(r.Compounds) == 0 {
		out.WriteString("No ChEMBL compounds were returned.\n")
	}
	for _, compound := range r.Compounds {
		name := compound.Name
		if name == "" {
			name = compound.ID
		}
		fmt.Fprintf(&out, "- [%s](https://www.ebi.ac.uk/chembl/explore/compound/%s) — %s, max phase %d\n", name, url.PathEscape(compound.ID), compound.ID, compound.MaxPhase)
	}

	if len(r.Warnings) > 0 {
		sort.Strings(r.Warnings)
		out.WriteString("\n## Warnings\n\n")
		for _, warning := range r.Warnings {
			fmt.Fprintf(&out, "- %s\n", warning)
		}
	}

	out.WriteString("\n## Provenance\n\n")
	fmt.Fprintf(&out, "- PubMed query: `%s`\n", r.Topic)
	fmt.Fprintf(&out, "- ChEMBL search: `%s`\n", r.Topic)
	return out.String()
}
